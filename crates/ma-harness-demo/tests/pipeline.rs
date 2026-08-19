//! ma-harness 端到端 integration test
//!
//! 验证 7 plugin 装载 + AgentLoop + EventLog + service 协作完整跑通.
//! Week 7 PoC 成功判据.

use std::sync::Arc;

use ma_harness_cordis::Context;
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
use ma_harness_plugin_bash::{BashPlugin, BashService, MAX_RUNTIME_MS};
use ma_harness_plugin_cordis::{CordisPlugin, CordisService};
use ma_harness_plugin_fs::{FsPlugin, FsService, READ_ALLOW_LIST};
use ma_harness_plugin_hello::{HelloPlugin, HelloService, GREETING_TEMPLATE};
use ma_harness_plugin_skill::{SkillPlugin, SkillService, SKILLS_DIR};
use ma_harness_plugin_subagent::{SubagentPlugin, SubagentService, MAX_DEPTH};
use ma_harness_plugin_web::{WebPlugin, WebService, EGRESS_ALLOW_LIST};

/// 构造装好 7 plugin 的 ctx
fn make_ctx() -> Context {
    let ctx = Context::new();
    ctx.plugin(HelloPlugin).expect("hello");
    ctx.plugin(BashPlugin).expect("bash");
    ctx.plugin(FsPlugin).expect("fs");
    ctx.plugin(WebPlugin).expect("web");
    ctx.plugin(SubagentPlugin).expect("subagent");
    ctx.plugin(SkillPlugin).expect("skill");
    ctx.plugin(CordisPlugin).expect("cordis");
    ctx
}

#[test]
fn seven_plugins_install() {
    let ctx = make_ctx();
    assert_eq!(ctx.plugins().len(), 7, "应该装 7 个 plugin");

    let mut names = ctx.plugins();
    names.sort();
    assert_eq!(
        names,
        vec!["bash", "cordis", "fs", "hello", "skill", "subagent", "web"]
    );
}

#[test]
fn all_seven_services_injectable() {
    let ctx = make_ctx();
    assert!(ctx.service::<HelloService>().is_some());
    assert!(ctx.service::<BashService>().is_some());
    assert!(ctx.service::<FsService>().is_some());
    assert!(ctx.service::<WebService>().is_some());
    assert!(ctx.service::<SubagentService>().is_some());
    assert!(ctx.service::<SkillService>().is_some());
    assert!(ctx.service::<CordisService>().is_some());
}

#[test]
fn typed_keys_overridable() {
    let ctx = make_ctx();
    // 业务方覆盖
    ctx.set(MAX_RUNTIME_MS, 5000u32);
    assert_eq!(ctx.get(MAX_RUNTIME_MS), Some(5000u32));
    ctx.set(GREETING_TEMPLATE, "Override {who}".to_string());
    assert_eq!(
        ctx.get(GREETING_TEMPLATE).as_deref(),
        Some("Override {who}")
    );
    ctx.set(MAX_DEPTH, 5u32);
    assert_eq!(ctx.get(MAX_DEPTH), Some(5u32));
    ctx.set(SKILLS_DIR, "/custom/skills".to_string());
    assert_eq!(ctx.get(SKILLS_DIR).as_deref(), Some("/custom/skills"));
}

#[tokio::test]
async fn agent_run_emits_four_events() {
    let ctx = make_ctx();
    let log = EventLog::open_in_memory().unwrap();
    let agent = AgentLoop::new(log.clone(), Arc::new(StubModelAdapter));

    let req = AgentRunRequest {
        session_id: "s1".to_string(),
        user_message: "test".to_string(),
        model: "stub".to_string(),
        temperature: 0.0,
        max_tokens: 100,
        system_prompt: None,
    };
    let resp = agent.run(req).await.unwrap();
    assert!(resp.model_response.content.contains("test"));

    // EventLog 应该有 4 个 model-visible 事件
    let page = log.get_model_visible("s1").unwrap();
    assert_eq!(page.events.len(), 4);
}

#[tokio::test]
async fn bash_runs_echo_across_platforms() {
    let ctx = make_ctx();
    ctx.set(MAX_RUNTIME_MS, 5000u32);
    let bash = ctx.service::<BashService>().unwrap();
    let out = bash.run_command(&ctx, "echo integration").await.unwrap();
    assert!(out.is_success());
    assert!(out.stdout.contains("integration"));
}

#[test]
fn hello_greet_uses_overridden_template() {
    let ctx = make_ctx();
    ctx.set(GREETING_TEMPLATE, "Hi {who}!".to_string());
    let hello = ctx.service::<HelloService>().unwrap();
    assert_eq!(hello.greet(&ctx, "test"), "Hi test!");
}

#[test]
fn cordis_inspect_shows_all_plugins() {
    let ctx = make_ctx();
    let cordis = ctx.service::<CordisService>().unwrap();
    let snap = cordis.inspect(&ctx).unwrap();
    assert_eq!(snap.plugin_count, 7);
    assert!(!snap.is_disposed);
}

#[tokio::test]
async fn subagent_spawn_uses_stub() {
    let ctx = make_ctx();
    let sub = ctx.service::<SubagentService>().unwrap();
    let result = sub.spawn_agent(&ctx, "sub-test").await.unwrap();
    assert!(result.content.contains("sub-test"));
    assert!(result.sub_session_id.starts_with("sub-"));
}

#[tokio::test]
async fn fs_blocks_path_outside_allow_list() {
    let ctx = make_ctx();
    // READ_ALLOW_LIST 是 plugin install 时设的空 vec (fail-closed)
    let fs_svc = ctx.service::<FsService>().unwrap();
    let result = fs_svc
        .read_file(&ctx, std::path::Path::new("/etc/passwd"))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn skill_lists_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = make_ctx();
    ctx.set(SKILLS_DIR, tmp.path().to_string_lossy().to_string());
    let skill = ctx.service::<SkillService>().unwrap();
    let list = skill.list_skills(&ctx).await.unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn web_blocks_outside_allow_list() {
    let ctx = make_ctx();
    // EGRESS_ALLOW_LIST 空 (fail-closed)
    let web = ctx.service::<WebService>().unwrap();
    let result = web.http_get(&ctx, "https://anywhere.example.com").await;
    assert!(result.is_err());
}

#[test]
fn ctx_dispose_marks_disposed() {
    let ctx = make_ctx();
    assert!(!ctx.is_disposed());
    ctx.dispose().unwrap();
    assert!(ctx.is_disposed());
}

#[test]
fn ctx_fork_shares_services() {
    let parent = make_ctx();
    // 业务方 override + 设 EGRESS_ALLOW_LIST 给子 ctx
    parent.set(EGRESS_ALLOW_LIST, vec!["https://parent-allowed.example.com".to_string()]);

    let child = parent.fork();
    // service 通过 Arc 共享
    let child_hello = child.service::<HelloService>().unwrap();
    let parent_hello = parent.service::<HelloService>().unwrap();
    assert!(std::sync::Arc::ptr_eq(&child_hello, &parent_hello));

    // typed key 不继承
    assert!(child.get(MAX_RUNTIME_MS).is_none());

    // child 自己的 EGRESS_ALLOW_LIST 也空 (fork 不继承 key)
    assert!(child.get(EGRESS_ALLOW_LIST).is_none());
}
