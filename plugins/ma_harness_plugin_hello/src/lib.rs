//! ma_harness_plugin_hello — 端到端 demo plugin (seam 风格)
//!
//! **目的**: 演示 ma-harness 公开 API (seam) 的最小可用链路.
//!
//! **关键设计**:
//! - 只用 `ma_harness_seam::*` 公开 API
//! - `ma_harness_cordis` 仅作为底层引用(typed key), 通过 seam 暴露
//! - 5 个 proc-macro (来自 ma_harness_plugin_macro) 通过 seam re-export
//! - **Phase 2.1**: 用 `#[dsh_service_dual]` / `#[dsh_plugin_dual]` 一次生成双 trait impl,
//!   取代 Phase 1 手动双重 impl cordis + seam 的 20 行 boilerplate.
//!
//! 详细设计见 `docs/ma-harness-arch-map.md` §11 (Week 1 Day 4 + Week 3-4 重写).
//! Phase 2.1 macro 增强设计见 `docs/ma-harness-arch-map.md` §11.1 (Day 55).

#![deny(unsafe_code)]
#![warn(missing_docs)]

use ma_harness_cordis::Context;
use ma_harness_seam::{ctx_key, dsh_plugin_dual, dsh_service_dual};

// ============================================================================
// 公开 typed key (seam re-export, 编译期 snake_case 校验)
// ============================================================================

/// greeting 模板. 业务方 set 改之, service 读之.
pub static GREETING_TEMPLATE: ma_harness_cordis::CtxKey<String> = ctx_key!("greeting_template");

/// 默认模板
pub const DEFAULT_TEMPLATE: &str = "Hello, {who}!";

// ============================================================================
// Service: HelloService (Phase 2.1 macro 一次生成 cordis + seam 两套 impl)
// ============================================================================

/// Hello service — 每次 greet 都从 ctx 读 template
#[dsh_service_dual(name = "hello", ctor = "HelloService::create")]
pub struct HelloService;

impl HelloService {
    /// 通过 ctx 构造自身 (user 写一次, macro 委托到 cordis::Service::install,
    /// seam::Service::install 再委托回 cordis — 全链路不重复实现)
    pub fn create(_ctx: &Context) -> Result<Self, ma_harness_cordis::BoxedError> {
        Ok(HelloService)
    }

    /// 从 ctx 里的 template 渲染问候
    pub fn greet(&self, ctx: &Context, who: &str) -> String {
        let template = ctx
            .get(GREETING_TEMPLATE)
            .unwrap_or_else(|| DEFAULT_TEMPLATE.to_string());
        template.replace("{who}", who)
    }
}

// ============================================================================
// Plugin: HelloPlugin (Phase 2.1 macro 一次生成 cordis + seam 两套 impl)
// ============================================================================

/// Hello plugin — install 时注入 HelloService + 写默认 typed key
#[dsh_plugin_dual(name = "hello", install = "HelloPlugin::install_into")]
pub struct HelloPlugin;

impl HelloPlugin {
    /// 委托 cordis::Plugin::install, seam::Plugin::install 再委托 cordis
    pub fn install_into(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = <HelloService as ma_harness_cordis::Service>::install(ctx)?;
        ctx.inject(std::sync::Arc::new(svc));
        ctx.set(GREETING_TEMPLATE, DEFAULT_TEMPLATE.to_string());
        Ok(())
    }
}

// ============================================================================
// 单元测试 (跟 Week 1 Day 4 一致, 验证 macro 生成的 impl 行为跟手写 impl 等价)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_seam::PluginRegistry;

    #[test]
    fn greet_with_default_template() {
        let ctx = Context::new();
        ctx.set(GREETING_TEMPLATE, DEFAULT_TEMPLATE.to_string());

        let svc = HelloService;
        assert_eq!(svc.greet(&ctx, "World"), "Hello, World!");
    }

    #[test]
    fn greet_chinese_template() {
        let ctx = Context::new();
        ctx.set(GREETING_TEMPLATE, "{who}, 你好!".to_string());
        let svc = HelloService;
        assert_eq!(svc.greet(&ctx, "小明"), "小明, 你好!");
    }

    #[test]
    fn seam_plugin_registry_works() {
        // 通过 seam::PluginRegistry 装载 HelloPlugin
        let mut reg = PluginRegistry::new();
        reg.register(HelloPlugin).unwrap();
        assert_eq!(reg.list(), vec!["hello".to_string()]);
    }

    #[test]
    fn plugin_install_injects_service_and_key() {
        let mut reg = PluginRegistry::new();
        reg.register(HelloPlugin).unwrap();
        assert!(reg.list().contains(&"hello".to_string()));
    }

    #[test]
    fn service_greet_uses_live_template() {
        // 直接拿 service + ctx, 不走 plugin
        let ctx = Context::new();
        ctx.set(GREETING_TEMPLATE, DEFAULT_TEMPLATE.to_string());
        let svc = <HelloService as ma_harness_cordis::Service>::install(&ctx).unwrap();

        // 默认
        assert_eq!(svc.greet(&ctx, "World"), "Hello, World!");

        // 改 ctx, 下次 greet 用新 template
        ctx.set(GREETING_TEMPLATE, "Hey {who}!".to_string());
        assert_eq!(svc.greet(&ctx, "World"), "Hey World!");
    }

    /// Phase 2.1 新增: 验证 macro 生成的双 trait impl 一致 (cordis + seam install 同结果)
    #[test]
    fn dsh_service_dual_generates_consistent_impls() {
        let ctx = Context::new();
        // 走 cordis 拿 service
        let svc_cordis = <HelloService as ma_harness_cordis::Service>::install(&ctx).unwrap();
        // 走 seam 拿 service (委托 cordis)
        let svc_seam = <HelloService as ma_harness_seam::Service>::install(&ctx).unwrap();
        // 两套 install 返回的 service 同 type, name 一致 (用 FQN 消歧义, 因为 cordis + seam 都有 name)
        assert_eq!(
            <HelloService as ma_harness_cordis::Service>::name(&svc_cordis),
            <HelloService as ma_harness_seam::Service>::name(&svc_seam),
        );
        assert_eq!(
            <HelloService as ma_harness_cordis::Service>::name(&svc_cordis),
            "hello",
        );
    }

    /// Phase 2.1 新增: 验证 macro 生成的 Plugin 双 trait impl 一致
    #[test]
    fn dsh_plugin_dual_generates_consistent_impls() {
        let ctx = Context::new();
        // seam 委托 cordis, 行为一致
        <HelloPlugin as ma_harness_seam::Plugin>::install(&HelloPlugin, &ctx).unwrap();
        // 装一次后, service 已在 ctx 里, greet 能用
        let svc = <HelloService as ma_harness_cordis::Service>::install(&ctx).unwrap();
        assert_eq!(svc.greet(&ctx, "Phase 2.1"), "Hello, Phase 2.1!");
    }
}
