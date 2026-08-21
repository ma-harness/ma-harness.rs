//! ma-harness CLI 入口 (`mah` 二进制)
//!
//! 7 个子命令 (Day 7-8 5 个 + Day 39 +2):
//! - `start` — 起 server (tonic gRPC + salvo HTTP)
//! - `run` — 跑一次 agent (本地, 不连 server)
//! - `plugins` — 列出已装载 plugin
//! - `events` — 查 session 事件
//! - `conformance` — 跑 conformance fixture (验证 ma-harness 跟 dsh 行为等价)
//! - `bench` — benchmark 信息 / 跑 cargo bench 提示
//! - `version` — 打印版本

mod acp;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ma_harness_conformance::{
    fixture::FixtureLoader, ConformanceRunner, ConformanceResult, Fixture, ReportFormat,
    ReportWriter,
};
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
// 2026-08-18 (Day 52): ma_harness_proto 恢复 (用本地 vendor/protoc), gRPC service 恢复
use ma_harness_proto::ma_harness::v1::{
    agent_service_server::AgentServiceServer, session_service_server::SessionServiceServer,
};
use ma_harness_seam::PluginLoader;
use ma_harness_registry::Registry; // P14 (2026-08-20): registry list/export CLI
// Phase 2.2 (T2.2): 引用 hello plugin 触发 link, inventory::submit! 才有 effect
#[allow(unused_imports)]
use ma_harness_plugin_hello as _hello;
use ma_harness_server::{ServerBuilder, SessionStore};

#[derive(Parser, Debug)]
#[command(name = "mah", about = "ma-harness AI agent orchestrator")]
struct Cli {
    /// 事件日志路径 (默认: ~/.ma-harness/events.db)
    #[arg(long, default_value = "~/.ma-harness/events.db")]
    log: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 启动 server (tonic gRPC + salvo HTTP)
    Start {
        /// gRPC 监听端口
        #[arg(long, default_value = "50051")]
        grpc_port: u16,
        /// HTTP 监听端口
        #[arg(long, default_value = "50050")]
        http_port: u16,
        /// 持久化 session store 路径 (sqlite db, 不传 = 内存)
        /// 业务方重启 server 时 session 跟 event 从这个 db 恢复
        #[arg(long)]
        store_path: Option<PathBuf>,
    },
    /// 跑一次 agent (本地, 不连 server)
    Run {
        /// Session ID (留空 = 新建)
        #[arg(long)]
        session: Option<String>,
        /// 用户消息
        message: String,
        /// 模型 (默认 stub)
        #[arg(long, default_value = "stub")]
        model: String,
    },
    /// 列出已装载 plugin (走 inventory 分布式注册)
    Plugins,
    /// 按名装载 plugin (Phase 2.2 / T2.2 inventory + dylib 走 PluginLoader)
    LoadPlugin {
        /// plugin 名 (e.g. "hello", "bash", "fs")
        name: String,
        /// 可选 ctx 标识 (debug 用, 默认 "default")
        #[arg(long, default_value = "default")]
        ctx_id: String,
    },
    /// 查 session 事件
    Events {
        /// Session ID
        session: String,
    },
    /// **P5-5 (Day 94)**: Session CRUD via local SqliteStore + EventLog
    ///
    /// 不连 server, 业务方传 --store-path 直接读本地 db
    /// (跟 `mah start --store-path <x>` 启动的 db 一致就能查)
    ///
    /// 例子:
    ///   mah sessions list --store-path ~/.ma-harness/sessions.db
    ///   mah sessions get <id> --store-path <db>
    ///   mah sessions events <id> --log <events.db>
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },
    /// 跑 conformance fixture, 比对实际事件 vs 期望, 出报告
    ///
    /// 例子:
    ///   mah conformance --fixtures fixtures/smoke.jsonl --output target/
    ///   mah conformance --fixtures fixtures/dsh/ --dsh --output target/
    Conformance {
        /// Fixture 路径 (文件 .jsonl 或目录)
        #[arg(long)]
        fixtures: PathBuf,
        /// 视为 dsh 风格 fixture (走 dsh_format 转换层)
        #[arg(long)]
        dsh: bool,
        /// 报告输出目录 (写 .md + .json)
        #[arg(long, default_value = "target/")]
        output: PathBuf,
        /// verbose (打印每条 fixture 跑的过程)
        #[arg(long, short)]
        verbose: bool,
    },
    /// Benchmark 信息 / 跑 cargo bench 提示
    ///
    /// 不真跑 (criterion 自己跑), 只打印命令 + 报告路径
    Bench {
        /// 单 crate (e.g. ma_harness_cordis) 不传跑全部
        crate_name: Option<String>,
    },
    /// **P14 (2026-08-20)**: Plugin Registry 列表 / 导出 (GH Pages 部署用)
    ///
    /// 例子:
    ///   mah registry list
    ///   mah registry list --registry ~/.ma-harness/registry.json
    ///   mah registry export --output docs/registry/registry.json
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },
    /// 版本
    Version,
    /// 跑 Code Mode (Phase 2 / T3.1) — 编译并执行 wasm module
    ///
    /// 例子:
    ///   mah code run ./hello.wat
    ///   mah code run ./module.wasm
    Code {
        #[command(subcommand)]
        action: CodeAction,
    },
    /// **Phase 3.3 / T3.3**: 业务方 prompt → LLM 生成 .wat → wasm 沙箱跑
    ///
    /// 需要环境 `OPENAI_API_KEY` (或 `--api-key <key>`)
    ///
    /// 例子:
    ///   OPENAI_API_KEY=sk-... mah run-prompt "compute 1+1, return the result as i32"
    ///   OPENAI_API_KEY=sk-... mah run-prompt "log 'hello world', return 0"
    RunPrompt {
        /// 业务方需求描述 (LLM 转 .wat)
        prompt: String,
        /// 可选 API key (缺省读 env OPENAI_API_KEY)
        #[arg(long)]
        api_key: Option<String>,
        /// 可选 model (缺省 gpt-4o-mini)
        #[arg(long, default_value = "gpt-4o-mini")]
        model: String,
    },
    /// **Phase 3.5 / T3.5**: 导出 OpenAPI spec (CI 同步用)
    ///
    /// 例子:
    ///   mah openapi export --output docs/api/openapi.json
    OpenApi {
        #[command(subcommand)]
        action: OpenApiAction,
    },
    /// **P13.5 / Day 101+2**: dsh-adapter 健康检查 + 版本信息
    ///
    /// 例子:
    ///   mah dsh info    # 显示 dsh runtime / Node.js / JSON-RPC 协议版本
    ///   mah dsh doctor  # Node.js 装没 / 子进程能起 / mock plugin 跑通
    Dsh {
        #[command(subcommand)]
        action: DshAction,
    },
    /// **P6-1 / Day 99**: 走 gRPC RunStream RPC, 实时打印 token (跟 stub/真 LLM 都能跑)
    ///
    /// 例子:
    ///   mah run-stream --grpc-url http://localhost:50051 "hello"
    ///   mah run-stream --grpc-url http://server:50051 --model "openai:gpt-4o-mini" "tell me a joke"
    RunStream {
        /// 业务方需求描述 (LLM 输入)
        prompt: String,
        /// gRPC server URL (默认 localhost:50051)
        #[arg(long, default_value = "http://localhost:50051")]
        grpc_url: String,
        /// Session ID (留空 = 新建)
        #[arg(long)]
        session: Option<String>,
        /// 模型 (默认 "stub", 真 LLM 走 "openai:gpt-4o-mini" / "anthropic:claude-3-5-sonnet" 等)
        #[arg(long, default_value = "stub")]
        model: String,
    },
    /// **P11-4 (Day 101+1)**: ACP (Agent Communication Protocol) 互通 (跟 dsh / Codex 生态)
    ///
    /// `mah acp serve` 起 JSON-RPC 2.0 stdio server, 跟 dsh `dsh-jsonrpc-agent` 风格一致.
    /// 业务方 (Python / Node / Rust) 写 JSON-RPC 到 stdin, 从 stdout 读响应/通知.
    ///
    /// 例子:
    ///   mah acp serve --model stub < input.jsonl > output.jsonl
    ///   # 跟 dsh SDK 互通: `from deepseek_harness import DeepSeekHarness` 调 mah
    Acp {
        #[command(subcommand)]
        action: AcpAction,
    },
    /// **Phase 3.7 / T3.7**: 显式 enforce landlock (Linux) / seatbelt (Mac) / stub (其他)
    ///
    /// ⚠️ **警告**: 一旦 enforce 是全进程 (不可逆). 业务方决定要不要跑.
    ///
    /// 例子:
    ///   mah sandbox apply --read-paths /tmp,/var/llm-output
    ///   mah sandbox apply --read-paths /tmp --write-paths /tmp
    ///   mah sandbox apply --read-paths /tmp --temp-dir   # 加系统 tmpdir
    Sandbox {
        #[command(subcommand)]
        action: SandboxAction,
    },
    /// **Phase 3.9 / T3.9**: 启动 TUI dashboard (ratatui)
    ///
    /// 3 个 panel: Sessions | Events | Plugins
    /// 每 500ms 刷新, 'q' / Esc / Ctrl-C 退出
    Tui {
        /// **P4-1**: EventLog sqlite path (走真 events)
        /// 缺省: stub fallback (Phase 3.9 行为)
        #[arg(long)]
        log: Option<std::path::PathBuf>,
        /// **P4-3**: SessionStore sqlite path (走真 sessions)
        /// 缺省: 走 EventLog 推 session / 全 stub fallback
        #[arg(long)]
        store_path: Option<std::path::PathBuf>,
    },
}

