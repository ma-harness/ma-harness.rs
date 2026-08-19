//! ma-harness 端到端 demo
//!
//! **目的**: Week 7 PoC 成功判据 — Default 模式完整跑通
//!
//! **流程**:
//! 1. 构造 ctx (内部 ma_harness_cordis::Context)
//! 2. 装载 6 first-party 插件 + hello plugin
//! 3. 跑一次 AgentLoop (StubModelAdapter, model adapter 是 Phase 1 简化)
//! 4. 查 EventLog 输出所有 model-visible 事件
//! 5. 用 BashService 跑一个 shell 命令 (演示多插件协作)
//!
//! **运行**: `cargo run -p ma_harness_demo`

#![warn(missing_docs)]

use std::sync::Arc;

use anyhow::Result;
use ma_harness_cordis::Context;
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
use ma_harness_plugin_bash::{BashPlugin, BashService, MAX_RUNTIME_MS};
use ma_harness_plugin_cordis::{CordisPlugin, CordisService};
use ma_harness_plugin_fs::{FsPlugin, FsService, READ_ALLOW_LIST};
use ma_harness_plugin_hello::{HelloPlugin, HelloService, GREETING_TEMPLATE};
use ma_harness_plugin_skill::{SkillPlugin, SkillService, SKILLS_DIR};
use ma_harness_plugin_subagent::{SubagentPlugin, SubagentService, MAX_DEPTH};
use ma_harness_plugin_web::{WebPlugin, WebService, EGRESS_ALLOW_LIST};

