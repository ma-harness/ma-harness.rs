//! ma_harness_plugin_hello — 端到端 demo plugin
//!
//! **目的**: 验证 cordis 元框架的最小可用链路:
//!   plugin install → ctx 注入 service → typed key 读写 → service 行为.
//!
//! **不**走 5 个 #[dsh_*] proc-macro (那些 Week 2-3 才实现), 用手写 Service / Plugin impl.
//!
//! 详细设计见 `docs/ma-harness-arch-map.md` §11 (Week 1 Day 4).

#![deny(unsafe_code)]
#![warn(missing_docs)]

use ma_harness_cordis::{Context, CtxKey, Plugin, Service};
use ma_harness_plugin_macro::ctx_key;

// ============================================================================
// 公开 typed key (编译期 snake_case 校验, 来自 ctx_key! macro)
// ============================================================================

/// greeting 模板. 业务方 set 改之, service 读之.
pub static GREETING_TEMPLATE: CtxKey<String> = ctx_key!("greeting_template");

/// 默认模板 (hello plugin install 时写入 ctx)
pub const DEFAULT_TEMPLATE: &str = "Hello, {who}!";

// ============================================================================
// Service: HelloService
// ============================================================================

/// Hello service — 每次 greet 都从 ctx 读 template, 演示 ctx 是活的 DI 容器
///
/// **关键设计**: service 不存 template, 每次调用都从 ctx 读.
/// 这样业务方 set GREETING_TEMPLATE 改值, 下次 greet 立刻生效.
pub struct HelloService;

impl HelloService {
    /// 用 ctx 里的 template 渲染问候
    pub fn greet(&self, ctx: &Context, who: &str) -> String {
        let template = ctx
            .get(GREETING_TEMPLATE)
            .unwrap_or_else(|| DEFAULT_TEMPLATE.to_string());
        template.replace("{who}", who)
    }

    /// 列出 ctx 里所有 typed key 的名字 (调试用)
    pub fn list_keys(&self, ctx: &Context) -> Vec<String> {
        // Week 2 加 ctx.keys() API. 现在占位.
        vec!["greeting_template".to_string()]
    }
}

impl Service for HelloService {
    type Error = anyhow::Error;

    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(HelloService)
    }

    fn name(&self) -> &str {
        "hello"
    }
}

// ============================================================================
// Plugin: HelloPlugin
// ============================================================================

/// Hello plugin — install 时注入 HelloService + 写默认 typed key
pub struct HelloPlugin;

impl Plugin for HelloPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        // 1. 注入 service (用 inject_from 便捷方法, 内部调 install)
        ctx.inject_from::<HelloService>()
            .map_err(|e| anyhow::anyhow!("failed to inject HelloService: {}", e))?;

        // 2. 写默认 template (业务方可以覆盖)
        ctx.set(GREETING_TEMPLATE, DEFAULT_TEMPLATE.to_string());

        tracing::info!("hello plugin installed");
        Ok(())
    }

    fn name(&self) -> &str {
        "hello"
    }
}

// ============================================================================
// 单元测试 (Week 1 Day 4 端到端, 不依赖 hello plugin install, 测 service 本身)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_with_default_template() {
        let ctx = Context::new();
        ctx.set(GREETING_TEMPLATE, DEFAULT_TEMPLATE.to_string());

        let svc = HelloService;
        assert_eq!(svc.greet(&ctx, "World"), "Hello, World!");
        assert_eq!(svc.greet(&ctx, "Alice"), "Hello, Alice!");
    }

    #[test]
    fn greet_with_custom_template() {
        let ctx = Context::new();
        // 业务方覆盖默认 template
        ctx.set(GREETING_TEMPLATE, "Hi {who}!".to_string());

        let svc = HelloService;
        assert_eq!(svc.greet(&ctx, "Bob"), "Hi Bob!");
    }

    #[test]
    fn greet_chinese_template() {
        let ctx = Context::new();
        ctx.set(GREETING_TEMPLATE, "{who}, 你好!".to_string());

        let svc = HelloService;
        assert_eq!(svc.greet(&ctx, "小明"), "小明, 你好!");
    }

    #[test]
    fn greet_without_template_uses_default() {
        // 没 set template, service 用 hard-coded DEFAULT_TEMPLATE
        let ctx = Context::new();
        let svc = HelloService;
        assert_eq!(svc.greet(&ctx, "Anonymous"), "Hello, Anonymous!");
    }

    #[test]
    fn service_install_returns_instance() {
        let ctx = Context::new();
        let svc = HelloService::install(&ctx).unwrap();
        assert_eq!(svc.name(), "hello");
    }

    #[test]
    fn plugin_install_injects_service_and_default_key() {
        let ctx = Context::new();
        ctx.plugin(HelloPlugin).unwrap();

        // 验证 service 注入
        let svc = ctx.service::<HelloService>().expect("service should be injected");
        assert_eq!(svc.name(), "hello");

        // 验证 typed key 默认值
        let template = ctx.get(GREETING_TEMPLATE).expect("template should be set");
        assert_eq!(template, DEFAULT_TEMPLATE);

        // 验证 service 能用 (改了 ctx 里的 template 后行为变化)
        ctx.set(GREETING_TEMPLATE, "Hey {who}!".to_string());
        assert_eq!(svc.greet(&ctx, "World"), "Hey World!");

        // 改回默认
        ctx.set(GREETING_TEMPLATE, DEFAULT_TEMPLATE.to_string());
        assert_eq!(svc.greet(&ctx, "World"), "Hello, World!");
    }

    #[test]
    fn plugin_listed_after_install() {
        let ctx = Context::new();
        assert!(ctx.plugins().is_empty());
        ctx.plugin(HelloPlugin).unwrap();
        assert_eq!(ctx.plugins(), vec!["hello".to_string()]);
    }

    #[test]
    fn ctx_extend_shares_hello_service() {
        // 父 ctx 装 hello plugin, 子 ctx extend, 子也能用 HelloService
        let parent = Context::new();
        parent.plugin(HelloPlugin).unwrap();

        let child = Context::new();
        // 子 ctx 自己 set template, 跟父 ctx 独立
        child.set(GREETING_TEMPLATE, "Sub: {who}".to_string());

        child.extend_from(&parent);
        // 子 ctx 拿到 HelloService (引用父的 Arc)
        let svc = child.service::<HelloService>().expect("extend should share service");

        // 子 ctx 自己的 template 生效 (不是父的 default)
        assert_eq!(svc.greet(&child, "User"), "Sub: User");

        // 父 ctx 用自己的 default template
        let parent_svc = parent.service::<HelloService>().unwrap();
        assert_eq!(parent_svc.greet(&parent, "User"), "Hello, User!");
    }
}
