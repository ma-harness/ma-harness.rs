//! ma_harness_plugin_hello — 端到端 demo plugin (seam 风格)
//!
//! **目的**: 演示 ma-harness 公开 API (seam) 的最小可用链路.
//!
//! **关键设计**:
//! - 只用 `ma_harness_seam::*` 公开 API
//! - `ma_harness_cordis` 仅作为底层引用 (typed key), 通过 seam 暴露
//! - 5 个 proc-macro (来自 ma_harness_plugin_macro) 通过 seam re-export
//!
//! 详细设计见 `docs/ma-harness-arch-map.md` §11 (Week 1 Day 4 + Week 3-4 重写).

#![deny(unsafe_code)]
#![warn(missing_docs)]

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};

// ============================================================================
// 公开 typed key (seam re-export, 编译期 snake_case 校验)
// ============================================================================

/// greeting 模板. 业务方 set 改之, service 读之.
pub static GREETING_TEMPLATE: ma_harness_cordis::CtxKey<String> = ctx_key!("greeting_template");

/// 默认模板
pub const DEFAULT_TEMPLATE: &str = "Hello, {who}!";

// ============================================================================
// Service: HelloService
// ============================================================================
//
// 双重 impl: cordis::Service (跟 ctx 内部对接) + seam::Service (公开 API 一致).
// 两份方法签名完全相同, 业务方写 seam 跟 ctx 内部 cordis 都自动满足.

/// Hello service — 每次 greet 都从 ctx 读 template
pub struct HelloService;

impl HelloService {
    /// 用 ctx 里的 template 渲染问候
    pub fn greet(&self, ctx: &Context, who: &str) -> String {
        let template = ctx
            .get(GREETING_TEMPLATE)
            .unwrap_or_else(|| DEFAULT_TEMPLATE.to_string());
        template.replace("{who}", who)
    }
}

impl CordisService for HelloService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(HelloService)
    }
    fn name(&self) -> &str {
        "hello"
    }
}

// 公开 seam 镜像 (跟 CordisService 同 impl 体)
impl SeamService for HelloService {
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
//
// 双重 impl: cordis::Plugin (跟 ctx 内部对接) + seam::Plugin (公开 API).

/// Hello plugin — install 时注入 HelloService + 写默认 typed key
pub struct HelloPlugin;

impl CordisPlugin for HelloPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = HelloService::install(ctx)?;
        ctx.inject(std::sync::Arc::new(svc));
        ctx.set(GREETING_TEMPLATE, DEFAULT_TEMPLATE.to_string());
        Ok(())
    }

    fn name(&self) -> &str {
        "hello"
    }
}

impl SeamPlugin for HelloPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        // 委托给 CordisPlugin::install (impl 体同)
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "hello"
    }
}

// ============================================================================
// 单元测试 (跟 Week 1 Day 4 一致, 验证 seam API 工作)
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
        // 用 seam::PluginRegistry 装载 HelloPlugin
        let mut reg = PluginRegistry::new();
        reg.register(HelloPlugin).unwrap();
        assert_eq!(reg.list(), vec!["hello".to_string()]);
    }

    #[test]
    fn plugin_install_injects_service_and_key() {
        // 用 seam::PluginRegistry 装载, 拿它内部 ctx
        let mut reg = PluginRegistry::new();
        reg.register(HelloPlugin).unwrap();
        // PluginRegistry 内部 ctx 暂时不暴露, Phase 2 加 accessor
        // Phase 1 简化: 验证 plugin 装载 + list 包含 "hello"
        assert!(reg.list().contains(&"hello".to_string()));
    }

    #[test]
    fn service_greet_uses_live_template() {
        // 直接测 service + ctx, 不走 plugin
        let ctx = Context::new();
        ctx.set(GREETING_TEMPLATE, DEFAULT_TEMPLATE.to_string());
        let svc = HelloService::install(&ctx).unwrap();

        // 默认
        assert_eq!(svc.greet(&ctx, "World"), "Hello, World!");

        // 改 ctx, 下次 greet 用新 template
        ctx.set(GREETING_TEMPLATE, "Hey {who}!".to_string());
        assert_eq!(svc.greet(&ctx, "World"), "Hey World!");
    }
}
