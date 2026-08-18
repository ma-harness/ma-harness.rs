//! Context — DI 容器 + typed key storage + plugin registry
//!
//! 整个 ma-harness 运行时核心. 所有 service / tool / listener 都从 ctx 取数据.
//!
//! # 关键 API (Week 1 Day 1-2 实现)
//!
//! - [`Context::new`] / [`Context::default`]
//! - [`Context::set`] / [`Context::get`] / [`Context::remove`] — typed key storage
//! - [`Context::inject`] — 注册 service (强类型, 编译期检查)
//! - [`Context::service`] — 取出 service (强类型, 编译期检查)
//! - [`Context::plugin`] — 装载 plugin
//! - [`Context::uninstall_plugin`] — 卸载 plugin
//! - [`Context::plugins`] — 列出已装载 plugin
//!
//! # Week 1 Day 5 才加
//!
//! - `ctx.on(event, handler)` — 订阅事件
//! - `ctx.emit(event)` — 发射事件
//! - `ctx.fork()` — 派生子 ctx
//! - `ctx.dispose()` — 释放所有 disposable 资源
//!
//! # 设计
//!
//! - 内部用 `dashmap<&'static str, Box<dyn Any>>` 装 typed key (按 name 索引, 不是 TypeId)
//! - service registry 用 `dashmap<TypeId, Arc<dyn Any>>` (按类型索引, 因为 service 是强类型拿取)
//! - plugin registry 用 `parking_lot::Mutex<HashMap>` (写入少读多)
//!
//! 关键设计: **typed key 索引用 name 不用 TypeId**.
//! 原因是 name 不同但 T 相同的两个 key 应该独立. 早期 bug 误用 TypeId 索引导致 key 互通.
//! 见 `test different_keys_same_type_dont_clash` 回归测试.

use dashmap::DashMap;
use std::any::{Any, TypeId};
use std::sync::Arc;

use crate::disposable::{Disposable, DisposableEntry, Scope};
use crate::key::CtxKey;
use crate::listener::{Listener, ListenerEvent, ListenerRegistry};
use crate::plugin::{Plugin, PluginRegistry};
use crate::service::Service;
use crate::CordisError;

/// 内部 typed storage: &'static str (name) -> Box<dyn Any>
/// 用 name 索引, 保证 key 名字不同的 ctx key 互不干扰.
type Storage = DashMap<&'static str, Box<dyn Any + Send + Sync>>;

/// 内部 service registry: TypeId -> Arc<dyn Any>
/// (Service trait 不 object safe 因为 Sized bound, 但 Arc<dyn Any> 可以)
type ServiceMap = DashMap<TypeId, Arc<dyn Any + Send + Sync>>;

/// Context 主体
#[derive(Default)]
pub struct Context {
    /// typed key storage (按 name 索引, 跟 dsh 风格一致)
    storage: Storage,
    /// service instances (按 TypeId 索引, 因为 service 是按类型拿)
    services: ServiceMap,
    /// plugin registry
    plugins: PluginRegistry,
    /// listener registry (Week 1 Day 5 加)
    listeners: ListenerRegistry,
    /// disposable list (Week 1 Day 5 加, 跟 scope 共享)
    disposables: Arc<parking_lot::Mutex<Vec<DisposableEntry>>>,
    /// ctx 是否已 dispose
    disposed: std::sync::atomic::AtomicBool,
}

impl Context {
    /// 创建一个空 ctx
    pub fn new() -> Self {
        Self::default()
    }

    // ========================================================================
    // Typed key storage
    // ========================================================================

