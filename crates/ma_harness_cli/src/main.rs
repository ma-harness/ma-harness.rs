//! ma-harness CLI 入口 (`mah` 二进制)
//!
//! Week 7-8 实现: 5 子命令 + server 真实起 (tonic + axum).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
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
    /// 启动 server (tonic gRPC + axum HTTP)
    Start {
        /// gRPC 监听端口
        #[arg(long, default_value = "50051")]
        grpc_port: u16,
        /// HTTP 监听端口
        #[arg(long, default_value = "50050")]
        http_port: u16,
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
    /// 版本
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { grpc_port, http_port } => {
            start_server(grpc_port, http_port).await
        }
        Commands::Run { session, message, model } => {
            run_local_agent(session, message, model).await
        }
        Commands::Plugins => list_plugins(),
        Commands::Events { session } => list_events(&session),
        Commands::Version => {
            println!("mah {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

/// 真实起 server: tonic gRPC + axum HTTP, 后台 tokio 任务, ctrl-c 优雅退出
async fn start_server(grpc_port: u16, http_port: u16) -> Result<()> {
    let log = EventLog::open_in_memory()?;
    let builder = ServerBuilder::with_stub(log);
    let agent = builder.build_agent_service();
    let session = builder.build_session_service();

    let grpc_addr = format!("0.0.0.0:{}", grpc_port).parse()?;
    let http_addr = format!("0.0.0.0:{}", http_port).parse()?;

    eprintln!("mah start: tonic gRPC on {}, axum HTTP on {}", grpc_addr, http_addr);

    // tonic gRPC
    let grpc_server = tonic::transport::Server::builder()
        .add_service(AgentServiceServer::new(agent))
        .add_service(SessionServiceServer::new(session))
        .serve(grpc_addr);

    // axum HTTP (Phase 1: /health + /version)
    let http_router = ma_harness_server::http::router();
    let http_server = axum::Server::bind(&http_addr).serve(http_router.into_make_service());

    // 并发跑两个 server, 等 ctrl-c
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
