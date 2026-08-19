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
use ma_harness_seam::PluginRegistry;
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
    /// 列出已装载 plugin
    Plugins,
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
        Commands::Events { session } => list_events(&session),
        Commands::Conformance { fixtures, dsh, output, verbose } => {
            run_conformance(&fixtures, dsh, &output, verbose)
        }
        Commands::Bench { crate_name } => print_bench_info(crate_name.as_deref()),
        Commands::Version => {
            println!("mah {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
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
    let mut reg = PluginRegistry::new();
    reg.register(ma_harness_plugin_hello::HelloPlugin)
        .map_err(|e| anyhow::anyhow!("register hello failed: {}", e))?;
    println!("Loaded plugins:");
    for name in reg.list() {
        println!("  - {}", name);
    }
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