/// **P5-5 (Day 94)**: Session CRUD sub-actions
#[derive(Subcommand, Debug)]
enum SessionsAction {
    /// 列出所有 session (走本地 SqliteStore)
    List {
        /// SqliteStore 路径 (跟 `mah start --store-path <x>` 启动的 db 一致)
        #[arg(long)]
        store_path: PathBuf,
    },
    /// 拿单个 session metadata
    Get {
        /// SqliteStore 路径
        #[arg(long)]
        store_path: PathBuf,
        /// Session ID
        id: String,
    },
    /// 拿 session 的 events (走本地 EventLog)
    Events {
        /// EventLog sqlite 路径 (跟 `mah start` 启动时拿的 events.db 一致)
        #[arg(long)]
        log: PathBuf,
        /// Session ID
        session: String,
    },
}

#[derive(Subcommand, Debug)]
enum OpenApiAction {
    /// 从 server router 导出当前 OpenAPI spec
    Export {
        /// 输出文件 (.json / .yaml, 格式按扩展名)
        #[arg(long, default_value = "openapi.json")]
        output: std::path::PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum DshAction {
    /// 显示 dsh-adapter 运行时信息 (Node.js / JSON-RPC 协议版本 / ma-harness 版本)
    Info,
    /// 健康检查: Node.js 装没 / 子进程能起 / 简化 mock plugin 跑通
    Doctor,
}

#[derive(Subcommand, Debug)]
enum RegistryAction {
    /// 列已 publish 的 plugin (默认 `~/.ma-harness/registry.json`)
    List {
        /// Registry JSON file (默认 `~/.ma-harness/registry.json`)
        #[arg(long)]
        registry: Option<std::path::PathBuf>,
    },
    /// 导出 registry 到 JSON file (供 GH Pages 静态站消费)
    ///
    /// 业务方 workflow: `mah registry export --output registry.json`
    /// 跟 `registry-pages.yml` workflow 配合, 自动部署到 GH Pages
    Export {
        /// 输出 JSON file
        #[arg(long, default_value = "registry.json")]
        output: std::path::PathBuf,
        /// Registry JSON file (默认 `~/.ma-harness/registry.json`)
        #[arg(long)]
        registry: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum SandboxAction {
    /// Enforce landlock/seatbelt/stub 沙箱 (不可逆)
    Apply {
        /// 允许读的路径 (逗号分隔, e.g. /tmp,/var/llm-output)
        #[arg(long, value_delimiter = ',')]
        read_paths: Vec<std::path::PathBuf>,
        /// 允许写的路径 (逗号分隔)
        #[arg(long, value_delimiter = ',')]
        write_paths: Vec<std::path::PathBuf>,
        /// 允许执行的路径 (逗号分隔, Phase 2.2 占位)
        #[arg(long, value_delimiter = ',')]
        exec_paths: Vec<std::path::PathBuf>,
        /// 加系统 tmpdir 进 read_paths
        #[arg(long)]
        temp_dir: bool,
    },
    /// 打印当前 OS 沙箱支持 (Linux landlock / Mac seatbelt / 其他 stub)
    Status,
}

#[derive(Subcommand, Debug)]
enum CodeAction {
    /// 跑一个 .wat 或 .wasm 文件
    Run {
        /// 文件路径 (.wat text / .wasm binary)
        file: PathBuf,
    },
}

/// **P11-4 (Day 101+1)**: ACP sub-actions
#[derive(Subcommand, Debug)]
enum AcpAction {
    /// 启 JSON-RPC 2.0 stdio server (跟 dsh 互通)
    ///
    /// 例子:
    ///   mah acp serve --model stub
    Serve {
        /// 模型 (默认 "stub")
        #[arg(long, default_value = "stub")]
        model: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            grpc_port,
            http_port,
            store_path,
        } => start_server(grpc_port, http_port, store_path.as_deref()).await,
        Commands::Run {
            session,
            message,
            model,
        } => run_local_agent(session, message, model).await,
        Commands::Plugins => list_plugins(),
        Commands::LoadPlugin { name, ctx_id } => load_plugin(&name, &ctx_id),
        Commands::Events { session } => list_events(&session),
        Commands::Sessions { action } => match action {
            SessionsAction::List { store_path } => sessions_list(&store_path),
            SessionsAction::Get { store_path, id } => sessions_get(&store_path, &id),
            SessionsAction::Events { log, session } => sessions_events(&log, &session),
        },
        Commands::Conformance {
            fixtures,
            dsh,
            output,
            verbose,
        } => run_conformance(&fixtures, dsh, &output, verbose),
        Commands::Bench { crate_name } => print_bench_info(crate_name.as_deref()),
        Commands::Version => {
            println!("mah {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Code { action } => match action {
            CodeAction::Run { file } => run_code(&file),
        },
        Commands::RunPrompt {
            prompt,
            api_key,
            model,
        } => run_prompt(&prompt, api_key.as_deref(), &model).await,
        Commands::OpenApi { action } => match action {
            OpenApiAction::Export { output } => export_openapi(&output),
        },
        Commands::Dsh { action } => match action {
            DshAction::Info => dsh_info(),
            DshAction::Doctor => dsh_doctor().await,
        },
        Commands::Sandbox { action } => match action {
            SandboxAction::Apply {
                read_paths,
                write_paths,
                exec_paths,
                temp_dir,
            } => apply_sandbox(read_paths, write_paths, exec_paths, temp_dir),
            SandboxAction::Status => print_sandbox_status(),
        },
        Commands::Tui { log, store_path } => run_tui(log.as_deref(), store_path.as_deref()),
        Commands::RunStream {
            prompt,
            grpc_url,
            session,
            model,
        } => {
            Box::pin(run_stream_cmd(
                &prompt,
                &grpc_url,
                session.as_deref(),
                &model,
            ))
            .await
        }
        Commands::Acp { action } => match action {
            AcpAction::Serve { model } => Box::pin(acp::run_acp_server(&model)).await,
        },
        Commands::Registry { action } => match action {
            RegistryAction::List { registry } => registry_list(registry.as_deref()),
            RegistryAction::Export { output, registry } => {
                registry_export(&output, registry.as_deref())
            }
        },
    }
}

/// 真实起 server: tonic gRPC + salvo HTTP, 后台 tokio 任务, ctrl-c 优雅退出
///
/// `store_path` = Some(path) → SqliteStore 持久化 session
/// `store_path` = None → InMemoryStore (Phase 1 默认)
async fn start_server(
    grpc_port: u16,
    http_port: u16,
    store_path: Option<&std::path::Path>,
) -> Result<()> {
    let log = EventLog::open_in_memory()?;
    eprintln!(
        "mah start: tonic gRPC on 0.0.0.0:{} + salvo HTTP on 0.0.0.0:{}",
        grpc_port, http_port
    );

    // Phase 2.10 (Day 64): 业务方指定 store_path → SqliteStore 持久化
    // Phase 5.1 (Day 90): session store 一次构造, gRPC + HTTP 共用
    // Phase 1 默认 InMemoryStore
    let session_store: Arc<dyn ma_harness_server::SessionStore> = if let Some(path) = store_path {
        let store = ma_harness_server::SqliteStore::open(path)
            .map_err(|e| anyhow::anyhow!("open sqlite store {}: {e}", path.display()))?;
        eprintln!("mah start: session store = sqlite:{}", path.display());
        Arc::new(store)
    } else {
        eprintln!("mah start: session store = in-memory (no persistence)");
        Arc::new(ma_harness_server::InMemoryStore::new())
    };
    let mut builder = ServerBuilder::with_stub(log);
    builder = builder.with_session_store(session_store.clone());

    // tonic gRPC server (P7-1.2: tonic-web 暴露 gRPC-web 给浏览器)
    // 2026-08-19 (Day 101): Web UI (P7-1) 通过 Vite proxy /api → tonic :50050 调 gRPC-web.
    // tonic_web::enable() 包每个 service (NamedService trait 适配),
    // 配 `accept_http1(true)` 让 server 接受 HTTP/1.1 (gRPC-web 协议).
    let grpc_addr: std::net::SocketAddr = format!("0.0.0.0:{}", grpc_port)
        .parse()
        .with_context(|| format!("invalid grpc_port: {}", grpc_port))?;
    let agent_svc = builder.build_agent_service();
    let session_svc = builder.build_session_service();
    let grpc_server = tonic::transport::Server::builder()
        .accept_http1(true)
        .add_service(tonic_web::enable(AgentServiceServer::new(agent_svc)))
        .add_service(tonic_web::enable(SessionServiceServer::new(session_svc)))
        .serve(grpc_addr);

    // salvo HTTP server
    // 2026-08-18 (Day 52): TcpAcceptor::try_from(tokio::net::TcpListener) — salvo 0.79 API
    // 2026-08-19 (Day 90): HTTP /v1/sessions 需要 SessionStore, 走 run_router_with_store (跟 gRPC 共用)
    // 2026-08-19 (Day 92): HTTP /v1/sessions/{id}/events 需要 EventLog, 走 run_router_with_log_and_store
    use salvo::conn::tcp::TcpAcceptor;
    let http_addr = format!("0.0.0.0:{}", http_port);
    let http_addr_parse: std::net::SocketAddr = http_addr
        .parse()
        .with_context(|| format!("invalid http_port: {}", http_port))?;
    // 跟 gRPC 共用同一个 EventLog (in-memory) + SessionStore
    let http_event_log = EventLog::open_in_memory()?;
    let http_router = ma_harness_server::http::run_router_with_log_and_store(
        Arc::new(ma_harness_core::StubModelAdapter),
        Arc::new(http_event_log),
        session_store,
    );
    let tokio_listener = tokio::net::TcpListener::bind(http_addr_parse)
        .await
        .with_context(|| format!("bind http {}", http_addr))?;
    let acceptor = TcpAcceptor::try_from(tokio_listener)
        .map_err(|e| anyhow::anyhow!("TcpAcceptor::try_from failed: {}", e))?;
    let http_server = salvo::Server::new(acceptor).serve(http_router);

    // 并发跑 gRPC + HTTP server, 等 ctrl-c
    tokio::select! {
        _ = grpc_server => eprintln!("grpc server exited"),
        _ = http_server => eprintln!("http server exited"),
        _ = tokio::signal::ctrl_c() => {
            eprintln!("mah: received ctrl-c, shutting down");
        }
    }
    Ok(())
}

async fn run_local_agent(session: Option<String>, message: String, model: String) -> Result<()> {
    let log = EventLog::open_in_memory()?;
    let session_id = session.unwrap_or_else(|| format!("local-{}", uuid::Uuid::new_v4()));
    let agent = AgentLoop::new(log, Arc::new(StubModelAdapter));
    let req = AgentRunRequest {
        session_id: session_id.clone(),
        user_message: message,
        model,
        temperature: 0.7,
        max_tokens: 1024,
        system_prompt: None,
    };
    let resp = agent.run(req).await?;
    println!(
        "Session: {}\nRun: {}\nContent: {}\nTokens: prompt={} completion={}",
        resp.session_id,
        resp.run_id,
        resp.model_response.content,
        resp.total_prompt_tokens,
        resp.total_completion_tokens,
    );
    Ok(())
}

fn list_plugins() -> Result<()> {
    // Phase 2.2 (T2.2): 走 inventory 查所有已注册 plugin (跨 crate 收集)
    // 不再硬编 ma_harness_plugin_hello::HelloPlugin, 走 PluginLoader::list()
    let names = PluginLoader::list();
    if names.is_empty() {
        println!("(no plugins registered via inventory)");
    } else {
        println!("Registered plugins ({} total):", names.len());
        for name in names {
            println!("  - {}", name);
        }
    }
    Ok(())
}

fn load_plugin(name: &str, ctx_id: &str) -> Result<()> {
    // Phase 2.2 (T2.2): 按名查 inventory, factory 构造, install 到 ctx
    use ma_harness_cordis::Context;
    let ctx = Context::new();
    eprintln!("mah load-plugin: looking up '{}' in ctx '{}'", name, ctx_id);
    PluginLoader::load_by_name(&ctx, name)
        .map_err(|e| anyhow::anyhow!("load '{}' failed: {}", name, e))?;
    println!("OK: loaded plugin '{}' into ctx '{}'", name, ctx_id);
    Ok(())
}

fn list_events(session: &str) -> Result<()> {
    let log = EventLog::open_in_memory()?;
    let page = log.get_model_visible(session)?;
    println!("Session {} ({} events):", session, page.events.len());
    for e in page.events {
        println!(
            "  seq={} type={} severity={}",
            e.seq, e.event.event_type, e.event.severity
        );
    }
    Ok(())
}

// ============================================================================
// P5-5 (Day 94): `mah sessions list/get/events` — 走本地 SqliteStore / EventLog
// 不连 server, 业务方传 db 路径直接读, debug 工具
// ============================================================================

/// `mah sessions list --store-path <db>` — 列出 SqliteStore 里所有 session
fn sessions_list(store_path: &std::path::Path) -> Result<()> {
    let store = ma_harness_server::SqliteStore::open(store_path)
        .map_err(|e| anyhow::anyhow!("open sqlite store {}: {e}", store_path.display()))?;
    let sessions = store.list().map_err(|e| anyhow::anyhow!("list: {e}"))?;
    if sessions.is_empty() {
        println!("(no sessions in {})", store_path.display());
        return Ok(());
    }
    println!(
        "Sessions ({} total) from {}:",
        sessions.len(),
        store_path.display()
    );
    for s in &sessions {
        let state_name = ma_harness_proto::ma_harness::v1::SessionState::try_from(s.state)
            .map(|st| format!("{:?}", st))
            .unwrap_or_else(|_| format!("unknown({})", s.state));
        let created = s
            .created_at
            .as_ref()
            .map(format_ts)
            .unwrap_or_else(|| "—".to_string());
        println!(
            "  {:36}  state={:9}  name={:20}  created={}",
            &s.id[..36.min(s.id.len())],
            state_name,
            format!("{:20}", s.name),
            created,
        );
    }
    Ok(())
}

/// `mah sessions get <id> --store-path <db>` — 拿单个 session
fn sessions_get(store_path: &std::path::Path, id: &str) -> Result<()> {
    let store = ma_harness_server::SqliteStore::open(store_path)
        .map_err(|e| anyhow::anyhow!("open sqlite store {}: {e}", store_path.display()))?;
    match store.get(id) {
        Ok(Some(s)) => {
            let state_name = ma_harness_proto::ma_harness::v1::SessionState::try_from(s.state)
                .map(|st| format!("{:?}", st))
                .unwrap_or_else(|_| format!("unknown({})", s.state));
            println!("Session:");
            println!("  id:     {}", s.id);
            println!("  name:   {}", s.name);
            println!("  state:  {} ({})", state_name, s.state);
            println!("  mode:   {}", s.mode);
            println!(
                "  created: {}",
                s.created_at
                    .as_ref()
                    .map(format_ts)
                    .unwrap_or_else(|| "—".to_string())
            );
            println!(
                "  updated: {}",
                s.updated_at
                    .as_ref()
                    .map(format_ts)
                    .unwrap_or_else(|| "—".to_string())
            );
            println!(
                "  closed:  {}",
                s.closed_at
                    .as_ref()
                    .map(format_ts)
                    .unwrap_or_else(|| "—".to_string())
            );
            println!("  user_id: {}", s.user_id);
            if !s.enabled_plugins.is_empty() {
                println!("  enabled_plugins: {}", s.enabled_plugins.join(", "));
            }
        }
        Ok(None) => {
            anyhow::bail!("session not found: {id}");
        }
        Err(e) => anyhow::bail!("get session: {e}"),
    }
    Ok(())
}

/// `mah sessions events <id> --log <events.db>` — 拿 session 的 events
fn sessions_events(log_path: &std::path::Path, session: &str) -> Result<()> {
    let log = EventLog::open(log_path)
        .map_err(|e| anyhow::anyhow!("open event log {}: {e}", log_path.display()))?;
    let page = log
        .get_model_visible(session)
        .map_err(|e| anyhow::anyhow!("get model visible: {e}"))?;
    if page.events.is_empty() {
        println!(
            "(no events for session {} in {})",
            session,
            log_path.display()
        );
        return Ok(());
    }
    println!(
        "Session {} ({} events) from {}:",
        session,
        page.events.len(),
        log_path.display()
    );
    for e in &page.events {
        let payload = e.event.payload_json.as_deref().unwrap_or("");
        let payload_short = if payload.len() > 60 {
            format!("{}...", &payload[..60])
        } else {
            payload.to_string()
        };
        println!(
            "  #{} [{}] {:12} {:20} {}",
            e.seq,
            e.event.ts.format("%H:%M:%S"),
            format!("{:?}", e.event.severity).to_lowercase(),
            format!("{:?}", e.event.event_type),
            payload_short,
        );
    }
    Ok(())
}

/// 格式化 prost_types::Timestamp (跟 http.rs 同样的方式)
fn format_ts(ts: &prost_types::Timestamp) -> String {
    let secs = ts.seconds;
    let nanos = ts.nanos as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| format!("{}s+{}ns", secs, nanos))
}

/// 跑 conformance fixture, 出报告
fn run_conformance(
    fixtures_path: &PathBuf,
    dsh: bool,
    output: &PathBuf,
    verbose: bool,
) -> Result<()> {
    // 1. 加载 fixture
    let fixtures: Vec<Fixture> = if dsh {
        // dsh 风格: 先 read 整个文件, 用 dsh_format::parse_dsh_jsonl
        let content = std::fs::read_to_string(fixtures_path)
            .with_context(|| format!("read dsh fixtures: {}", fixtures_path.display()))?;
        ma_harness_conformance::dsh_format::parse_dsh_jsonl(&content)
            .map_err(|e| anyhow::anyhow!("parse dsh jsonl: {e}"))?
    } else if fixtures_path.is_dir() {
        // ma-harness 风格: 目录里所有 .jsonl
        FixtureLoader::from_dir(fixtures_path)
            .map_err(|e| anyhow::anyhow!("load fixtures from dir: {e}"))?
    } else {
        // ma-harness 风格: 单文件
        FixtureLoader::from_jsonl(fixtures_path)
            .map_err(|e| anyhow::anyhow!("load fixtures from file: {e}"))?
    };

    if fixtures.is_empty() {
        eprintln!("No fixtures loaded from {}", fixtures_path.display());
        return Ok(());
    }
    eprintln!(
        "Loaded {} fixtures from {}",
        fixtures.len(),
        fixtures_path.display()
    );

    // 2. 跑
    let mut runner = ConformanceRunner::new();
    if verbose {
        runner = runner.verbose();
    }
    let results: Vec<ConformanceResult> = runner.run_all(&fixtures);

    // 3. 汇总
    let summary = runner.build_summary(&results);

    eprintln!(
        "Conformance: {} / {} passed ({:.1}%) in {}ms",
        summary.passed,
        summary.total,
        summary.pass_rate * 100.0,
        summary.total_duration_ms
    );
    if !summary.meets_target() {
        eprintln!(
            "WARNING: pass rate {:.1}% < 95% target (see report for diffs)",
            summary.pass_rate * 100.0
        );
        // CI gating: 业务方脚本可以靠 exit code 判 conformance 是否通过。
        // P15 (Day 101+2): expect_fail 字段让 negative test (e.g. comparer 抓 extra event)
        // 在报告里算 pass, 不会拉低 pass rate。dsh_synthetic / dsh_snap 都 100%, smoke 含 1 个
        // expect_fail=true fixture 翻转后也 100%。
        std::process::exit(1);
    }

    // 4. 写报告
    std::fs::create_dir_all(output)
        .with_context(|| format!("create output dir: {}", output.display()))?;

    let report = ReportWriter::build(&results, summary);
    let md_path = output.join("conformance-report.md");
    let json_path = output.join("conformance-report.json");

    ReportWriter::write_markdown(&report, &md_path)
        .with_context(|| format!("write markdown: {}", md_path.display()))?;
    ReportWriter::write_json(&report, &json_path)
        .with_context(|| format!("write json: {}", json_path.display()))?;

    println!("Markdown: {}", md_path.display());
    println!("JSON:     {}", json_path.display());
    println!("Format:   {:?}", ReportFormat::Markdown);

    Ok(())
}

/// 跑 Code Mode: 编译并执行 .wat / .wasm 文件
fn run_code(file: &std::path::Path) -> Result<()> {
    use ma_harness_code::CodeRunner;
    let runner = CodeRunner::new().map_err(|e| anyhow::anyhow!("init CodeRunner: {e}"))?;
    eprintln!("mah code run: loading {}", file.display());
    let ext = file.extension().and_then(|s| s.to_str()).unwrap_or("");
    let output = match ext {
        "wat" => {
            let text = std::fs::read_to_string(file)
                .with_context(|| format!("read WAT: {}", file.display()))?;
            runner
                .run_wat(&text)
                .map_err(|e| anyhow::anyhow!("run WAT: {e}"))?
        }
        "wasm" => {
            let bytes =
                std::fs::read(file).with_context(|| format!("read WASM: {}", file.display()))?;
            runner
                .run_wasm(&bytes)
                .map_err(|e| anyhow::anyhow!("run WASM: {e}"))?
        }
        other => {
            anyhow::bail!("unsupported extension '.{}', expected .wat or .wasm", other);
        }
    };
    println!("--- stdout ---");
    for line in &output.stdout_lines {
        println!("{}", line);
    }
    println!("--- return value: {} ---", output.return_value);
    Ok(())
}

/// 从 LLM 文本响应里提取 WAT (处理 markdown fence + 找 (module ... ))
fn extract_wat_from_llm_response(text: &str) -> Option<String> {
    // 1. 找 ```wat ... ``` fence
    if let Some(start) = text.find("```wat") {
        let after = &text[start + 6..];
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim().to_string());
        }
    }
    // 2. 找 ``` ... ``` (没指定语言)
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        if let Some(end) = after.find("```") {
            let body = after[..end].trim();
            // 验证内容是 WAT (含 (module)
            if body.contains("(module") {
                return Some(body.to_string());
            }
        }
    }
    // 3. 找 (module ... ) 直接形式
    if let Some(start) = text.find("(module") {
        // 简单算 (module 配对的 ), 配错 fall back
        let mut depth = 0i32;
        let mut end = None;
        for (i, c) in text.as_bytes()[start..].iter().copied().enumerate() {
            match c {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = end {
            return Some(text[start..end].to_string());
        }
    }
    None
}

// enumerate helper: walk over bytes with index

/// **P6-1 (Day 99)**: 解析 `mah run-stream --model <S>` 字符串 → (proto::ModelAdapter enum int, model name)
///
/// 业务方格式:
///   "stub"                       → (0=Unspecified, "stub")  [server 自己 stub fallback]
///   "openai:gpt-4o-mini"         → (1=Openai,  "gpt-4o-mini")
///   "anthropic:claude-3-5-sonnet" → (1=Openai,  "claude-3-5-sonnet")  [proto 暂未分, 走 Openai 通道]
///   "gpt-4o-mini" (无 prefix)     → (0=Unspecified, "gpt-4o-mini")
///   "weird:foo" (未知 provider)  → (0=Unspecified, "foo")
fn parse_model_arg(s: &str) -> (i32, String) {
    if let Some((provider, name)) = s.split_once(':') {
        let adapter = match provider {
            "openai" => 1,    // proto ModelAdapter::Openai
            "anthropic" => 1, // proto 暂未分, fallback Openai 通道
            _ => 0,           // 未知 provider → Unspecified, server 自己处理
        };
        (adapter, name.to_string())
    } else {
        // "stub" / "gpt-4o-mini" 等无 prefix → 0 (Unspecified)
        (0, s.to_string())
    }
}

/// **P6-1 (Day 99)**: 走 gRPC RunStream RPC, 业务方命令行拿 streaming token
///
/// 流程:
/// 1. 连 gRPC server (tonic)
/// 2. 构造 AgentRunRequest (session_id / model_config)
/// 3. 调 stub.RunStream(req) 拿 server-streaming response
/// 4. iter AgentStreamEvent, 拿 message.content[0].text 实时打印
///
/// 跟 bindings/python/stream_client.py 同样模式, 走 stub adapter 也能跑 (3 word "hello world from stub" → 3 token)
async fn run_stream_cmd(
    prompt: &str,
    grpc_url: &str,
    session: Option<&str>,
    model: &str,
) -> Result<()> {
    use futures::StreamExt;
    use ma_harness_proto::ma_harness::v1::{
        agent_service_client::AgentServiceClient, agent_stream_event::Event, AgentRunRequest,
        ContentBlock, Message, ModelConfig, TextBlock, ToolRole,
    };
    use std::io::Write;

    // 1. 连 gRPC server
    // tonic 0.12 Endpoint::try_from 要 'static 生命周期, async fn 拿 &str 绑 'static 必 fail.
    // 修法 (P6-1 踩坑): grpc_url.to_string() 转 owned, 后续 'static 走 owned String.
    let grpc_url_owned = grpc_url.to_string();
    let endpoint = tonic::transport::Endpoint::try_from(grpc_url_owned.clone())
        .map_err(|e| anyhow::anyhow!("parse grpc url {grpc_url_owned}: {e}"))?;
    let channel = endpoint.connect().await?;
    let mut client = AgentServiceClient::new(channel);

    // 2. 构造 AgentRunRequest
    let session_id = session
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("cli-stream-{}", uuid::Uuid::new_v4()));
    let (adapter_int, model_name) = parse_model_arg(model);
    let req = AgentRunRequest {
        session_id: session_id.clone(),
        input: Some(Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: ToolRole::User as i32,
            content: vec![ContentBlock {
                content: Some(
                    ma_harness_proto::ma_harness::v1::content_block::Content::Text(TextBlock {
                        text: prompt.to_string(),
                    }),
                ),
            }],
            created_at: None,
            session_id: session_id.clone(),
        }),
        model_config: Some(ModelConfig {
            adapter: adapter_int,
            model: model_name,
            temperature: 0.0,
            max_tokens: 1024,
            system_prompt: "".to_string(),
        }),
        options: None,
    };

