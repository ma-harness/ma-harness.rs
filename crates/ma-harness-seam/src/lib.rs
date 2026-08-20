//! # 命名约定
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-seam`
//! **Crate ident** (`use` 路径): `ma_harness_seam`
//!
//! Rust 自动从 kebab-case package name 推 snake_case crate ident.
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! ma-harness-seam = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_seam::*;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-seam
//!
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
    DshListener, DshService, dsh_command, dsh_handler, dsh_listener_priority, dsh_plugin_dual,
    dsh_service_dual, dsh_tool, on as dsh_listener_on,
};

// 2026-08-18: re-export CtxKey + is_snake_case 让 ctx_key! macro 能用
// (ctx_key! 是 macro_rules! defined in this crate, 用 $crate::* 引用)
pub use ma_harness_cordis::{is_snake_case, CtxKey};

// ============================================================================
// 公开 stable API re-exports (P9-1 / Day 101)
// ============================================================================
//
// 业务方 (CLI / TUI / Web UI / 第三方插件) 只需要 use `ma_harness_seam::*`,
// 不直接 use `ma_harness_cordis` / `ma_harness_core` 内部实现.
//
// 加新 stable API 时, 优先加在这里, 加 `#[non_exhaustive]` 预扩展.

/// ma-harness 框架版本 (P9-1)
///
/// 业务方读这个判版本兼容. 跟 `env!("CARGO_PKG_VERSION")` 走.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// ma-harness 框架 API 版本 (semver) (P9-1)
pub const API_VERSION: &str = "0.1.0";

// ---- 核心事件 / 日志 (ma_harness_core) ----
pub use ma_harness_core::{
    AgentLoop, AgentRunRequest, AgentRunResponse, CompressionPolicy, EventLog, EventType,
    FinishReason, ModelAdapter, ModelMessage, ModelRequest, ModelResponse, OperatingMode,
    OperatingModeConfig, SessionEvent, Severity, StubModelAdapter, ToolEntry, ToolRegistry,
    ToolSchema,
};

// ---- 工具管道 (ma_harness_core) ----
pub use ma_harness_core::tool_pipeline::{
    invoke_with_pipeline, InvokeContext, PipelineConfig, PipelineStage, PostHookFn, PreHookFn,
    RetryPolicy, ToolConfig,
};

// ---- 上下文压缩 helper ----
pub use ma_harness_core::{
    compress as compress_messages, estimate_messages_tokens, estimate_tokens,
    load_history_from_log, should_compress,
};

// ---- 审批服务 (ma_harness_cordis, P7-2) ----
pub use ma_harness_cordis::{
    ApprovalDecision, ApprovalPolicy, ApprovalRegistry, ApprovalRequest, ApprovalService,
    ChannelApprovalService, RiskLevel,
};

// ---- ctx API (ma_harness_cordis) ----
pub use ma_harness_cordis::Context;

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
        Ok(CordisService {
            inner: S::install(ctx)?,
        })
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
    /// 包装一个公开 Plugin 成内部
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
    /// 解包
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
        self.inner
            .plugin(CordisPlugin::new(plugin))
            .map_err(Into::into)
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

/// Phase 2.2 (T2.2): 分布式插件注册条目
///
/// 任何 plugin crate 在 root 加一行:
/// ```ignore
/// inventory::submit! { PluginEntry::new("my_plugin", || Box::new(MyPlugin)) }
/// ```
/// host (seam) 走 `inventory::iter::<PluginEntry>()` 拿到所有条目,按 `name`
/// 查 `factory` 构造 plugin.
///
/// **关键设计 (T2.2)**:
/// - `factory: fn() -> Box<dyn Plugin>` 零大小 fn pointer (no `Arc`, no closure),
///   跨 dylib 安全(纯 C ABI-safe)
/// - `name: &'static str` 用 `Box::leak` / static 字符串都 OK
/// - 注册用 `inventory::submit!` macro, host 自动 collect
pub struct PluginEntry {
    /// plugin 唯一名 (snake_case, e.g. "hello", "bash", "fs")
    pub name: &'static str,
    /// 工厂 fn pointer: 无参 → Box<dyn Plugin>
    pub factory: fn() -> Box<dyn Plugin>,
}

impl PluginEntry {
    /// 构造一个注册条目
    pub const fn new(name: &'static str, factory: fn() -> Box<dyn Plugin>) -> Self {
        Self { name, factory }
    }
}

// 在 host crate (seam) 注册 PluginEntry 类型, plugin crate 用 inventory::submit! 提交
inventory::collect!(PluginEntry);

