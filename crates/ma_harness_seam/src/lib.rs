//! ma_harness_seam — 插件抽象层 (公开占位, #[non_exhaustive])
//!
//! **公开 crate** (2026-08-18 锁定). 插件作者**应该** use 这个, 不直接 use `ma_harness_cordis` (内部).
//! 改一次要走 ADR. API 标记 `#[non_exhaustive]` 预留扩展空间.
//!
//! 详细设计见 `docs/ma-harness-arch-map.md` §3 (Seam 类型) + `docs/macro-design.md` (5 个 proc-macro).
//!
//! # Week 1-2 实现
//!
//! 公开 5 个 trait (Plugin / Service / Listener / Disposable / Tool) + 5 个 proc-macro re-export + ctx_key! re-export.
//! 公开 trait 跟 cordis 内部 trait 互转 (seam 提供转换函数, 不强制 impl 同一份).
//!
//! # Phase 1 范围
//!
//! - 5 个 trait (跟 cordis 对齐, 但**trait 独立**, 不强制 impl 同一份)
//! - 5 个 macro re-export
//! - ctx_key! re-export
//! - PluginRegistry 公开 (基于 cordis::PluginRegistry + 包装, 简化 API)
//!
//! # Phase 2 待做
//!
//! - ListenerRegistry / Disposable::Scope 公开包装
//! - ToolRegistry re-export from core
//! - AgentLoop facade

#![deny(unsafe_code)]
#![warn(missing_docs)]

// ============================================================================
// Re-export 5 个 proc-macro + ctx_key!
// ============================================================================

pub use ma_harness_plugin_macro::{
    ctx_key, dsh_command, dsh_handler, dsh_listener, dsh_service, dsh_tool,
};

// ============================================================================
// 公开 trait
// ============================================================================

/// 公开 Service trait (跟 cordis 的 Service 解耦)
///
/// 插件作者 impl 这个, **不**直接 impl `ma_harness_cordis::Service`.
/// seam 内部通过 `IntoCordis` 转换 impl cordis 的 Service.
pub trait Service: Send + Sync + 'static {
    /// 关联的 ctx 类型 (固定是 `ma_harness_cordis::Context`)
    type Ctx = ma_harness_cordis::Context;

    /// 关联的错误类型
    type Error: std::error::Error + Send + Sync + 'static;

    /// 构造自身
    fn install(ctx: &Self::Ctx) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// 实例名
    fn name(&self) -> &str;
}

/// 公开 Plugin trait (跟 cordis 的 Plugin 解耦)
pub trait Plugin: Send + Sync + 'static {
    /// 安装到 ctx
    fn install(&self, ctx: &ma_harness_cordis::Context) -> anyhow::Result<()>;

    /// 插件名
    fn name(&self) -> &str;

    /// 卸载 (Phase 1 默认 no-op)
    fn uninstall(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// 公开 Listener trait (跟 cordis 的 Listener 解耦)
pub trait Listener<E>: Send + Sync + 'static
where
    E: ma_harness_cordis::ListenerEvent,
{
    /// 事件触发时调
    fn handle(&self, ctx: &ma_harness_cordis::Context, event: &E);
}

// blanket impl: 任何 `Fn(&Context, &E) + Send + Sync + 'static` 都是 Listener
impl<F, E> Listener<E> for F
where
    F: Fn(&ma_harness_cordis::Context, &E) + Send + Sync + 'static,
    E: ma_harness_cordis::ListenerEvent,
{
    fn handle(&self, ctx: &ma_harness_cordis::Context, event: &E) {
        self(ctx, event)
    }
}

/// 公开 Disposable trait (跟 cordis 的 Disposable 解耦)
pub trait Disposable: Send + Sync + 'static {
    /// 释放资源
    fn dispose(&self) -> anyhow::Result<()>;
}

/// 公开 Tool trait (model-callable 工具)
///
/// 跟 `ma_harness_core::ToolSchema` / `ToolRegistry` 配套.
pub trait Tool: Send + Sync + 'static {
    /// 工具名
    fn name(&self) -> &str;
    /// 工具描述 (喂给 LLM)
    fn description(&self) -> &str;
    /// JSON Schema
    fn schema(&self) -> serde_json::Value;
    /// 调用 (args JSON, 返回 JSON)
    fn invoke(
        &self,
        args: serde_json::Value,
        ctx: &ma_harness_cordis::Context,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<serde_json::Value>>;
}

// ============================================================================
// Cordis 内部 trait ↔ Seam 公开 trait 转换
// ============================================================================