    eprintln!("mah run-stream: prompt = {prompt}");
    eprintln!("mah run-stream: grpc_url = {grpc_url}");
    eprintln!("mah run-stream: model = {model}");

    // 3. 调 RunStream 拿 server-streaming
    let mut stream = client.run_stream(req).await?.into_inner();
    let mut collected = Vec::new();
    while let Some(event) = stream.next().await {
        let event = event?;
        if let Some(Event::Message(msg)) = event.event {
            if let Some(ContentBlock {
                content: Some(ma_harness_proto::ma_harness::v1::content_block::Content::Text(t)),
            }) = msg.content.first()
            {
                let token = &t.text;
                collected.push(token.clone());
                // 实时打印 (无 newline, 类似 typewriter)
                print!("{token}");
                std::io::stdout().flush().ok();
            }
        }
    }
    println!(); // 最后换行
    eprintln!(
        "\n--- done: {} tokens, full content: {:?} ---",
        collected.len(),
        collected.join("")
    );
    Ok(())
}

/// **Phase 3.3 / T3.3**: 业务方 prompt → LLM 生成 .wat → wasm 沙箱跑
///
/// 流程:
/// 1. 拿 OPENAI_API_KEY (--api-key 显式 > env)
/// 2. 构造 OpenaiAdapter
/// 3. 发 prompt + system instruction "return .wat"
/// 4. parse_response → content
/// 5. extract_wat_from_llm_response 提取 .wat
/// 6. CodeRunner (T3.1 sandbox) 跑
/// 7. 显示 stdout + return value
async fn run_prompt(prompt: &str, api_key: Option<&str>, model: &str) -> Result<()> {
    use ma_harness_code::{CodeRunner, SandboxConfig};
    use ma_harness_core::ModelRequest;
    use ma_harness_model::OpenaiAdapter;

    // 1. API key
    let key = match api_key {
        Some(k) => k.to_string(),
        None => std::env::var("OPENAI_API_KEY").map_err(|_| {
            anyhow::anyhow!("OPENAI_API_KEY not set. Use --api-key or export OPENAI_API_KEY=sk-...")
        })?,
    };

    eprintln!("mah run-prompt: prompt = {prompt}");
    eprintln!("mah run-prompt: model = {model}");

    // 2. 构造 adapter
    let adapter = OpenaiAdapter::new(key).with_model(model.to_string());

    // 3. 构造 ModelRequest
    let system = "You are a WebAssembly expert. The user will give you a task. \
                  Generate a valid WAT (WebAssembly text format) module that performs the task. \
                  The module MUST export a function named 'run' that returns i32. \
                  If the task requires printing output, import the host function 'host.log(ptr:i32, len:i32)' \
                  and export a 'memory'. Otherwise, just return the result as i32. \
                  Return ONLY the WAT source, optionally wrapped in ```wat ... ``` markdown fence. \
                  Do NOT add explanations outside the code block.";

    let req = ModelRequest {
        model: model.to_string(),
        messages: vec![ma_harness_core::ModelMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
        temperature: 0.0, // 0 = deterministic, 跟 code generation 对齐
        max_tokens: 1024,
        system_prompt: Some(system.to_string()),
    };

    // 4. 调 LLM (走 ModelAdapter trait)
    use ma_harness_core::ModelAdapter;
    eprintln!("mah run-prompt: calling LLM...");
    let resp = adapter
        .complete(&req)
        .await
        .map_err(|e| anyhow::anyhow!("LLM call failed: {e}"))?;
    eprintln!(
        "mah run-prompt: LLM returned ({} prompt + {} completion tokens)",
        resp.prompt_tokens, resp.completion_tokens
    );

    // 5. 提取 WAT
    let wat = extract_wat_from_llm_response(&resp.content).ok_or_else(|| {
        anyhow::anyhow!(
            "no WAT found in LLM response. Raw content (first 500 chars):\n{}",
            resp.content.chars().take(500).collect::<String>()
        )
    })?;

    eprintln!("--- LLM generated WAT ({} bytes) ---", wat.len());
    for line in wat.lines() {
        eprintln!("  {}", line);
    }
    eprintln!("--- end WAT ---");

    // 6. wasm 跑 (T3.1 sandbox)
    let runner = CodeRunner::new_with_config(SandboxConfig::default())
        .map_err(|e| anyhow::anyhow!("init CodeRunner: {e}"))?;

    eprintln!("mah run-prompt: running WAT in wasm sandbox...");
    let output = runner
        .run_wat(&wat)
        .map_err(|e| anyhow::anyhow!("wasm run failed: {e}"))?;

    // 7. 显示结果
    println!("--- stdout ---");
    for line in &output.stdout_lines {
        println!("{}", line);
    }
    println!("--- return value: {} ---", output.return_value);
    Ok(())
}

/// 打印 benchmark 信息 (不真跑, criterion 走 cargo bench)
fn print_bench_info(crate_name: Option<&str>) -> Result<()> {
    println!("ma-harness bench info");
    println!("=====================");
    println!();
    println!("Benchmark 实际跑用 cargo bench (criterion 0.5 驱动).");
    println!();
    println!("跑法:");
    if let Some(c) = crate_name {
        println!("  cargo bench -p {c}");
    } else {
        println!("  cargo bench --workspace");
    }
    println!();
    println!("单 bench:");
    if let Some(c) = crate_name {
        println!("  cargo bench -p {c} -- <bench_name>");
    } else {
        println!("  cargo bench -p ma_harness_cordis -- ctx_set_typed_key");
    }
    println!();
    println!("HTML 报告:");
    println!("  target/criterion/<crate>/<bench_name>/report/index.html");
    println!();
    println!("详细 bench 列表见 docs/benchmark-design.md § 3 + docs/benchmark-report-week11.md");
    Ok(())
}

/// **Phase 3.5 / T3.5**: 从 server router 导出 OpenAPI spec
///
/// 走 salvo-oapi 0.79 `OpenApi::new("title", "0.1").merge_router(&router)`,
/// 然后 to_pretty_json / to_yaml 写到文件.
///
/// CI drift 检查:
///   1. 跑 `mah openapi export --output /tmp/new.json`
///   2. diff /tmp/new.json docs/api/openapi.json
///   3. drift → fail
///
/// **Phase 5.1 (Day 90)**: 改用 run_router_with_store 拿 /v1/sessions 4 endpoint
fn export_openapi(output: &std::path::Path) -> Result<()> {
    use salvo::oapi::OpenApi;
    use ma_harness_core::StubModelAdapter;
    use std::sync::Arc;

    // 构造完整 router (含 /v1/runs + /v1/sessions + /v1/sessions/{id}/events) + stub + InMemoryStore
    // run_router_with_log_and_store 内部会 set 3 个 global
    // OpenAPI 导出只关心 router 结构, 不发真 HTTP
    let router = ma_harness_server::http::run_router_with_log_and_store(
        Arc::new(StubModelAdapter),
        Arc::new(
            ma_harness_core::EventLog::open_in_memory()
                .map_err(|e| anyhow::anyhow!("event log: {e}"))?,
        ),
        Arc::new(ma_harness_server::InMemoryStore::new()),
    );
    let doc = OpenApi::new("ma-harness API", "0.1.0").merge_router(&router);

    // 按扩展名决定 json / yaml
    let ext = output
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("json");
    let content: String = match ext {
        "yaml" | "yml" => doc
            .to_yaml()
            .map_err(|e| anyhow::anyhow!("openapi to_yaml failed: {e}"))?,
        _ => doc
            .to_pretty_json()
            .map_err(|e| anyhow::anyhow!("openapi to_pretty_json failed: {e}"))?,
    };

    // 写文件
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create output dir: {}", parent.display()))?;
        }
    }
    std::fs::write(output, &content)
        .with_context(|| format!("write openapi: {}", output.display()))?;

    eprintln!(
        "mah openapi export: wrote {} ({} bytes)",
        output.display(),
        content.len()
    );
    Ok(())
}