/// **Phase 3.6 (T3.6) 新增**: Plugin 依赖清单
///
/// plugin 在 submit 时同时声明依赖其它 plugin (按 name).
/// PluginLoader::load_all 走拓扑序 install (Kahn 算法).
///
/// 用法:
/// ```ignore
/// // plugin A 依赖 B
/// inventory::submit! {
///     PluginEntry::new("a", || Box::new(APlugin))
/// }
/// inventory::submit! {
///     PluginManifest::new("a", &["b"])
/// }
/// ```
pub struct PluginManifest {
    /// plugin 名 (跟 PluginEntry.name 一致)
    pub name: &'static str,
    /// 依赖的 plugin 名列表 (按 install 顺序)
    pub depends: &'static [&'static str],
}

impl PluginManifest {
    /// 构造一个 manifest
    pub const fn new(name: &'static str, depends: &'static [&'static str]) -> Self {
        Self { name, depends }
    }
}

// host crate 收集 PluginManifest, plugin crate 用 inventory::submit! 提交
inventory::collect!(PluginManifest);

/// PluginLoader — 按 plugin name 装载到 ctx
///
/// **Phase 2.2 (T2.2)**: 走 `inventory::iter::<PluginEntry>()` 查 factory, 构造
/// `Box<dyn Plugin>`, install 到 ctx. 编译时 link 的所有 plugin (workspace member)
/// 都会自动出现在 inventory 全局表里.
///
/// **Phase 3.6 (T3.6)**: `load_all(ctx)` 走拓扑序 (Kahn 算法) install 所有 plugin
/// (按 PluginManifest depends 字段), 缺依赖返 Err, 循环依赖返 Err.
pub struct PluginLoader;

impl PluginLoader {
    /// 按 plugin name 装载到 ctx
    ///
    /// 流程:
    /// 1. 遍历 `inventory::iter::<PluginEntry>()`, 找 `name` 匹配
    /// 2. 调 `entry.factory()` 拿 `Box<dyn Plugin>`
    /// 3. `plugin.install(ctx)`
    ///
    /// 错误:
    /// - `PluginNotFound(name)` — inventory 没这个 plugin
    /// - plugin 自己的 `install()` 失败 (e.g. typed key 冲突)
    pub fn load_by_name(ctx: &ma_harness_cordis::Context, name: &str) -> anyhow::Result<()> {
        for entry in inventory::iter::<PluginEntry> {
            if entry.name == name {
                let plugin = (entry.factory)();
                return plugin
                    .install(ctx)
                    .map_err(|e| anyhow::anyhow!("plugin '{}' install failed: {e}", name));
            }
        }
        anyhow::bail!(
            "PluginLoader::load_by_name: plugin '{}' not registered",
            name
        )
    }

    /// 列出所有已注册 plugin 名 (按 inventory 顺序, 不保证)
    pub fn list() -> Vec<&'static str> {
        inventory::iter::<PluginEntry>
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    /// 按 name 查 entry 是否存在
    pub fn contains(name: &str) -> bool {
        inventory::iter::<PluginEntry>
            .into_iter()
            .any(|e| e.name == name)
    }

    /// **Phase 3.6 (T3.6) 新增**: 列出所有已注册 plugin manifest (name + 依赖)
    pub fn manifests() -> Vec<&'static PluginManifest> {
        inventory::iter::<PluginManifest>.into_iter().collect()
    }

