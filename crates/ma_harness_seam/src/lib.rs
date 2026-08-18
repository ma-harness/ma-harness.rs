//! ma_harness_seam — 插件抽象层 (公开占位, #[non_exhaustive])
//!
//! **公开 crate** (2026-08-18 锁定). 插件作者**应该** use 这个, 不直接 use `ma_harness_cordis` (内部).
//! 改一次要 ADR. API 标记 `#[non_exhaustive]` 预留扩展空间.
//!
//! 详细设计见 `docs/ma-harness-arch-map.md` §3 (Seam 类型) + `docs/macro-design.md` (5 个 proc-macro).
//!
//! # Week 1-2 实现
//!
//! 公开 5 个 trait (Plugin / Service / Listener / Disposable / Tool) + 5 个 proc-macro re-export + ctx_key! re-export.
//! 公开 trait 跟 cordis 内部 trait 互转 (seam 提供转换函数, 不强制 impl 同一 trait).
//!
//! # Phase 1 范围
//!
//! - 5 个 trait (跟 cordis 对齐, 但每个 trait 独立, 不强制 impl 同一 trait)
//! - 5 个 macro re-export
//! - ctx_key! re-export
//! - PluginRegistry 公开 (基于 cordis::PluginRegistry + 包装, 简单 API)
//!
//! # Phase 2 待做
//!
//! - ListenerRegistry / Disposable::Scope 公开包装
//! - ToolRegistry re-export from core
//! - AgentLoop facade

#![deny(unsafe_code)]
#![warn(missing_docs)]

// ============================================================================
// Re-export 5 个 proc-macro + ctx_key! (从 ma_harness_plugin_macro re-export)
// ============================================================================

// 2026-08-18: derive macro 用驼峰名 (Rust 规则), attribute macro 用蛇形.
// derive: DshService, DshListener (写 `#[derive(DshService)]`)
// attribute: dsh_tool, dsh_command, dsh_handler (写 `#[dsh_tool(...)]`)
// 公开 API 名字保持 dsh_ 开头 (用户一致), derive 没办法, attribute 不变
pub use ma_harness_plugin_macro::{
    DshListener, DshService, dsh_command, dsh_handler, dsh_tool, on as dsh_listener_on,
};

// 2026-08-18: re-export CtxKey + is_snake_case 让 ctx_key! macro 能用
// (ctx_key! 是 macro_rules! defined in this crate, 用 $crate::* 引用)
pub use ma_harness_cordis::{is_snake_case, CtxKey};

// ============================================================================
// ctx_key! — 编译期 snake_case 校验
// ============================================================================
//
// 2026-08-18: 从 ma_harness_plugin_macro 移到 seam (proc-macro crate 不允许
// export macro_rules!, 这是 Rust 语言限制).
//
// seam 不是 proc-macro crate, 可以 export macro_rules!.

/// 构造一个 [`ma_harness_cordis::CtxKey`], 编译期 reject 非 snake_case 名字.
///
/// # 用法
///
/// ```ignore
/// use ma_harness_cordis::CtxKey;
/// use ma_harness_seam::ctx_key;
///
/// static MY_KEY: CtxKey<String> = ctx_key!("my_key");
/// // 下面这行编译失败 (camelCase 拒绝):
/// // static BAD_KEY: CtxKey<String> = ctx_key!("myKey");
/// ```
///
/// # 原理
///
/// 1. const eval 阶段调 `ma_harness_cordis::is_snake_case(name)`
/// 2. 若非法, 触发 `[()][(!is_valid) as usize]` const 数组越界 panic
/// 3. cargo build 时 "index out of bounds" 跟具体位置
#[macro_export]
macro_rules! ctx_key {
    ($name:expr) => {{
        const __NAME: &str = $name;
        // seam 依赖 cordis, 直接调它的 const fn
        const __IS_VALID: bool = $crate::is_snake_case(__NAME);
        // 编译期校验: 非法时 const 越界 panic
        const _: () = [()][(!__IS_VALID) as usize];
        // 校验通过, 构造 CtxKey (走 new_unchecked 跳过 runtime 检查)
        $crate::CtxKey::new_unchecked(__NAME)
    }};
}

// ============================================================================
// 公开 trait
// ============================================================================

/// 公开 Service trait (跟 cordis 的 Service 解耦)
///
/// 插件作者 impl 这个, **不要**直接 impl `ma_harness_cordis::Service`.
/// seam 内部通过 `CordisService<S>` 转换 impl cordis 的 Service.
///
/// 2026-08-18: 去掉 `type Ctx = ...` 默认 (stable 不支持), impl 必须显式指定
pub trait Service: Send + Sync + 'static {
    /// 关联的 ctx 类型 (impl 必须显式指定 `type Ctx = ma_harness_cordis::Context;`)
    type Ctx;

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
    /// 事件触发时调用
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
// Cordis 内部 trait 跟 Seam 公开 trait 转换
// ============================================================================

/// 把公开 Service 转成内部 (impl ma_harness_cordis::Service for CordisService<S>)
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

impl<S: Service<Ctx = ma_harness_cordis::Context>> ma_harness_cordis::Service for CordisService<S> {
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

/// PluginLoader — 简单 API: 拿一个 plugin name, 装进 ctx
///
/// **Phase 1 stub**: 只暴露 trait 边界, 不实际加载. 真正加载见 Phase 2 (用
/// inventory 动态加载 + plugin.toml entry 解析).
pub struct PluginLoader;

impl PluginLoader {
    /// Phase 1 stub: 返回 "未实现" 错误
    ///
    /// Phase 2 改成读 plugin.toml 的 entry, 动态加载编译时 link 的 plugin.
    pub fn load_by_name(
        _ctx: &ma_harness_cordis::Context,
        _name: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!(
            "PluginLoader::load_by_name 尚未实现. Phase 2 (inventory + plugin.toml 解析)."
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
        type Ctx = Context;
        type Error = Box<dyn std::error::Error + Send + Sync>;
        fn install(_ctx: &Context) -> Result<Self, Self::Error> {
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
            let s = MyService::install(ctx).map_err(|e| anyhow::anyhow!("{e}"))?;
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
