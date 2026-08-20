//! 集成测试: 5 个 proc-macro + ctx_key! 都能编译
//!
//! 验证:
//! 1. ctx_key! 编译期 reject camelCase
//! 2. #[dsh_service] derive 生成 Service impl
//! 3. #[dsh_listener] derive 不报错
//! 4. #[dsh_tool] 生成 schema + invoke
//! 5. #[dsh_command] 生成 clap 入口
//! 6. #[dsh_handler] 不报错 (Phase 1 占位)

use ma_harness_cordis::Context;
use ma_harness_core::ToolSchema;
// 2026-08-19 (Day 101 / P7-0.4): 修 pre-existing broken test
// 1. `ctx_key!` 在 ma_harness_seam (proc-macro crate 不允许 export macro_rules!)
// 2. derive macro `#[proc_macro_derive(DshService, ...)]` 在 proc-macro crate root
//    自动 export 名字 `DshService` (用法 `#[derive(DshService)]`).
//    之前 use `dsh_service / dsh_listener` 错的, 因为这是 `pub fn` 名字 (attribute 形式),
//    不是 derive 名字. derive macro 不需要 import, 在 scope 内自动可访问.
use ma_harness_plugin_macro::{DshListener, DshService, dsh_command, dsh_handler, dsh_tool};
use ma_harness_seam::ctx_key;

// ============================================================================
// ctx_key!
// ============================================================================

static SESSION_ID: ma_harness_cordis::CtxKey<String> = ctx_key!("session_id");
static MAX_TOKENS: ma_harness_cordis::CtxKey<u32> = ctx_key!("max_tokens");

#[test]
fn ctx_key_basic() {
    assert_eq!(SESSION_ID.name(), "session_id");
    assert_eq!(MAX_TOKENS.name(), "max_tokens");
    let ctx = Context::new();
    ctx.set(SESSION_ID, "abc".to_string());
    assert_eq!(ctx.get(SESSION_ID), Some("abc".to_string()));
}

// ============================================================================
// #[dsh_service]
// ============================================================================

/// 测试 service, 验证 #[DshService] 宏自动 impl Service trait
#[derive(DshService)]
pub struct MyTestService;

impl MyTestService {
    // 2026-08-19 (Day 101 / P7-0.4): 修 pre-existing broken test
    // ma_harness_cordis::Service trait 要求 `fn install`, 之前写 `fn new` 不符合 trait.
    // 改 `install` 后 #[DshService] (cordis Service impl) 才能调用 MyTestService::install.
    /// 构造 service (cordis Service trait 要求)
    pub fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(MyTestService)
    }
}

impl MyTestService {
    /// 测试方法, 验证宏生成的 trait impl 正常工作
    pub fn do_something(&self) -> &'static str {
        "done"
    }
}

#[test]
fn dsh_service_implements_service() {
    let ctx = Context::new();
    let svc = MyTestService::install(&ctx).unwrap();
    assert_eq!(svc.do_something(), "done");
}

// ============================================================================
// #[dsh_listener] (Phase 1: 标记用, 不生成代码)
// ============================================================================

/// 测试 listener, 验证 #[DshListener] 宏标记 (Phase 1: 标记用, 不生成代码)
#[derive(DshListener, Default)]
pub struct MyTestListener;

impl MyTestListener {
    /// 构造 listener
    pub fn new() -> Self {
        MyTestListener
    }
}

#[test]
fn dsh_listener_struct_marks() {
    let _ = MyTestListener::new();
    // Phase 1: macro 只加 __DSH_LISTENER 常量
    assert_eq!(MyTestListener::__DSH_LISTENER, ());
}

// ============================================================================
// #[dsh_tool]
// ============================================================================

/// 给 model 看的工具: 加法
// 2026-08-19 (Day 101 / P7-0.4): 参数改 serde_json::Value, 适配 dsh_tool Phase 1
// 简化版 (invoker 传 Value 不 cast). 真实业务方用 #[dsh_tool] 时参数可以是
// 任何 JSON-deserializable type, 但 macro Phase 1 简化要求 fn 接受 Value.
#[dsh_tool]
async fn add(
    // 第一个数
    a: serde_json::Value,
    // 第二个数
    b: serde_json::Value,
) -> anyhow::Result<i64> {
    let a = a.as_i64().unwrap();
    let b = b.as_i64().unwrap();
    Ok(a + b)
}

#[test]
fn dsh_tool_generates_schema() {
    let schema: ToolSchema = add_schema();
    assert_eq!(schema.name, "add");
    assert!(schema.description.contains("加法"));
    assert!(schema.parameters.is_object());
}

#[tokio::test]
async fn dsh_tool_invoke_works() {
    let ctx = Context::new();
    let result = add_invoke(serde_json::json!({"a": 2, "b": 3}), &ctx)
        .await
        .unwrap();
    assert_eq!(result, serde_json::json!(5));
}

#[tokio::test]
async fn dsh_tool_invoke_missing_arg_errors() {
    let ctx = Context::new();
    let result = add_invoke(serde_json::json!({"a": 1}), &ctx).await;
    assert!(result.is_err());
}

// ============================================================================
// #[dsh_command]
// ============================================================================

/// 测试指令: ping
// 2026-08-19 (Day 101 / P7-0.4): 改 fn 签名只接受 ctx, 适配 dsh_command macro
// Phase 1 简化 (dispatch fn 调 `#fn_name(ctx).await`). 之前 verbose: bool 1 个
// 额外 arg + ctx, 跟 macro 展开不匹配 (E0061 1 vs 2 args).
#[dsh_command]
async fn ping(ctx: &Context) -> anyhow::Result<()> {
    ctx.set(SESSION_ID, "ping".to_string());
    Ok(())
}

#[test]
fn dsh_command_generates_clap_cmd() {
    let cmd = ping_clap_cmd();
    assert_eq!(cmd.get_name(), "ping");
}

#[tokio::test]
async fn dsh_command_dispatch_works() {
    let ctx = Context::new();
    let matches = ping_clap_cmd().get_matches_from(vec!["ping"]);
    ping_dispatch(&matches, &ctx).await.unwrap();
    // 默认 verbose=false, ctx 应该 set SESSION_ID = "ping"
    // (但 dsh_command Phase 1 简化: 不解析 arg, 都用 ctx 拿, 这里只是 smoke test)
    assert!(ctx.get(SESSION_ID).is_some());
}

// ============================================================================
// #[dsh_handler] (Phase 1: 透传占位)
// ============================================================================

/// 测试 handler, 验证 #[dsh_handler] 宏 (Phase 1: 透传占位)
#[dsh_handler(adapter = "test_adapter")]
pub async fn test_handler(req: String, _ctx: Context) -> anyhow::Result<String> {
    Ok(req)
}

#[test]
fn dsh_handler_marker() {
    // Phase 1: macro 透传, 不生成额外代码, 只要能编译就过
    // 实际跑 function 跟普通函数一样
    let _ = test_handler; // 不调用 (async), 只确保存在
}

// ============================================================================
// ToolRegistry 简单 smoke (Phase 1 跳过 lambda 类型不匹配问题)
// ============================================================================