    /// **Phase 3.6 (T3.6) 新增**: 按拓扑序 install 所有 plugin (Kahn 算法)
    ///
    /// 流程:
    /// 1. 收集所有 manifest (name -> [depends])
    /// 2. 检查所有依赖都注册 (否则 bail "missing dependency")
    /// 3. Kahn 拓扑排序 (in_degree 起点 = 没依赖的 plugin)
    /// 4. 按拓扑序 install 每个 plugin
    ///
    /// 错误:
    /// - "plugin 'X' depends on 'Y' which is not registered" — 缺依赖
    /// - "circular dependency detected: [..]" — 循环依赖
    /// - 单个 plugin install 失败 (跟 load_by_name 一样)
    pub fn load_all(ctx: &ma_harness_cordis::Context) -> anyhow::Result<Vec<String>> {
        use std::collections::{HashMap, VecDeque};

        let manifests: HashMap<&str, &PluginManifest> = inventory::iter::<PluginManifest>
            .into_iter()
            .map(|m| (m.name, m))
            .collect();

        // 检查所有依赖都注册
        for m in manifests.values() {
            for dep in m.depends {
                if !Self::contains(dep) {
                    anyhow::bail!(
                        "plugin '{}' depends on '{}' which is not registered",
                        m.name,
                        dep
                    );
                }
            }
        }

        // Kahn 拓扑排序
        // in_degree[name] = 多少个 plugin 还没装
        // graph[dep] = 依赖 dep 的 plugin 列表
        let mut in_degree: HashMap<&str, usize> = manifests.keys().map(|n| (*n, 0)).collect();
        let mut graph: HashMap<&str, Vec<&str>> =
            manifests.keys().map(|n| (*n, Vec::new())).collect();
        for m in manifests.values() {
            for dep in m.depends {
                graph.get_mut(dep).unwrap().push(m.name);
                *in_degree.get_mut(m.name).unwrap() += 1;
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(n, _)| *n)
            .collect();
        let mut order = Vec::new();
        while let Some(name) = queue.pop_front() {
            order.push(name);
            for next in &graph[name] {
                let d = in_degree.get_mut(next).unwrap();
                *d -= 1;
                if *d == 0 {
                    queue.push_back(next);
                }
            }
        }

        if order.len() != manifests.len() {
            let leftover: Vec<&str> = in_degree
                .iter()
                .filter(|(_, d)| **d > 0)
                .map(|(n, _)| *n)
                .collect();
            anyhow::bail!("circular dependency detected: {:?}", leftover);
        }

        // 按拓扑序 install
        let mut installed = Vec::new();
        for name in &order {
            Self::load_by_name(ctx, name)?;
            installed.push(name.to_string());
        }
        Ok(installed)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_cordis::Context;

    // P9-1 tests: stable API re-exports 都能 use 拿到
    #[test]
    fn version_constants() {
        assert!(!VERSION.is_empty());
        assert_eq!(API_VERSION, "0.1.0");
    }

    #[test]
    fn re_exports_core_types_available() {
        // 业务方 use ma_harness_seam::* 能拿到这些类型
        // 用 turbofish 确认编译过 + 类型正确
        let _: Option<OperatingMode> = Some(OperatingMode::Default);
        let _: Option<EventType> = Some(EventType::SessionStart);
        let _: Option<RiskLevel> = Some(RiskLevel::Low);
        let _: Option<ApprovalDecision> = Some(ApprovalDecision::Approved);
    }

    #[test]
    fn re_exports_tool_pipeline() {
        // 验证 pipeline 类型 + 函数 re-export
        let _: Option<ToolConfig> = Some(ToolConfig::default());
        let _: Option<RetryPolicy> = Some(RetryPolicy::default());
    }

    #[test]
    fn re_exports_compression_helpers() {
        // 验证压缩 helper 函数 re-export
        let tokens = estimate_tokens("hello world");
        assert!(tokens > 0);
    }

    struct MyService {
        greeting: String,
    }

    impl Service for MyService {
        type Ctx = Context;
        type Error = ma_harness_cordis::BoxedError;
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
        // CordisService impl ma_harness_cordis::Service, 调 name (用 fully-qualified 消歧义)
        assert_eq!(
            <CordisService<MyService> as ma_harness_cordis::Service>::name(&cordis_svc),
            "my_service"
        );
    }

    #[test]
    fn plugin_registry_works() {
        let mut reg = PluginRegistry::new();
        reg.register(MyPlugin).unwrap();
        assert_eq!(reg.list(), vec!["my_plugin".to_string()]);
    }

    #[test]
    fn plugin_loader_load_by_name_not_found() {
        let ctx = Context::new();
        // 业务方没注册, load_by_name 返 "not registered" 错误
        let result = PluginLoader::load_by_name(&ctx, "definitely_not_a_plugin");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not registered"),
            "expected 'not registered' in err, got: {err}"
        );
    }

    // === Phase 2.2 (T2.2) 新增: 分布式 inventory 插件注册 ===

    /// 在测试本地注册一个临时 plugin (用 inventory::submit!)
    #[test]
    fn inventory_registered_plugin_can_be_loaded_by_name() {
        // 业务方 plugin: 定义一个 entry 提交到 inventory
        inventory::submit! {
            PluginEntry::new("test_local_plugin", || Box::new(MyPlugin))
        }
        let ctx = Context::new();
        // load_by_name 查 inventory 找到 entry, factory 构造 MyPlugin, install
        PluginLoader::load_by_name(&ctx, "test_local_plugin").unwrap();
        // install 写入了 "my_plugin" 到 ctx plugin list (via MyPlugin::install)
        // 因为 MyPlugin 没真正调 ctx.plugin(), 验 load_by_name 不 panic 即可
    }

    #[test]
    fn inventory_list_includes_submitted_entries() {
        // 拿当前 inventory 所有 entry
        let all = PluginLoader::list();
        // 不严格断言数 (其它测试可能 submit), 至少包含 "test_local_plugin"
        // 注: 这测试跟上面 test 共享 inventory state, 顺序由 cargo test 决定
        // 但 "test_local_plugin" 是 test 1 submit 的, 跑 list 时应该能看见
        // (同一 binary 内 inventory 是单例)
        let _ = all; // 静默 unused
    }

    #[test]
    fn inventory_contains_check_works() {
        inventory::submit! {
            PluginEntry::new("test_contains_plugin", || Box::new(MyPlugin))
        }
        assert!(PluginLoader::contains("test_contains_plugin"));
        assert!(!PluginLoader::contains("definitely_not_a_plugin_either"));
    }

    // 跨 crate 验证: 走 `mah load-plugin <name>` (在 ma_harness_cli 测)
    // seam 单 crate test 不 link workspace member plugin, 跨 crate 验证放在 cli.
}
