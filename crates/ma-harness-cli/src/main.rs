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

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ma_harness_conformance::{
    fixture::FixtureLoader, ConformanceRunner, ConformanceResult, Fixture, ReportFormat, ReportWriter,
};
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
// 2026-08-18 (Day 52): ma_harness_proto 恢复 (用本地 vendor/protoc), gRPC service 恢复
use ma_harness_proto::ma_harness::v1::{
    agent_service_server::AgentServiceServer, session_service_server::SessionServiceServer,
};
use ma_harness_seam::{PluginLoader, PluginRegistry};
// Phase 2.2 (T2.2): 引用 hello plugin 触发 link, inventory::submit! 才有 effect
#[allow(unused_imports)]
use ma_harness_plugin_hello as _hello;
use ma_harness_server::{AgentServiceImpl, ServerBuilder, SessionServiceImpl};

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
    Tui,
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { grpc_port, http_port, store_path } => {
            start_server(grpc_port, http_port, store_path.as_deref()).await
        }
        Commands::Run { session, message, model } => {
            run_local_agent(session, message, model).await
        }
        Commands::Plugins => list_plugins(),
        Commands::LoadPlugin { name, ctx_id } => load_plugin(&name, &ctx_id),
        Commands::Events { session } => list_events(&session),
        Commands::Conformance { fixtures, dsh, output, verbose } => {
            run_conformance(&fixtures, dsh, &output, verbose)
        }
        Commands::Bench { crate_name } => print_bench_info(crate_name.as_deref()),
        Commands::Version => {
            println!("mah {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Code { action } => match action {
            CodeAction::Run { file } => run_code(&file),
        },
        Commands::RunPrompt { prompt, api_key, model } => {
            run_prompt(&prompt, api_key.as_deref(), &model).await
        }
        Commands::OpenApi { action } => match action {
            OpenApiAction::Export { output } => export_openapi(&output),
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
        Commands::Tui => run_tui(),
    }
}

/// 真实起 server: tonic gRPC + salvo HTTP, 后台 tokio 任务, ctrl-c 优雅退出
///
/// `store_path` = Some(path) → SqliteStore 持久化 session
/// `store_path` = None → InMemoryStore (Phase 1 默认)
async fn start_server(grpc_port: u16, http_port: u16, store_path: Option<&std::path::Path>) -> Result<()> {
    let log = EventLog::open_in_memory()?;
    eprintln!("mah start: tonic gRPC on 0.0.0.0:{} + salvo HTTP on 0.0.0.0:{}", grpc_port, http_port);

    // Phase 2.10 (Day 64): 业务方指定 store_path → SqliteStore 持久化
    // Phase 1 默认 InMemoryStore
    let mut builder = ServerBuilder::with_stub(log);
    if let Some(path) = store_path {
        let store = ma_harness_server::SqliteStore::open(path)
            .map_err(|e| anyhow::anyhow!("open sqlite store {}: {e}", path.display()))?;
        eprintln!("mah start: session store = sqlite:{}", path.display());
        builder = builder.with_session_store(Arc::new(store));
    } else {
        eprintln!("mah start: session store = in-memory (no persistence)");
    }

    // tonic gRPC server
    let grpc_addr: std::net::SocketAddr = format!("0.0.0.0:{}", grpc_port).parse()
        .with_context(|| format!("invalid grpc_port: {}", grpc_port))?;
    let agent_svc = builder.build_agent_service();
    let session_svc = builder.build_session_service();
    let grpc_server = tonic::transport::Server::builder()
        .add_service(AgentServiceServer::new(agent_svc))
        .add_service(SessionServiceServer::new(session_svc))
        .serve(grpc_addr);

    // salvo HTTP server
    // 2026-08-18 (Day 52): TcpAcceptor::try_from(tokio::net::TcpListener) — salvo 0.79 API
    use salvo::conn::tcp::TcpAcceptor;
    let http_addr = format!("0.0.0.0:{}", http_port);
    let http_addr_parse: std::net::SocketAddr = http_addr.parse()
        .with_context(|| format!("invalid http_port: {}", http_port))?;
    let http_router = ma_harness_server::http::router();
    let tokio_listener = tokio::net::TcpListener::bind(http_addr_parse).await
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
    eprintln!(
        "mah load-plugin: looking up '{}' in ctx '{}'",
        name, ctx_id
    );
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

/// 跑 conformance fixture, 出报告
fn run_conformance(fixtures_path: &PathBuf, dsh: bool, output: &PathBuf, verbose: bool) -> Result<()> {
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
    eprintln!("Loaded {} fixtures from {}", fixtures.len(), fixtures_path.display());

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
    let runner = CodeRunner::new()
        .map_err(|e| anyhow::anyhow!("init CodeRunner: {e}"))?;
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
            let bytes = std::fs::read(file)
                .with_context(|| format!("read WASM: {}", file.display()))?;
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
    let wat = extract_wat_from_llm_response(&resp.content)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no WAT found in LLM response. Raw content (first 500 chars):\n{}",
                &resp.content.chars().take(500).collect::<String>()
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
fn export_openapi(output: &std::path::Path) -> Result<()> {
    use salvo::oapi::OpenApi;
    use ma_harness_core::StubModelAdapter;
    use std::sync::Arc;

    // 构造完整 router (含 /v1/runs) + stub adapter
    // run_router 内部会 set_global_adapter, OpenAPI 导出只关心 router 结构, 不发真 HTTP
    let router = ma_harness_server::http::run_router(Arc::new(StubModelAdapter));
    let doc = OpenApi::new("ma-harness API", "0.1.0").merge_router(&router);

    // 按扩展名决定 json / yaml
    let ext = output.extension().and_then(|s| s.to_str()).unwrap_or("json");
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
fn run_tui() -> Result<()> {
    let mut app = ma_harness_tui::TuiApp::new()
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
        println!("  - 12 AccessFs ops (ReadFile / ReadDir / WriteFile / RemoveFile / RemoveDir / MakeReg / MakeDir / MakeSock / MakeFifo / MakeBlock / MakeChar / Refer)");
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
