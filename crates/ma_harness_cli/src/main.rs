//! ma-harness CLI 入口 (`mah` 二进制)
//!
//! Week 1 Day 19 实现: clap 路由 (start / run / list / status).
//! Phase 2 加 plugin install / config / log show 等.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
use ma_harness_seam::PluginRegistry;

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
    /// 启动 server (Phase 2: 真实 gRPC + HTTP)
    Start {
        /// 监听端口
        #[arg(long, default_value = "50051")]
        port: u16,
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
    // tracing 初始化
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { port } => {
            eprintln!("mah start (port={}): Phase 1 占位, Phase 2 真实起 server", port);
            // Phase 1 stub: 阻塞直到 ctrl-c
            tokio::signal::ctrl_c().await?;
            Ok(())
        }
        Commands::Run { session, message, model } => {
            let log = EventLog::open_in_memory()?;
            let session_id = session.unwrap_or_else(|| format!("local-{}", uuid::Uuid::new_v4()));
            let agent = AgentLoop::new(log.clone(), std::sync::Arc::new(StubModelAdapter));
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
        Commands::Plugins => {
            let mut reg = PluginRegistry::new();
            // Phase 1 占位: 装 hello plugin (已 commit 的 demo)
            reg.register(ma_harness_plugin_hello::HelloPlugin)
                .map_err(|e| anyhow::anyhow!("register hello failed: {}", e))?;
            println!("Loaded plugins:");
            for name in reg.list() {
                println!("  - {}", name);
            }
            Ok(())
        }
        Commands::Events { session } => {
            let log = EventLog::open_in_memory()?;
            let page = log.get_model_visible(&session)?;
            println!("Session {} ({} events):", session, page.events.len());
            for e in page.events {
                println!("  seq={} type={} severity={}", e.seq, e.event.event_type, e.event.severity);
            }
            Ok(())
        }
        Commands::Version => {
            println!("mah {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