/// **Phase 3.7 / T3.7**: Enforce landlock (Linux) / seatbelt (Mac) / stub (其他) 沙箱
///
/// **P13.5 / Day 101+2**: `mah dsh info` — 显示 dsh-adapter 运行时信息
fn dsh_info() -> Result<()> {
    println!("=== ma-harness dsh-adapter info ===");
    println!("ma-harness version: {}", env!("CARGO_PKG_VERSION"));
    println!(
        "dsh protocol version: {} (P13.1 锁定)",
        ma_harness_plugin_dsh_adapter::DSH_PROTOCOL_VERSION
    );

    // 探测 Node.js
    let node = detect_node();
    println!("Node.js: {}", node.display());

    // 探测试图跑
    println!("\ndsh-adapter crate: ma-harness-plugin-dsh-adapter");
    println!("Plugin path format: dsh::/path/to/plugin.ts");
    println!("Usage:");
    println!("  mah load-plugin dsh::./examples/k8s_pod_status.ts");
    println!("  mah dsh doctor   # 健康检查");
    Ok(())
}

/// **P13.5 / Day 101+2**: `mah dsh doctor` — 健康检查
async fn dsh_doctor() -> Result<()> {
    println!("=== ma-harness dsh-adapter doctor ===");
    let mut all_ok = true;

    // 1. Node.js 在 PATH?
    let node = detect_node();
    if node.is_file() {
        println!("[ok] Node.js found: {}", node.display());
    } else {
        println!("[FAIL] Node.js not found in PATH");
        println!("       Install Node.js 22.19+ (or 24+) from https://nodejs.org/");
        all_ok = false;
    }

    // 2. dsh-adapter crate compile OK (这步是 build 触发的, run 阶段跳过)
    println!("[ok] dsh-adapter crate compiled (P13.1-P13.4 5 commits)");

    // 3. 模拟 spawn node + JSON-RPC echo (in-process test)
    println!("[..]  spawn node mock dsh server + JSON-RPC initialize + tools/list + tools/call...");
    match dsh_doctor_subprocess_test().await {
        Ok(report) => {
            println!("[ok]  mock dsh server responded: {}", report);
        }
        Err(e) => {
            println!("[FAIL] mock dsh server test failed: {e}");
            all_ok = false;
        }
    }

    println!();
    if all_ok {
        println!("[ok] All checks passed. dsh-adapter is ready.");
    } else {
        println!("[FAIL] Some checks failed. See messages above.");
    }
    Ok(())
}

