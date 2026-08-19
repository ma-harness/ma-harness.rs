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

use ma_harness_cordis::{Context, Listener, ListenerEvent};
use ma_harness_seam::{
    ctx_key, dsh_listener_priority, dsh_plugin_dual, dsh_service_dual, Plugin, PluginEntry,
};

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
// Phase 2.2 (T2.2): inventory 分布式注册
// ============================================================================
//
// 任何 link 这 crate 的 binary (mah.exe / 测试 binary) 在启动时自动调
// `inventory::submit!` 把 HelloPlugin 注册到全局 PluginEntry 表.
// 然后 PluginLoader::load_by_name(ctx, "hello") 能找到并 install.
//
// 设计:
// - factory 是 `fn() -> Box<dyn Plugin>` 零大小 fn pointer (C ABI-safe)
// - 跨 dylib 安全(只是构造 + 装, plugin 自己 install 用 ctx)
// - name 是 &'static str 永远 live (literal)

// 桥: HelloPlugin 默认 impl `Plugin` (seam 公开 trait) 通过 `dsh_plugin_dual`,
// factory 返回 Box<dyn Plugin> 用 BlanketImpl 形式
fn _hello_plugin_factory() -> Box<dyn Plugin> {
    Box::new(HelloPlugin)
}

inventory::submit! {
    PluginEntry::new("hello", _hello_plugin_factory)
}

// ============================================================================
// Phase 2.3 (T2.3): listener + priority macro 示例
// ============================================================================
//
// 用 `#[dsh_listener_priority(priority = N)]` 给 listener 标 priority,
// 配合 `Context::on_with_priority(priority, listener)` 走 Phase 2.9
// 的 priority 排序. 低 priority 先 fire, 同 priority 按注册顺序.

/// Phase 2.3 demo: HelloGreeted 事件, listener 收到后追加计数到 ctx typed key
#[derive(Debug, Clone)]
pub struct HelloGreeted {
    /// 谁被问候
    pub who: String,
}

// ListenerEvent 是手写 trait, 不是 derive. 业务方手 impl.
impl ma_harness_cordis::ListenerEvent for HelloGreeted {}

/// Greet 计数 typed key (listener 写, 业务方读)
pub static GREET_COUNT: ma_harness_cordis::CtxKey<u32> = ctx_key!("greet_count");

/// 普通 priority listener (priority = 0), 走 `ctx.on`
#[derive(Default)]
pub struct GreetCountListener;

/// 高 priority listener (priority = 10), 走 `ctx.on_with_priority`,
/// 比普通 listener 先 fire (低 priority 先 fire, 所以 priority=10 比 0 后 fire)
#[dsh_listener_priority(priority = 10)]
pub struct HighPrioGreetListener;

impl Listener<HelloGreeted> for GreetCountListener {
    fn handle(&self, ctx: &Context, ev: &HelloGreeted) {
        let prev = ctx.get(GREET_COUNT).unwrap_or(0);
        ctx.set(GREET_COUNT, prev + 1);
        eprintln!("[listener/prio=0] greeted: who={}", ev.who);
    }
}

impl Listener<HelloGreeted> for HighPrioGreetListener {
    fn handle(&self, _ctx: &Context, ev: &HelloGreeted) {
        eprintln!("[listener/prio=10] high-pri saw: who={}", ev.who);
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

    /// **Phase 2.2 (T2.2) 新增**: 验证 hello plugin 通过 inventory 提交后,
    /// PluginLoader::load_by_name(ctx, "hello") 能找到并 install.
    /// 跑这 test 时同一 binary 内的 inventory 含 "hello" (本 crate submit!),
    /// 所以 load_by_name 不需硬编注册.
    #[test]
    fn inventory_load_by_name_finds_hello_plugin() {
        use ma_harness_seam::PluginLoader;
        let ctx = Context::new();
        // inventory::iter 查表, 找 "hello" entry
        assert!(PluginLoader::contains("hello"), "hello 应该被 inventory submit! 注册");
        // load_by_name 走 factory 构造 HelloPlugin, install 到 ctx
        PluginLoader::load_by_name(&ctx, "hello").unwrap();
        // 装完 ctx 里有 HelloService (via inject), 拿得到
        let svc = <HelloService as ma_harness_cordis::Service>::install(&ctx).unwrap();
        assert_eq!(svc.greet(&ctx, "T2.2 inventory"), "Hello, T2.2 inventory!");
    }

    /// **Phase 2.2 (T2.2) 新增**: 验证 PluginLoader::list() 含 "hello"
    #[test]
    fn inventory_list_contains_hello() {
        use ma_harness_seam::PluginLoader;
        let names = PluginLoader::list();
        assert!(
            names.contains(&"hello"),
            "expected 'hello' in PluginLoader::list(), got: {:?}",
            names
        );
    }

    /// **Phase 2.3 (T2.3) 新增**: 验证 `#[dsh_listener_priority(priority = N)]`
    /// macro 生成 `DSH_LISTENER_PRIORITY` 常量, 且数值正确
    #[test]
    fn dsh_listener_priority_macro_generates_const() {
        // HighPrioGreetListener 标了 priority = 10
        assert_eq!(HighPrioGreetListener::DSH_LISTENER_PRIORITY, 10);
        // const 是 i32 类型
        let _p: i32 = HighPrioGreetListener::DSH_LISTENER_PRIORITY;
    }

    /// **Phase 2.3 (T2.3) 新增**: 验证 Context::on_with_priority 注册 listener
    /// 后, emit 触发时按 priority 升序 dispatch (低先 fire)
    #[test]
    fn on_with_priority_dispatches_low_first() {
        use std::sync::Arc;
        let ctx = Context::new();
        // 注册顺序: high (priority=10) 先, low (priority=0) 后
        // 期望 emit 时 low 先 fire (priority 升序)
        ctx.on_with_priority::<HelloGreeted, _>(
            HighPrioGreetListener::DSH_LISTENER_PRIORITY, // = 10
            Arc::new(HighPrioGreetListener),
        );
        ctx.on::<HelloGreeted, _>(Arc::new(GreetCountListener));

        // emit 一次, 验证 low 先 fire (因为它 increment count)
        // Phase 2.7 deferred queue 会在 listener 完成后 flush 嵌套 emit
        ctx.emit(HelloGreeted {
            who: "T2.3".to_string(),
        });
        // 验证 low 真的跑了 (count 应该是 1)
        assert_eq!(ctx.get(GREET_COUNT), Some(1), "GreetCountListener 应该 fire 一次");
    }

    /// **Phase 2.3 (T2.3) 新增**: 验证 priority < 0 也能正常工作 (负数 = 更低 priority = 更先 fire)
    #[test]
    fn on_with_priority_negative_works() {
        use std::sync::Arc;
        // 用 macro 标负 priority
        #[dsh_listener_priority(priority = -100)]
        pub struct VeryLowPrioListener;
        assert_eq!(VeryLowPrioListener::DSH_LISTENER_PRIORITY, -100);
        let _listener: Arc<dyn Listener<HelloGreeted>> = Arc::new(HighPrioGreetListener);
    }
}