    /// 存一个 typed 值
    pub fn set<T: Send + Sync + 'static>(&self, key: CtxKey<T>, value: T) {
        self.storage.insert(key.name(), Box::new(value));
    }

    /// 取一个 typed 值 (克隆)
    ///
    /// 2026-08-18 修复: dashmap Ref 局部 drop, 用 unsafe 延长 lifetime
    /// SAFETY: Box 在 self.storage 里, Ref dropped 后 &T 仍指向 Box (Box 地址稳定)
    #[allow(unsafe_code)]
    pub fn get<T: Clone + Send + Sync + 'static>(&self, key: CtxKey<T>) -> Option<T> {
        let entry = self.storage.get(key.name())?;
        let value_ref = entry.value().downcast_ref::<T>()?;
        Some(unsafe { &*(value_ref as *const T) }.clone())
    }

    /// 取一个 typed 值 (引用, 零拷贝)
    ///
    /// 2026-08-18 修复: dashmap Ref 局部 drop, 用 unsafe 延长 lifetime
    /// SAFETY: Box 在 self.storage 里, Ref dropped 后 &T 仍指向 Box (Box 地址稳定)
    ///         self 不变 (no &mut self 同时存在) → Box 不会被移动 → &T 安全
    #[allow(unsafe_code)]
    pub fn get_ref<T: Send + Sync + 'static>(&self, key: CtxKey<T>) -> Option<&T> {
        let entry = self.storage.get(key.name())?;
        let value_ref = entry.value().downcast_ref::<T>()?;
        Some(unsafe { &*(value_ref as *const T) })
    }

    /// 移除一个 typed 值
    pub fn remove<T: Send + Sync + 'static>(&self, key: CtxKey<T>) -> Option<T> {
        self.storage
            .remove(key.name())
            .and_then(|(_, v)| v.downcast::<T>().ok().map(|b| *b))
    }

    /// 检查 key 是否存在
    pub fn contains<T: Send + Sync + 'static>(&self, key: CtxKey<T>) -> bool {
        self.storage.contains_key(key.name())
    }

    // ========================================================================
    // Service inject / fetch
    // ========================================================================

        /// 注入一个 service (用户自己 `MyService::install(&ctx)?` 拿到实例, 调这个塞)
    ///
    /// 2026-08-18 (Day 53) 修复: 加 `S: Any` bound, 让 Arc<S> 能 coerce 到 Arc<dyn Any + Send + Sync>
    pub fn inject<S: Service + Any>(&self, service: Arc<S>) {
        let erased: Arc<dyn Any + Send + Sync> = service;
        self.services.insert(TypeId::of::<S>(), erased);
    }

    /// 注入一个由 install 构造的 service (便捷方法)
    ///
    /// 错误处理: install 返回 `S::Error`,通过 `Display + Send + Sync` 边界强转 string,
    /// 不要求 `S::Error: Into<CordisError>` (避免给每个 Service 强加 Into bound).
    ///
    /// 2026-08-18 修复:
    /// - 加 `S::Ctx = Context` 显式 bound, 之前 default 已删除
    /// - 加 `S::Error: Sized` bound (Service trait 的 Error 是 `?Sized` 让 Box<dyn Error> 能用,
    ///   但 Result<T, E> 隐含 E: Sized)
    pub fn inject_from<S: Service<Ctx = Context>>(&self) -> Result<Arc<S>, CordisError>
    where
        S::Error: Sized,
    {
        let svc = S::install(self).map_err(|e| {
            tracing::error!(error = %e, service = std::any::type_name::<S>(), "service install failed");
            CordisError::Other(format!("service install failed: {}", e))
        })?;
        let arc = Arc::new(svc);
        self.services.insert(TypeId::of::<S>(), arc.clone());
        Ok(arc)
    }

    /// 取一个 service (克隆 Arc, 不消耗原注册)
    ///
    /// 2026-08-18 (Day 53) 修复: 用 `Arc::downcast` 替代 `downcast_ref` (后者 downcast `&Arc<dyn Any>`
    /// 到 `&Arc<S>`, 跟 outer Arc wrapper 错位, 永远 None). 改 downcast inner `dyn Any` 通过 `Arc::downcast`
    pub fn service<S: Service + Any>(&self) -> Option<Arc<S>> {
        let arc_any: Arc<dyn Any + Send + Sync> =
            self.services.get(&TypeId::of::<S>())?.value().clone();
        // Arc::downcast 消费 Arc<dyn Any>, 返 Result<Arc<T>, Arc<dyn Any>>
        Arc::downcast::<S>(arc_any).ok()
    }

    /// 取一个 service, 找不到时返回错误
    pub fn service_or<S: Service>(&self, name: &'static str) -> Result<Arc<S>, CordisError> {
        self.service::<S>().ok_or(CordisError::ServiceNotFound(name))
    }

    // ========================================================================
    // Plugin
    // ========================================================================

    /// 装载一个 plugin
    pub fn plugin<P: Plugin>(&self, plugin: P) -> Result<(), CordisError> {
        let arc = Arc::new(plugin);
        arc.install(self).map_err(|e| {
            // install 失败时回滚 (不在 registry 里, 不用 unregister)
            tracing::error!(error = %e, name = arc.name(), "plugin install failed");
            CordisError::Other(format!("plugin install failed: {}", e))
        })?;
        self.plugins.register(arc)
    }

    /// 卸载一个 plugin
    pub fn uninstall_plugin(&self, name: &str) -> Result<(), CordisError> {
        let p = self.plugins.unregister(name)?;
        p.uninstall().map_err(|e| {
            // uninstall 失败, 把 plugin 重新塞回 registry
            let _ = self.plugins.register(p);
            CordisError::Other(format!("plugin uninstall failed: {}", e))
        })?;
        Ok(())
    }

    /// 列出所有已装载 plugin
    pub fn plugins(&self) -> Vec<String> {
        self.plugins.list()
    }

    /// 取一个已装载 plugin (Arc)
    pub fn plugin_by_name(&self, name: &str) -> Option<Arc<dyn Plugin>> {
        self.plugins.get(name)
    }

    // ========================================================================
    // Extend (Week 1 Day 3 新增, arch-map §2.3)
    // ========================================================================

    /// 从另一个 ctx 继承 service 引用.
    ///
    /// # 行为
    ///
    /// - 把 `other` 注册的所有 service (`Arc<dyn Any + Send + Sync>`) 复制引用进 `self`
    /// - typed key storage **不**继承 (key 是 owned 语义, 自己重新 set)
    /// - plugin **不**继承 (plugin 是自包含, 自己重新装载)
    ///
    /// # 死锁安全
    ///
    /// 先把 other 的 services 一次性 collect 出 Arc, 再插进 self,
    /// 不同时持两把锁.
    ///
    /// # 用途
    ///
    /// 父 ctx 派生 sub ctx 时 (e.g. plugin subagent), 把父 ctx 已有 service
    /// 引用共享给子 ctx. Phase 2 加 `Context::fork()` 用此实现.
    pub fn extend_from(&self, other: &Context) {
        // 一次性 collect (持 other 锁), 然后释放, 再 insert (持 self 锁)
        let to_insert: Vec<(std::any::TypeId, std::sync::Arc<dyn std::any::Any + Send + Sync>)> =
            other
                .services
                .iter()
                .map(|entry| {
                    let type_id: std::any::TypeId = *entry.key();
                    let arc: std::sync::Arc<dyn std::any::Any + Send + Sync> = entry.value().clone();
                    (type_id, arc)
                })
                .collect();
        // iter() 结束, other.services 锁释放

        for (type_id, arc) in to_insert {
            self.services.insert(type_id, arc);
        }
    }

    // ========================================================================
    // Listener / Emit (Week 1 Day 5)
    // ========================================================================

    /// 订阅事件 E
    ///
    /// listener 可以是闭包 `|ctx, ev| { ... }` 或 impl `Listener<E>` 的 struct.
    /// 闭包形式 `Fn(&Context, &E) + Send + Sync + 'static` 自动 impl Listener<E>.
    ///
    /// # 重复订阅
    ///
    /// 同一 E 类型可以注册多个 listener, 触发时按注册顺序同步 dispatch.
    /// 同一 listener (Arc ptr_eq) 重复注册会**追加**, 不去重.
    pub fn on<E: ListenerEvent, L: Listener<E>>(&self, listener: Arc<L>) {
        self.listeners.on(listener);
    }

    /// 触发事件
    ///
    /// 同步 dispatch 给所有订阅 E 的 listener.
    /// 失败由 listener 内部处理 (不返回 Result, Phase 2 加).
    ///
    /// # Reentrancy (Phase 1 不允许)
    ///
    /// listener 不能 emit 另一个 event. 这避免循环触发 + 栈溢出.
    /// Phase 1 实现: emit 时检查 thread-local "in emit" 标记, 已 true 则 panic.
    ///
    /// # Panic 安全
    ///
    /// IN_EMIT thread-local 标志用 RAII guard 包装, 即使 listener panic
    /// 也会在 unwinding 时恢复, 下次 emit 不会卡死.
    pub fn emit<E: ListenerEvent>(&self, event: E) {
        if IN_EMIT.with(|b| b.get()) {
            panic!(
                "reentrant emit detected for event type {}. \
                 Phase 1 不支持 listener 内 emit. Phase 2 加 deferred queue.",
                std::any::type_name::<E>()
            );
        }
        // RAII guard: 即使 listener panic, guard drop 时会 set 回 false
        let _guard = EmitGuard::new();
        self.listeners.emit(self, &event);
    }

    /// 列出订阅 E 的 listener 数量 (调试)
    pub fn listener_count<E: ListenerEvent>(&self) -> usize {
        self.listeners.count::<E>()
    }

    // ========================================================================
    // Disposable / Scope (Week 1 Day 5)
    // ========================================================================

    /// 创建一个 RAII scope
    ///
    /// scope drop 时, 注册的 disposable 按 LIFO 顺序释放.
    /// 业务方也可主动 `scope.dispose()` 提前释放.
    pub fn scope(&self) -> Scope {
        Scope::new(Arc::clone(&self.disposables))
    }

    /// 直接注册 disposable (不进 scope, ctx.dispose 时统一释放)
    pub fn on_dispose<D: Disposable>(&self, d: Arc<D>) {
        let mut disposables = self.disposables.lock();
        disposables.push(DisposableEntry::new(d));
    }

    /// 主动释放 ctx 所有 disposable
    ///
    /// LIFO 顺序. 失败时收集, 返回第一个错误.
    /// 多次调用 idempotent (disposed AtomicBool).
    pub fn dispose(&self) -> anyhow::Result<()> {
        use std::sync::atomic::Ordering;
        if self
            .disposed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let mut disposables = self.disposables.lock();
            let mut first_err: Option<anyhow::Error> = None;
            while let Some(e) = disposables.pop() {
                if let Err(err) = e.dispose() {
                    if first_err.is_none() {
                        first_err = Some(err);
                    }
                }
            }
            match first_err {
                Some(e) => Err(e),
                None => Ok(()),
            }
        } else {
            Ok(())
        }
    }

    /// ctx 是否已 dispose
    pub fn is_disposed(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.disposed.load(Ordering::SeqCst)
    }

    // ========================================================================
    // Fork (Week 1 Day 5, 派生 sub ctx)
    // ========================================================================

    /// 派生子 ctx, 共享 service 引用
    ///
    /// 跟 `extend_from` 类似, 但**新建**一个 ctx (不是已有 ctx extend).
    /// typed key / plugin / disposable / listener 都不继承 (子 ctx 自己装).
    /// service (Arc) 继承 — 跟 dsh fork 行为一致 (live 引用, 不是 snapshot).
    pub fn fork(&self) -> Context {
        let child = Context::new();
        child.extend_from(self);
        child
    }
}