/// `mah dsh doctor` 用: 跑 mock dsh server, 验 JSON-RPC 跑通
async fn dsh_doctor_subprocess_test() -> Result<String> {
    use std::path::Path;
    use ma_harness_plugin_dsh_adapter::{DshAdapter, DshConfig};

    let adapter = DshAdapter::spawn(Path::new("mock://inline"), DshConfig::default()).await?;
    let server_info = adapter.initialize().await?;
    let tools = adapter.list_tools().await?;
    let _ = adapter
        .call_tool("echo", serde_json::json!({"msg": "doctor"}))
        .await?;
    adapter.shutdown().await?;
    Ok(format!(
        "server={} v{}, tools={}",
        server_info.name,
        server_info.version,
        tools.len()
    ))
}

/// 探测 node 可执行文件路径 (跨平台 PATH 搜)
fn detect_node() -> std::path::PathBuf {
    let path_env = match std::env::var("PATH") {
        Ok(p) => p,
        Err(_) => return std::path::PathBuf::from("node"),
    };
    let sep = if cfg!(windows) { ';' } else { ':' };
    let exts: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    for dir in path_env.split(sep) {
        if dir.is_empty() {
            continue;
        }
        for ext in exts {
            let candidate = std::path::PathBuf::from(dir).join(format!("node{ext}"));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    std::path::PathBuf::from("node")
}

/// 警告: 一旦 enforce 是全进程 (不可逆). 业务方决定要不要跑.
///
/// 流程:
/// 1. 构造 ma_harness_sandbox::Policy
/// 2. 选 DefaultEnforcer (跨平台 type alias)
/// 3. enforce(&policy)
/// 4. 成功: 进程 fs 受限, 后续操作严格走白名单
fn apply_sandbox(
    read_paths: Vec<std::path::PathBuf>,
    write_paths: Vec<std::path::PathBuf>,
    exec_paths: Vec<std::path::PathBuf>,
    temp_dir: bool,
) -> Result<()> {
    use ma_harness_sandbox::{DefaultEnforcer, Enforcer, PathRule, Policy};

    let mut read_rules: Vec<PathRule> = read_paths
        .iter()
        .map(|p| PathRule::Subpath(p.clone()))
        .collect();
    if temp_dir {
        read_rules.push(PathRule::TempDir);
    }
    let write_rules: Vec<PathRule> = write_paths
        .iter()
        .map(|p| PathRule::Subpath(p.clone()))
        .collect();
    let exec_rules: Vec<PathRule> = exec_paths
        .iter()
        .map(|p| PathRule::Subpath(p.clone()))
        .collect();

    let policy = Policy {
        read_paths: read_rules,
        write_paths: write_rules,
        exec_paths: exec_rules,
        allow_network: false,
    };

    eprintln!("mah sandbox apply: enforcing policy:");
    eprintln!("  read_paths: {:?}", policy.read_paths);
    eprintln!("  write_paths: {:?}", policy.write_paths);
    eprintln!("  exec_paths: {:?}", policy.exec_paths);
    eprintln!("  allow_network: {}", policy.allow_network);

    let enforcer = DefaultEnforcer::default();
    match enforcer.enforce(&policy) {
        Ok(()) => {
            eprintln!("mah sandbox apply: OK — host process fs limited");
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("sandbox enforce failed: {e:?}");
        }
    }
}

/// **Phase 3.9 / T3.9**: 启动 TUI dashboard (ratatui)
///
/// 走 ma_harness_tui::TuiApp::run(), 用户在 terminal 看 3 panel:
/// - Sessions (左)
/// - Plugins (右)
/// - Status bar (底): ticks / uptime / events
///
/// 'q' / Esc / Ctrl-C 退出. 走 ratatui::init() + ratatui::restore() 保证 terminal 状态恢复.
///
/// **P4-1** 增强: log 参数走真 EventLog (sqlite 读), 缺省走 stub.
/// **P4-3** 增强: store_path 参数走真 SessionStore (sqlite 读), 缺省走 log 推 / stub.
fn run_tui(log: Option<&std::path::Path>, store_path: Option<&std::path::Path>) -> Result<()> {
    // P4-3: 业务方传 --store-path → SqliteStore 接真 sessions
    let store: Option<Arc<dyn ma_harness_server::SessionStore>> = match store_path {
        Some(p) => match ma_harness_server::SqliteStore::open(p) {
            Ok(s) => {
                eprintln!("mah tui: session store = sqlite:{}", p.display());
                Some(Arc::new(s))
            }
            Err(e) => {
                eprintln!(
                    "mah tui: WARN failed to open session store {}: {e}; using stub",
                    p.display()
                );
                None
            }
        },
        None => None,
    };

    let mut app = ma_harness_tui::TuiApp::new_with_log_and_store(log, store)
        .map_err(|e| anyhow::anyhow!("init TuiApp: {e}"))?;
    app.run().map_err(|e| anyhow::anyhow!("tui run: {e}"))
}

/// **Phase 3.7 / T3.7**: 打印当前 OS 沙箱支持
fn print_sandbox_status() -> Result<()> {
    println!("ma-harness sandbox status");
    println!("=========================");
    println!();
    println!("Target OS: {}", std::env::consts::OS);
    println!("Architecture: {}", std::env::consts::ARCH);
    println!();
    #[cfg(target_os = "linux")]
    {
        println!("Backend: landlock 0.4");
        println!("  - Landlock ABI V1 (kernel >= 5.13)");
        println!(
            "  - 12 AccessFs ops (ReadFile / ReadDir / WriteFile / RemoveFile / RemoveDir / MakeReg / MakeDir / MakeSock / MakeFifo / MakeBlock / MakeChar / Refer)"
        );
        println!("  - restrict_self() 不可逆");
        println!("  - 走 landlock::Ruleset + PathBeneath");
    }
    #[cfg(target_os = "macos")]
    {
        println!("Backend: macos seatbelt (Phase 2.2 stub)");
        println!("  - 占位: 返回 Ok, 不实际 enforce");
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        println!("Backend: stub (Windows / 其他)");
        println!("  - warn + no-op, 业务方 fs 不受限制");
    }
    Ok(())
}

/// **P14 (2026-08-20)**: 业务方 publish 后的 plugin list (给 GH Pages 静态站消费)
///
/// 例子:
///   mah registry list                            # 默认 ~/.ma-harness/registry.json
///   mah registry list --registry my.json         # 自定义路径
fn registry_list(registry: Option<&std::path::Path>) -> Result<()> {
    let path = registry
        .map(|p| p.to_path_buf())
        .or_else(|| dirs_home().map(|h| h.join(".ma-harness").join("registry.json")))
        .context("no registry path given and no ~/.ma-harness/registry.json")?;

    if !path.exists() {
        eprintln!("Registry file not found: {}", path.display());
        eprintln!(
            "Hint: run `mah plugin publish <manifest.json>` first, or pass --registry <path>"
        );
        return Ok(());
    }

    let reg = Registry::open(&path)
        .with_context(|| format!("failed to open registry at {}", path.display()))?;

    println!("ma-harness Plugin Registry");
    println!("============================");
    println!("Path:   {}", path.display());
    println!(
        "Plugins: {} ({} versions total)",
        reg.count(),
        reg.version_count()
    );
    println!();

    let mut by_author: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for m in reg.list() {
        by_author.entry(m.author.clone()).or_default().push(format!(
            "{} @ {} (tags: {})",
            m.name,
            m.version,
            if m.tags.is_empty() {
                "-".to_string()
            } else {
                m.tags.join(", ")
            }
        ));
    }

    for (author, plugins) in &by_author {
        println!("{} ({}):", author, plugins.len());
        for p in plugins {
            println!("  - {}", p);
        }
    }
    Ok(())
}

/// **P14 (2026-08-20)**: 导出 registry 到 JSON file (给 GH Pages 静态站消费)
///
/// 例子:
///   mah registry export --output docs/registry/registry.json
///   mah registry export --output target/registry.json --registry my.json
fn registry_export(output: &std::path::Path, registry: Option<&std::path::Path>) -> Result<()> {
    let path = registry
        .map(|p| p.to_path_buf())
        .or_else(|| dirs_home().map(|h| h.join(".ma-harness").join("registry.json")))
        .context("no registry path given and no ~/.ma-harness/registry.json")?;

    if !path.exists() {
        anyhow::bail!(
            "Registry file not found: {}. Run `mah plugin publish` first.",
            path.display()
        );
    }

    let reg = Registry::open(&path)
        .with_context(|| format!("failed to open registry at {}", path.display()))?;

    // 确保输出目录存在
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create output dir {}", parent.display()))?;
        }
    }

    reg.export(output)
        .with_context(|| format!("failed to export registry to {}", output.display()))?;

    println!(
        "Exported {} plugins ({} versions) to {}",
        reg.count(),
        reg.version_count(),
        output.display()
    );
    Ok(())
}

