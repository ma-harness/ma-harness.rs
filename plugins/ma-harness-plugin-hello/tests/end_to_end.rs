//! 集成测试: hello plugin 在外部 crate 的端到端使用
//!
//! 验证:
//! 1. 外部 crate 能用 ma_harness_plugin_hello 的 GREETING_TEMPLATE key
//! 2. 装 plugin 后能 ctx.service 拿 HelloService
//! 3. 多个 ctx 共存, 各自独立

use ma_harness_cordis::Context;
use ma_harness_plugin_hello::{HelloPlugin, HelloService, GREETING_TEMPLATE};

#[test]
fn external_crate_uses_hello_plugin() {
    let ctx = Context::new();
    ctx.plugin(HelloPlugin).unwrap();

    // 拿 service
    let svc = ctx
        .service::<HelloService>()
        .expect("HelloService should be injected");
    assert_eq!(svc.name(), "hello");

    // 默认 template
    let default = ctx.get(GREETING_TEMPLATE).expect("default template set");
    assert_eq!(default, "Hello, {who}!");

    // greet 走默认
    assert_eq!(svc.greet(&ctx, "External"), "Hello, External!");
}

#[test]
fn override_template_after_install() {
    let ctx = Context::new();
    ctx.plugin(HelloPlugin).unwrap();
    let svc = ctx.service::<HelloService>().unwrap();

    // 覆盖
    ctx.set(GREETING_TEMPLATE, "Greetings, {who}.".to_string());
    assert_eq!(svc.greet(&ctx, "Stranger"), "Greetings, Stranger.");

    // 再覆盖, 验证 service 是真每次都从 ctx 读, 不是缓存
    ctx.set(GREETING_TEMPLATE, "[{who}]".to_string());
    assert_eq!(svc.greet(&ctx, "Anonymous"), "[Anonymous]");
}

#[test]
fn multiple_contexts_are_independent() {
    let ctx_a = Context::new();
    let ctx_b = Context::new();

    ctx_a.plugin(HelloPlugin).unwrap();
    ctx_b.plugin(HelloPlugin).unwrap();

    // 各自 set 不同 template
    ctx_a.set(GREETING_TEMPLATE, "A: {who}".to_string());
    ctx_b.set(GREETING_TEMPLATE, "B: {who}".to_string());

    let svc_a = ctx_a.service::<HelloService>().unwrap();
    let svc_b = ctx_b.service::<HelloService>().unwrap();

    assert_eq!(svc_a.greet(&ctx_a, "X"), "A: X");
    assert_eq!(svc_b.greet(&ctx_b, "X"), "B: X");

    // 互相不影响
    ctx_a.set(GREETING_TEMPLATE, "A2: {who}".to_string());
    assert_eq!(svc_a.greet(&ctx_a, "X"), "A2: X");
    assert_eq!(svc_b.greet(&ctx_b, "X"), "B: X", "ctx_b 不受 ctx_a 影响");
}

#[test]
fn plugin_uninstall_works() {
    let ctx = Context::new();
    ctx.plugin(HelloPlugin).unwrap();
    assert_eq!(ctx.plugins().len(), 1);

    ctx.uninstall_plugin("hello").unwrap();
    assert_eq!(ctx.plugins().len(), 0);

    // service 还在 (Phase 1 uninstall 默认 no-op, 服务不清除)
    // 验证: 还能 service 拿到
    let svc = ctx.service::<HelloService>();
    assert!(svc.is_some(), "Phase 1 uninstall 不清 service, arc 还活着");
}