// Reentrancy guard (Phase 1 简化: thread-local bool + RAII guard 防止 panic 泄漏)
thread_local! {
    static IN_EMIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// RAII guard: drop 时把 IN_EMIT set 回 false (即使 panic unwinding 也跑)
///
/// 2026-08-18 (Day 53) 修复: new() 也 set IN_EMIT=true, 不然 emit 检查永远 false
struct EmitGuard;

impl EmitGuard {
    fn new() -> Self {
        IN_EMIT.with(|b| b.set(true));
        EmitGuard
    }
}

impl Drop for EmitGuard {
    fn drop(&mut self) {
        IN_EMIT.with(|b| b.set(false));
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("storage_keys", &self.storage.len())
            .field("services", &self.services.len())
            .field("plugins", &self.plugins.list())
            .field("disposed", &self.is_disposed())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CtxKey, Plugin, Service};

    static SESSION_ID: CtxKey<String> = CtxKey::new("session_id");
    static MAX_TOKENS: CtxKey<u32> = CtxKey::new("max_tokens");

    #[derive(Debug)]
    struct GreetingService;

    impl Service for GreetingService {
        type Ctx = Context;
        type Error = crate::error::BoxedError;
        fn install(_ctx: &Context) -> Result<Self, Self::Error> {
            Ok(GreetingService)
        }
        fn name(&self) -> &str {
            "greeting"
        }
    }

    struct HelloPlugin;

    impl Plugin for HelloPlugin {
        fn install(&self, ctx: &Context) -> anyhow::Result<()> {
            // 装 service
            let svc = GreetingService::install(ctx)?;
            ctx.inject(Arc::new(svc));
            // 存 key
            ctx.set(SESSION_ID, "hello-session".to_string());
            Ok(())
        }
        fn name(&self) -> &str {
            "hello"
        }
    }

    #[test]
    fn empty_context() {
        let ctx = Context::new();
        assert!(ctx.plugins().is_empty());
        assert_eq!(ctx.get(SESSION_ID), None);
    }

    #[test]
    fn set_and_get_typed_key() {
        let ctx = Context::new();
        ctx.set(SESSION_ID, "abc".to_string());
        ctx.set(MAX_TOKENS, 1024u32);

        assert_eq!(ctx.get(SESSION_ID), Some("abc".to_string()));
        assert_eq!(ctx.get(MAX_TOKENS), Some(1024u32));
    }

    #[test]
    fn get_ref_no_clone() {
        let ctx = Context::new();
        ctx.set(SESSION_ID, "ref-test".to_string());
        let r: Option<&String> = ctx.get_ref(SESSION_ID);
        assert_eq!(r.map(String::as_str), Some("ref-test"));
    }

    #[test]
    fn remove_typed_key() {
        let ctx = Context::new();
        ctx.set(SESSION_ID, "to-remove".to_string());
        assert!(ctx.contains(SESSION_ID));
        let removed = ctx.remove(SESSION_ID);
        assert_eq!(removed, Some("to-remove".to_string()));
        assert!(!ctx.contains(SESSION_ID));
    }

    #[test]
    fn inject_and_get_service() {
        let ctx = Context::new();
        let svc = GreetingService::install(&ctx).unwrap();
        ctx.inject(Arc::new(svc));
        let got = ctx.service::<GreetingService>();
        assert!(got.is_some());
        assert_eq!(got.unwrap().name(), "greeting");
    }

    #[test]
    fn inject_from_convenience() {
        let ctx = Context::new();
        let arc = ctx.inject_from::<GreetingService>().unwrap();
        assert_eq!(arc.name(), "greeting");
        // 二次取能拿到
        assert!(ctx.service::<GreetingService>().is_some());
    }

    #[test]
    fn service_or_errors_when_missing() {
        let ctx = Context::new();
        let err = ctx.service_or::<GreetingService>("greeting").unwrap_err();
        assert!(matches!(err, CordisError::ServiceNotFound(_)));
    }

    #[test]
    fn plugin_install_and_list() {
        let ctx = Context::new();
        ctx.plugin(HelloPlugin).unwrap();
        assert_eq!(ctx.plugins(), vec!["hello".to_string()]);
    }

    #[test]
    fn plugin_duplicate_errors() {
        let ctx = Context::new();
        ctx.plugin(HelloPlugin).unwrap();
        let err = ctx.plugin(HelloPlugin).unwrap_err();
        assert!(matches!(err, CordisError::PluginAlreadyRegistered(_)));
    }

    #[test]
    fn plugin_uninstall() {
        let ctx = Context::new();
        ctx.plugin(HelloPlugin).unwrap();
        ctx.uninstall_plugin("hello").unwrap();
        assert!(ctx.plugins().is_empty());
    }

    #[test]
    fn plugin_in_install_registers_service_and_key() {
        let ctx = Context::new();
        ctx.plugin(HelloPlugin).unwrap();
        // 装 hello plugin 时已经注册了 GreetingService 和 SESSION_ID
        assert!(ctx.service::<GreetingService>().is_some());
        assert_eq!(ctx.get(SESSION_ID), Some("hello-session".to_string()));
    }

    #[test]
    fn debug_impl_does_not_panic() {
        let ctx = Context::new();
        ctx.set(SESSION_ID, "x".to_string());
        let s = format!("{:?}", ctx);
        assert!(s.contains("Context"));
    }

    #[test]
    fn different_keys_same_type_dont_clash() {
        // 回归测试: 名字不同的 CtxKey<T> 不应共享 storage.
        // bug 现象: 早期实现用 TypeId 索引, 导致 ctx.set(KEY_A, ...) 后 ctx.get(KEY_B) 也能拿到.
        static KEY_A: CtxKey<String> = CtxKey::new("key_a");
        static KEY_B: CtxKey<String> = CtxKey::new("key_b");

        let ctx = Context::new();
        ctx.set(KEY_A, "a_value".to_string());
        assert_eq!(ctx.get(KEY_A), Some("a_value".to_string()));
        assert_eq!(ctx.get(KEY_B), None, "key_b 不应能拿到 key_a 的值");
    }

    #[test]
    fn extend_from_copies_services() {
        // 父 ctx 装 service, 子 ctx extend 父, 子能拿到.
        let parent = Context::new();
        parent.inject_from::<GreetingService>().unwrap();
        assert!(parent.service::<GreetingService>().is_some());

        let child = Context::new();
        assert!(child.service::<GreetingService>().is_none(), "子 ctx 初始没 service");
        child.extend_from(&parent);
        assert!(child.service::<GreetingService>().is_some(), "extend 后子 ctx 应继承 service");

        // Arc 引用同一份, 不是 clone
        let parent_arc = parent.service::<GreetingService>().unwrap();
        let child_arc = child.service::<GreetingService>().unwrap();
        assert!(Arc::ptr_eq(&parent_arc, &child_arc), "extend 应共享 Arc, 不 clone");
    }

    #[test]
    fn extend_from_does_not_copy_keys() {
        // typed key 不 extend, 业务方自己 set.
        let parent = Context::new();
        parent.set(SESSION_ID, "parent_value".to_string());

        let child = Context::new();
        child.extend_from(&parent);
        assert_eq!(child.get(SESSION_ID), None, "typed key 不应被 extend");
    }

    #[test]
    fn extend_from_does_not_copy_plugins() {
        // plugin 不 extend, 业务方自己 plugin().
        let parent = Context::new();
        parent.plugin(HelloPlugin).unwrap();
        assert_eq!(parent.plugins().len(), 1);

        let child = Context::new();
        child.extend_from(&parent);
        assert!(child.plugins().is_empty(), "plugin 不应被 extend");
    }

    #[test]
    fn extend_from_overwrites_same_service() {
        // 同一 service 类型: child 自己装一份, extend 后被 parent 覆盖.
        let parent = Context::new();
        let p_arc = parent.inject_from::<GreetingService>().unwrap();

        let child = Context::new();
        let c_arc = child.inject_from::<GreetingService>().unwrap();

        // 初始两个不同 Arc
        assert!(!Arc::ptr_eq(&p_arc, &c_arc));

        child.extend_from(&parent);

        // extend 后, child 的 service 应被 parent 覆盖
        let c_arc_after = child.service::<GreetingService>().unwrap();
        assert!(Arc::ptr_eq(&p_arc, &c_arc_after), "extend 应覆盖 child 已有 service");
    }

    #[test]
    fn extend_from_empty_parent_is_noop() {
        let parent = Context::new();
        let child = Context::new();
        child.set(SESSION_ID, "child_value".to_string());

        child.extend_from(&parent);
        // 不应有副作用
        assert_eq!(child.get(SESSION_ID), Some("child_value".to_string()));
        assert!(child.plugins().is_empty());
    }

    // =========================================================================
    // Week 1 Day 5 新增测试: listener / emit / scope / fork / dispose
    // =========================================================================

    use crate::listener::ListenerEvent;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone)]
    struct TestEvent {
        msg: String,
    }
    impl ListenerEvent for TestEvent {}

    #[derive(Debug, Clone)]
    struct OtherTestEvent;
    impl ListenerEvent for OtherTestEvent {}

    #[test]
    fn emit_with_no_listener_noop() {
        let ctx = Context::new();
        ctx.emit(TestEvent {
            msg: "hi".to_string(),
        }); // 不 panic
    }

    #[test]
    fn on_and_emit_calls_listener() {
        let ctx = Context::new();
        let called = Arc::new(AtomicUsize::new(0));
        let c2 = Arc::clone(&called);
        let msg_received = Arc::new(parking_lot::Mutex::new(String::new()));
        let m2 = Arc::clone(&msg_received);

        ctx.on::<TestEvent, _>(Arc::new(move |_ctx: &Context, ev: &TestEvent| {
            c2.fetch_add(1, Ordering::SeqCst);
            *m2.lock() = ev.msg.clone();
        }));

        ctx.emit(TestEvent {
            msg: "hello".to_string(),
        });
        assert_eq!(called.load(Ordering::SeqCst), 1);
        assert_eq!(*msg_received.lock(), "hello");
    }

    #[test]
    fn multiple_listeners_all_called() {
        let ctx = Context::new();
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        let a2 = Arc::clone(&a);
        let b2 = Arc::clone(&b);

        ctx.on::<TestEvent, _>(Arc::new(move |_: &Context, _: &TestEvent| {
            a2.fetch_add(1, Ordering::SeqCst);
        }));
        ctx.on::<TestEvent, _>(Arc::new(move |_: &Context, _: &TestEvent| {
            b2.fetch_add(1, Ordering::SeqCst);
        }));

        ctx.emit(TestEvent {
            msg: "x".to_string(),
        });
        assert_eq!(a.load(Ordering::SeqCst), 1);
        assert_eq!(b.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn listeners_for_different_events_isolated() {
        let ctx = Context::new();
        let counter_called = Arc::new(AtomicUsize::new(0));
        let other_called = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&counter_called);
        let oc = Arc::clone(&other_called);

        ctx.on::<TestEvent, _>(Arc::new(move |_: &Context, _: &TestEvent| {
            cc.fetch_add(1, Ordering::SeqCst);
        }));
        ctx.on::<OtherTestEvent, _>(Arc::new(move |_: &Context, _: &OtherTestEvent| {
            oc.fetch_add(1, Ordering::SeqCst);
        }));

        ctx.emit(TestEvent {
            msg: "x".to_string(),
        });
        assert_eq!(counter_called.load(Ordering::SeqCst), 1);
        assert_eq!(other_called.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn listener_count_reflects_subscriptions() {
        let ctx = Context::new();
        assert_eq!(ctx.listener_count::<TestEvent>(), 0);
        ctx.on::<TestEvent, _>(Arc::new(|_: &Context, _: &TestEvent| {}));
        assert_eq!(ctx.listener_count::<TestEvent>(), 1);
        ctx.on::<TestEvent, _>(Arc::new(|_: &Context, _: &TestEvent| {}));
        assert_eq!(ctx.listener_count::<TestEvent>(), 2);
    }

    #[test]
    fn listener_can_read_ctx_during_emit() {
        // listener 拿 &Context, 可以 ctx.get / ctx.service
        let ctx = Context::new();
        ctx.set(SESSION_ID, "session_42".to_string());

        let captured = Arc::new(parking_lot::Mutex::new(String::new()));
        let cap2 = Arc::clone(&captured);

        ctx.on::<TestEvent, _>(Arc::new(move |ctx: &Context, _ev: &TestEvent| {
            let id = ctx.get(SESSION_ID).unwrap_or_default();
            *cap2.lock() = id;
        }));

        ctx.emit(TestEvent {
            msg: "x".to_string(),
        });
        assert_eq!(*captured.lock(), "session_42");
    }

    #[test]
    #[should_panic(expected = "reentrant emit")]
    fn reentrant_emit_panics() {
        let ctx = Context::new();
        ctx.on::<TestEvent, _>(Arc::new(move |ctx: &Context, _ev: &TestEvent| {
            // listener 内 emit: reentrancy → panic
            ctx.emit(OtherTestEvent);
        }));
        ctx.emit(TestEvent {
            msg: "trigger".to_string(),
        });
    }

    // === Disposable / Scope ===

    struct CountingResource {
        count: Arc<AtomicUsize>,
    }
    impl Disposable for CountingResource {
        fn dispose(&self) -> anyhow::Result<()> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FailingResource;
    impl Disposable for FailingResource {
        fn dispose(&self) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("intentional failure"))
        }
    }

    #[test]
    fn scope_disposes_on_drop() {
        let ctx = Context::new();
        let count = Arc::new(AtomicUsize::new(0));
        let resource = Arc::new(CountingResource {
            count: Arc::clone(&count),
        });

        {
            let scope = ctx.scope();
            scope.add(resource);
            assert_eq!(count.load(Ordering::SeqCst), 0);
        } // scope drop

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ctx_dispose_releases_all_resources() {
        let ctx = Context::new();
        let count = Arc::new(AtomicUsize::new(0));
        let r1 = Arc::new(CountingResource {
            count: Arc::clone(&count),
        });
        let r2 = Arc::new(CountingResource {
            count: Arc::clone(&count),
        });

        ctx.on_dispose(r1);
        ctx.on_dispose(r2);
        assert_eq!(count.load(Ordering::SeqCst), 0);
        ctx.dispose().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert!(ctx.is_disposed());
    }

    #[test]
    fn ctx_dispose_idempotent() {
        let ctx = Context::new();
        let count = Arc::new(AtomicUsize::new(0));
        let r = Arc::new(CountingResource {
            count: Arc::clone(&count),
        });
        ctx.on_dispose(r);
        ctx.dispose().unwrap();
        ctx.dispose().unwrap(); // 第二次 no-op
        ctx.dispose().unwrap(); // 第三次 no-op
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ctx_dispose_returns_first_error() {
        let ctx = Context::new();
        ctx.on_dispose(Arc::new(FailingResource));
        let result = ctx.dispose();
        assert!(result.is_err());
        assert!(ctx.is_disposed(), "dispose 失败也标记 disposed");
    }

    // === Fork ===

    #[test]
    fn fork_inherits_services() {
        let parent = Context::new();
        parent.inject_from::<GreetingService>().unwrap();

        let child = parent.fork();
        assert!(child.service::<GreetingService>().is_some(), "fork 应继承 service");
    }

    #[test]
    fn fork_does_not_inherit_keys_or_plugins() {
        let parent = Context::new();
        parent.set(SESSION_ID, "parent".to_string());
        parent.plugin(HelloPlugin).unwrap();

        let child = parent.fork();
        assert!(child.get(SESSION_ID).is_none(), "fork 不继承 typed key");
        assert!(child.plugins().is_empty(), "fork 不继承 plugin");
    }

    #[test]
    fn fork_shares_service_arc() {
        // fork 跟 extend_from 行为一致: shared Arc, 不是 clone
        let parent = Context::new();
        let p_arc = parent.inject_from::<GreetingService>().unwrap();

        let child = parent.fork();
        let c_arc = child.service::<GreetingService>().unwrap();
        assert!(Arc::ptr_eq(&p_arc, &c_arc));
    }

    #[test]
    fn fork_disposes_independently() {
        let parent = Context::new();
        let parent_count = Arc::new(AtomicUsize::new(0));
        parent.on_dispose(Arc::new(CountingResource {
            count: Arc::clone(&parent_count),
        }));

        let child = parent.fork();
        let child_count = Arc::new(AtomicUsize::new(0));
        child.on_dispose(Arc::new(CountingResource {
            count: Arc::clone(&child_count),
        }));

        parent.dispose().unwrap();
        assert_eq!(parent_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            child_count.load(Ordering::SeqCst),
            0,
            "parent dispose 不应触发 child 的 disposable"
        );
        assert!(!child.is_disposed(), "child 不应受 parent dispose 影响");
    }
}