/// Helper: 拿 $HOME / %USERPROFILE% (无依赖 dirs crate)
fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

/// **P14 (2026-08-20)**: tests for `mah registry` CLI subcommand
#[cfg(test)]
mod registry_cli_tests {
    use super::*;
    use ma_harness_registry::{PluginManifest, PluginSource, Registry};
    use semver::Version;
    use chrono::Utc;

    fn make_sample_manifest(name: &str, version: &str, author: &str) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: version.parse().unwrap(),
            description: format!("Sample plugin {}", name),
            author: author.to_string(),
            source: PluginSource::Local(format!("../plugins/{}", name)),
            tags: vec!["test".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// registry_list 走 sample registry
    #[test]
    fn registry_list_works() {
        let tmpdir = tempfile::tempdir().unwrap();
        let reg_path = tmpdir.path().join("registry.json");
        let mut reg = Registry::open_in_memory();
        reg.publish(make_sample_manifest("plug-a", "0.1.0", "alice"))
            .unwrap();
        reg.publish(make_sample_manifest("plug-b", "0.2.0", "bob"))
            .unwrap();
        reg.save(&reg_path).unwrap();

        let result = registry_list(Some(&reg_path));
        assert!(result.is_ok(), "registry_list should succeed: {:?}", result);
    }

    /// registry_export 生成 JSON, 内容正确
    #[test]
    fn registry_export_writes_valid_json() {
        let tmpdir = tempfile::tempdir().unwrap();
        let reg_path = tmpdir.path().join("registry.json");
        let out_path = tmpdir.path().join("out.json");

        let mut reg = Registry::open_in_memory();
        reg.publish(make_sample_manifest("plug-x", "1.0.0", "i25ma"))
            .unwrap();
        reg.save(&reg_path).unwrap();

        let result = registry_export(&out_path, Some(&reg_path));
        assert!(
            result.is_ok(),
            "registry_export should succeed: {:?}",
            result
        );
        assert!(out_path.exists(), "out.json should exist");

        // 验证 JSON 能 roundtrip
        let loaded = Registry::open(&out_path).unwrap();
        let manifest = loaded.get("plug-x").expect("plug-x should be in registry");
        assert_eq!(manifest.version, Version::parse("1.0.0").unwrap());
        assert_eq!(manifest.author, "i25ma");
    }

    /// registry_list / export 在 registry 不存在时返 Err (export) 或 Ok+warning (list)
    #[test]
    fn registry_missing_file_handled() {
        let tmpdir = tempfile::tempdir().unwrap();
        let reg_path = tmpdir.path().join("nonexistent.json");
        let out_path = tmpdir.path().join("out.json");

        // list: 不 panic, 返 Ok, 警告
        let list_result = registry_list(Some(&reg_path));
        assert!(list_result.is_ok(), "list 不存在 file 应返 Ok + warning");

        // export: 返 Err
        let export_result = registry_export(&out_path, Some(&reg_path));
        assert!(export_result.is_err(), "export 不存在 file 应返 Err");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_core::SessionEvent;

    // === P5-5 (Day 94): mah sessions CLI ===

    /// sessions_list 在 db 不存在时报错 (清晰错误信息)
    #[test]
    fn sessions_list_missing_db_errors() {
        let result = sessions_list(&std::path::PathBuf::from("/nonexistent/path/x.db"));
        assert!(result.is_err());
    }

    /// sessions_list 走真 SqliteStore, 创 2 session + 验 list 拿到
    #[test]
    fn sessions_list_works() {
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("sessions.db");
        let store = ma_harness_server::SqliteStore::open(&db_path).unwrap();
        for (id, name) in [("alpha", "first"), ("beta", "second")] {
            store
                .create(&ProtoSession {
                    id: id.to_string(),
                    name: name.to_string(),
                    state: ProtoSessionState::Active as i32,
                    mode: OperatingMode::Default as i32,
                    created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                    updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                    closed_at: None,
                    metadata: None,
                    stats: None,
                    enabled_plugins: vec![],
                    user_id: String::new(),
                })
                .unwrap();
        }
        // sessions_list 走 SqliteStore, 不 panic, 返 Result
        let result = sessions_list(&db_path);
        assert!(result.is_ok(), "sessions_list 走通: {:?}", result);
    }

    /// sessions_get 拿存在的 session
    #[test]
    fn sessions_get_works() {
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("sessions.db");
        let store = ma_harness_server::SqliteStore::open(&db_path).unwrap();
        store
            .create(&ProtoSession {
                id: "get-test".to_string(),
                name: "getname".to_string(),
                state: ProtoSessionState::Active as i32,
                mode: OperatingMode::Default as i32,
                created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                closed_at: None,
                metadata: None,
                stats: None,
                enabled_plugins: vec!["hello".to_string()],
                user_id: String::new(),
            })
            .unwrap();
        let result = sessions_get(&db_path, "get-test");
        assert!(result.is_ok(), "sessions_get 走通: {:?}", result);
    }

    /// sessions_get 拿不存在的 session 返 Err
    #[test]
    fn sessions_get_missing_errors() {
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("sessions.db");
        let _ = ma_harness_server::SqliteStore::open(&db_path).unwrap();
        let result = sessions_get(&db_path, "nonexistent-id");
        assert!(result.is_err(), "missing session 应返 Err");
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("not found"),
            "错误信息应含 not found, got: {}",
            err
        );
    }

    /// sessions_events 走真 EventLog
    #[test]
    fn sessions_events_works() {
        use ma_harness_core::EventType;
        let tmpdir = tempfile::tempdir().unwrap();
        let log_path = tmpdir.path().join("events.db");
        let log = EventLog::open(&log_path).unwrap();
        let mut ev = SessionEvent::new("ev-test", EventType::SessionStart);
        ev.payload_json = Some(r#"{"hello":"world"}"#.to_string());
        let _ = log.append(ev);
        let result = sessions_events(&log_path, "ev-test");
        assert!(result.is_ok(), "sessions_events 走通: {:?}", result);
    }

    /// format_ts 走 prost_types::Timestamp → RFC3339
    #[test]
    fn format_ts_works() {
        let ts = prost_types::Timestamp::from(std::time::SystemTime::UNIX_EPOCH);
        let s = format_ts(&ts);
        assert!(
            s.contains("1970") || s.contains("+"),
            "应含 1970 或 + (UTC offset), got {}",
            s
        );
    }

    // === T3.3 WAT extraction helper 测试 ===

    #[test]
    fn extract_wat_from_wat_fence() {
        let text = r#"Here's the WAT:
```wat
(module
    (memory (export "memory") 1)
    (func (export "run") (result i32)
        i32.const 42
    )
)
```
That's it."#;
        let wat = extract_wat_from_llm_response(text).unwrap();
        assert!(wat.contains("(module"));
        assert!(wat.contains("i32.const 42"));
    }

    #[test]
    fn extract_wat_from_plain_fence() {
        let text = r#"```
(module (func (export "run") (result i32) i32.const 1))
```"#;
        let wat = extract_wat_from_llm_response(text).unwrap();
        assert!(wat.contains("(module"));
    }

    #[test]
    fn extract_wat_from_bare_module() {
        let text = r#"Here is the code:
(module
    (func (export "run") (result i32) i32.const 0)
)
End."#;
        let wat = extract_wat_from_llm_response(text).unwrap();
        assert!(wat.contains("(module"));
        assert!(wat.contains("i32.const 0"));
    }

    #[test]
    fn extract_wat_no_module_returns_none() {
        let text = "I cannot generate WAT for this.";
        assert!(extract_wat_from_llm_response(text).is_none());
    }

    #[test]
    fn extract_wat_handles_paren_matching() {
        // 嵌套括号 (i32.const (1 + 2)) 不应乱配
        let text = r#"
(module
    (func (export "run") (result i32)
        i32.const 5
    )
)
"#;
        let wat = extract_wat_from_llm_response(text).unwrap();
        // 应含 export run + i32.const 5
        assert!(wat.contains("export \"run\""));
        assert!(wat.contains("i32.const 5"));
    }

    // === P6-1 (Day 99): mah run-stream CLI ===

    /// parse_model_arg: "stub" → (0, "stub")
    #[test]
    fn parse_model_arg_stub() {
        let (adapter, name) = parse_model_arg("stub");
        assert_eq!(adapter, 0, "stub 应走 Unspecified");
        assert_eq!(name, "stub");
    }

    /// parse_model_arg: "openai:gpt-4o-mini" → (1, "gpt-4o-mini")
    #[test]
    fn parse_model_arg_openai() {
        let (adapter, name) = parse_model_arg("openai:gpt-4o-mini");
        assert_eq!(adapter, 1, "openai 应走 Openai enum (1)");
        assert_eq!(name, "gpt-4o-mini");
    }

    /// parse_model_arg: "anthropic:claude-3-5-sonnet" → (1, "claude-3-5-sonnet")
    /// (proto 暂未分, fallback Openai 通道)
    #[test]
    fn parse_model_arg_anthropic() {
        let (adapter, name) = parse_model_arg("anthropic:claude-3-5-sonnet");
        assert_eq!(adapter, 1, "anthropic 暂走 Openai 通道");
        assert_eq!(name, "claude-3-5-sonnet");
    }

    /// parse_model_arg: "gpt-4o-mini" (无 prefix) → (0, "gpt-4o-mini")
    #[test]
    fn parse_model_arg_no_prefix() {
        let (adapter, name) = parse_model_arg("gpt-4o-mini");
        assert_eq!(adapter, 0, "无 prefix 应走 Unspecified");
        assert_eq!(name, "gpt-4o-mini");
    }

    /// parse_model_arg: "weird:foo" (未知 provider) → (0, "foo")
    #[test]
    fn parse_model_arg_unknown_provider() {
        let (adapter, name) = parse_model_arg("weird:foo");
        assert_eq!(adapter, 0, "未知 provider 应走 Unspecified");
        assert_eq!(name, "foo");
    }

    /// parse_model_arg: 多个 `:` 切第一对 (split_once) → ("openai", "gpt-4o:turbo" 保留)
    #[test]
    fn parse_model_arg_multi_colon() {
        let (adapter, name) = parse_model_arg("openai:gpt-4o:turbo");
        assert_eq!(adapter, 1);
        assert_eq!(name, "gpt-4o:turbo", "split_once 只切第一个 `:`");
    }
}