/// 把公开 Service 转成内部 (impl ma_harness_cordis::Service for SeamService<S>)
pub struct CordisService<S: Service> {
    inner: S,
}

impl<S: Service> CordisService<S> {
    /// 包装一个公开 Service 成内部
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    /// 解包
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: Service> ma_harness_cordis::Service for CordisService<S> {
    type Ctx = ma_harness_cordis::Context;
    type Error = S::Error;
    fn install(ctx: &Self::Ctx) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        Ok(CordisService { inner: S::install(ctx)? })
    }
    fn name(&self) -> &str {
        self.inner.name()
    }
}

/// 把公开 Plugin 转成内部
pub struct CordisPlugin<P: Plugin> {
    inner: P,
}

impl<P: Plugin> CordisPlugin<P> {
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
    pub fn into_inner(self) -> P {
        self.inner
    }
}

impl<P: Plugin> ma_harness_cordis::Plugin for CordisPlugin<P> {
    fn install(&self, ctx: &ma_harness_cordis::Context) -> anyhow::Result<()> {
        self.inner.install(ctx)
    }
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn uninstall(&self) -> anyhow::Result<()> {
        self.inner.uninstall()
    }
}

// ============================================================================
// 公开注册表: 简单包装 cordis 的注册表
// ============================================================================

/// 公开 PluginRegistry
pub struct PluginRegistry {
    inner: ma_harness_cordis::Context,
}

impl PluginRegistry {
    /// 构造一个新 registry
    pub fn new() -> Self {
        Self {
            inner: ma_harness_cordis::Context::new(),
        }
    }

    /// 注册一个公开 Plugin
    pub fn register<P: Plugin>(&mut self, plugin: P) -> anyhow::Result<()> {
        self.inner.plugin(CordisPlugin::new(plugin)).map_err(Into::into)
    }

    /// 列出所有 plugin
    pub fn list(&self) -> Vec<String> {
        self.inner.plugins()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 公开 Dispatcher: 加载 plugin.toml + 装载 first-party 插件
// ============================================================================

/// PluginLoader — 简化 API: 拿一个 plugin name, 装进 ctx
///
/// **Phase 1 stub**: 只暴露 trait 边界, 不实际加载. 真正加载走 Phase 2 的 inventory
/// 动态加载 + plugin.toml entry 解析.
pub struct PluginLoader;

impl PluginLoader {
    /// Phase 1 stub: 返回 "未实现" 错误
    ///
    /// Phase 2 改成读 plugin.toml 找 entry, 动态加载编译时 link 的 plugin.
    pub fn load_by_name(
        _ctx: &ma_harness_cordis::Context,
        _name: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!(
            "PluginLoader::load_by_name 尚未实现. Phase 2 加 inventory + plugin.toml 解析."
        )
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_cordis::Context;

    struct MyService {
        greeting: String,
    }

    impl Service for MyService {
        type Error = anyhow::Error;
        fn install(_ctx: &Context) -> anyhow::Result<Self> {
            Ok(MyService {
                greeting: "hi".to_string(),
            })
        }
        fn name(&self) -> &str {
            "my_service"
        }
    }

    struct MyPlugin;

    impl Plugin for MyPlugin {
        fn install(&self, ctx: &Context) -> anyhow::Result<()> {
            let s = MyService::install(ctx)?;
            assert_eq!(s.name(), "my_service");
            assert_eq!(s.greeting, "hi");
            Ok(())
        }
        fn name(&self) -> &str {
            "my_plugin"
        }
    }

    #[test]
    fn seam_service_can_install() {
        let ctx = Context::new();
        let s = MyService::install(&ctx).unwrap();
        assert_eq!(s.name(), "my_service");
    }

    #[test]
    fn cordis_service_wraps_seam_service() {
        let ctx = Context::new();
        let s = MyService::install(&ctx).unwrap();
        let cordis_svc = CordisService::new(s);
        // CordisService impl ma_harness_cordis::Service, 调 name
        assert_eq!(cordis_svc.name(), "my_service");
    }

    #[test]
    fn plugin_registry_works() {
        let mut reg = PluginRegistry::new();
        reg.register(MyPlugin).unwrap();
        assert_eq!(reg.list(), vec!["my_plugin".to_string()]);
    }

    #[test]
    fn plugin_loader_load_by_name_not_implemented() {
        let ctx = Context::new();
        let result = PluginLoader::load_by_name(&ctx, "hello");
        assert!(result.is_err());
    }
}