#[tokio::main]
async fn main() -> Result<()> {
    // tracing
    tracing_subscriber::fmt::init();

    println!("========================================");
    println!("ma-harness 端到端 demo (Week 7)");
    println!("========================================\n");

    // ------------------------------------------------------------------
    // 1. 构造 ctx + 装 6 first-party + hello plugin
    // ------------------------------------------------------------------
    println!("[1] 构造 ctx + 装 7 plugin");
    let ctx = Context::new();
    ctx.plugin(HelloPlugin).expect("install hello");
    ctx.plugin(BashPlugin).expect("install bash");
    ctx.plugin(FsPlugin).expect("install fs");
    ctx.plugin(WebPlugin).expect("install web");
    ctx.plugin(SubagentPlugin).expect("install subagent");
    ctx.plugin(SkillPlugin).expect("install skill");
    ctx.plugin(CordisPlugin).expect("install cordis");

    println!("    已装载 plugins: {:?}", ctx.plugins());
    assert_eq!(ctx.plugins().len(), 7);

    // ------------------------------------------------------------------
    // 2. 业务方覆盖默认 typed key (演示 "活的 ctx")
    // ------------------------------------------------------------------
    println!("\n[2] 业务方覆盖默认 typed key");
    ctx.set(MAX_RUNTIME_MS, 10_000u32);  // bash 默认 10s
    ctx.set(READ_ALLOW_LIST, vec!["D:\\workspace\\learn\\rust\\ma-harness.rs".to_string()]);
    ctx.set(EGRESS_ALLOW_LIST, vec!["https://api.github.com".to_string()]);
    ctx.set(MAX_DEPTH, 2u32);  // subagent 最多 2 层递归
    ctx.set(SKILLS_DIR, "./skills".to_string());
    ctx.set(GREETING_TEMPLATE, "Hi {who}, welcome!".to_string());

    // ------------------------------------------------------------------
    // 3. 跑 AgentLoop (StubModelAdapter, model adapter Phase 1 简化)
    // ------------------------------------------------------------------
    println!("\n[3] 跑 AgentLoop (StubModelAdapter)");
    let log = EventLog::open_in_memory().expect("open in-memory log");
    let agent = AgentLoop::new(log.clone(), Arc::new(StubModelAdapter));

    let req = AgentRunRequest {
        session_id: "demo-session".to_string(),
        user_message: "hello world".to_string(),
        model: "stub".to_string(),
        temperature: 0.7,
        max_tokens: 1024,
        system_prompt: None,
    };
    let resp = agent.run(req).await?;
    println!("    session: {}", resp.session_id);
    println!("    run_id: {}", resp.run_id);
    println!("    content: {}", resp.model_response.content);
    println!("    tokens: prompt={} completion={}",
        resp.total_prompt_tokens, resp.total_completion_tokens);

    // ------------------------------------------------------------------
    // 4. 查 EventLog, 输出所有 model-visible 事件
    // ------------------------------------------------------------------
    println!("\n[4] EventLog 查询 model-visible 事件");
    let page = log.get_model_visible("demo-session")?;
    println!("    找到 {} 个 model-visible 事件:", page.events.len());
    for e in &page.events {
        println!("    - seq={:>3} type={:<20} severity={:<5}",
            e.seq, e.event.event_type, e.event.severity);
    }
    assert_eq!(page.events.len(), 4, "应该 4 个 model-visible 事件 (RunStart/ModelRequest/ModelResponse/RunEnd)");

    // ------------------------------------------------------------------
    // 5. 演示 BashService 跑 shell 命令
    // ------------------------------------------------------------------
    println!("\n[5] BashService 跑 shell 命令");
    let bash = ctx.service::<BashService>().expect("BashService 注入");
    let output = bash.run_command(&ctx, "echo hello from bash").await?;
    println!("    exit_code: {}", output.exit_code);
    println!("    stdout: {}", output.stdout.trim());
    println!("    duration: {}ms", output.duration_ms);
    assert!(output.is_success());

    // ------------------------------------------------------------------
    // 6. 演示 HelloService (用业务方覆盖的 template)
    // ------------------------------------------------------------------
    println!("\n[6] HelloService 用覆盖的 template");
    let hello = ctx.service::<HelloService>().expect("HelloService 注入");
    let greeting = hello.greet(&ctx, "yifenma");
    println!("    greeting: {}", greeting);
    assert_eq!(greeting, "Hi yifenma, welcome!");

    // ------------------------------------------------------------------
    // 7. 演示 CordisService 反射
    // ------------------------------------------------------------------
    println!("\n[7] CordisService 反射 ctx");
    let cordis = ctx.service::<CordisService>().expect("CordisService 注入");
    let snap = cordis.inspect(&ctx).expect("inspect");
    println!("    plugin_count: {}", snap.plugin_count);
    println!("    plugins: {:?}", snap.plugins);
    println!("    is_disposed: {}", snap.is_disposed);
    assert_eq!(snap.plugin_count, 7);
    assert!(!snap.is_disposed);

    // ------------------------------------------------------------------
    // 8. 演示 SubagentService fork ctx
    // ------------------------------------------------------------------
    println!("\n[8] SubagentService fork ctx 跑子 agent");
    let sub = ctx.service::<SubagentService>().expect("SubagentService 注入");
    let sub_result = sub.spawn_agent(&ctx, "sub hello").await?;
    println!("    sub_session_id: {}", sub_result.sub_session_id);
    println!("    sub_content: {}", sub_result.content);
    assert!(sub_result.content.contains("sub hello"));

    // ------------------------------------------------------------------
    // 9. 演示 FsService 读文件 (用了 READ_ALLOW_LIST)
    // ------------------------------------------------------------------
    println!("\n[9] FsService 读 README.md (在白名单内)");
    let fs_svc = ctx.service::<FsService>().expect("FsService 注入");
    let readme = std::path::PathBuf::from("D:\\workspace\\learn\\rust\\ma-harness.rs\\README.md");
    if readme.exists() {
        let content = fs_svc.read_file(&ctx, &readme).await?;
        let first_line = content.lines().next().unwrap_or("");
        println!("    first line: {}", first_line);
    } else {
        println!("    README.md 不存在, 跳过");
    }

    // ------------------------------------------------------------------
    // 10. 演示 SkillService 列 skill
    // ------------------------------------------------------------------
    println!("\n[10] SkillService 列 skills (空目录预期)");
    let skill = ctx.service::<SkillService>().expect("SkillService 注入");
    let skills = skill.list_skills(&ctx).await?;
    println!("    skills: {:?}", skills);
    // 不需要 assert (空目录正常)

    // ------------------------------------------------------------------
    // 11. 演示 WebService URL 白名单 (不调, 只 inspect)
    // ------------------------------------------------------------------
    println!("\n[11] WebService 实例化 (白名单生效, 不真发请求)");
    let _web = ctx.service::<WebService>().expect("WebService 注入");
    println!("    EGRESS_ALLOW_LIST: {:?}", ctx.get(EGRESS_ALLOW_LIST));

    // ------------------------------------------------------------------
    // 12. 释放 ctx
    // ------------------------------------------------------------------
    println!("\n[12] ctx.dispose()");
    ctx.dispose()?;
    println!("    is_disposed: {}", ctx.is_disposed());
    assert!(ctx.is_disposed());

    println!("\n========================================");
    println!("端到端 demo 完成 (Week 7 PoC 成功判据 ✓)");
    println!("========================================");
    Ok(())
}
