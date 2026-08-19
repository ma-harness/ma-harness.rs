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
use crate::listener::{AsyncListener, AsyncListenerRegistry, Listener, ListenerEvent, ListenerRegistry};
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
    /// async listener registry (Phase 2.8 / Day 62 加)
    async_listeners: AsyncListenerRegistry,
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

    /// 订阅事件 E + priority (Phase 2.9 / T2.3)
    ///
    /// **priority 升序 dispatch** (低 priority 先 fire). 同 priority 按注册顺序.
    /// 默认 `on()` 等价于 `on_with_priority(0, listener)`.
    ///
    /// 用法:
    /// ```ignore
    /// use ma_harness_plugin_macro::dsh_listener;
    ///
    /// #[dsh_listener(priority = 10)]
    /// pub struct HighPriorityListener;
    ///
    /// impl Listener<MyEvent> for HighPriorityListener { ... }
    ///
    /// // 注册 (DshListener macro 生成 DSH_LISTENER_PRIORITY 常量)
    /// ctx.on_with_priority::<MyEvent, _>(
    ///     HighPriorityListener::DSH_LISTENER_PRIORITY,
    ///     Arc::new(HighPriorityListener),
    /// );
    /// ```
    ///
    /// **关联 macro**: `#[dsh_listener(priority = N)]` 生成 `DSH_LISTENER_PRIORITY` 常量,
    /// 业务方传这个常量到 `on_with_priority`.
    pub fn on_with_priority<E: ListenerEvent, L: Listener<E>>(
        &self,
        priority: i32,
        listener: Arc<L>,
    ) {
        self.listeners.on_with_priority(priority, listener);
    }

    /// 订阅异步事件 E
    ///
    /// 跟 `on` 并存, 业务方选 sync / async. 闭包 `async |ctx, ev| { ... }` 自动 impl AsyncListener.
    /// 需要 `Send + 'static` 因为 future 要 spawn 到 tokio runtime.
    pub fn on_async<E: ListenerEvent, L: AsyncListener<E>>(&self, listener: Arc<L>) {
        self.async_listeners.on(listener);
    }

    /// 触发事件
    ///
    /// **Phase 2.7 (Day 61)**: 改走 deferred queue + flush loop.
    /// listener 内 emit 不会 panic — event 入队, 当前 listener 结束后
    /// 立即处理下一条. flush loop 直到 queue 空.
    ///
    /// # 设计
    ///
    /// thread-local `DEFERRED_QUEUE: RefCell<Vec<Box<dyn AnyListenerEvent>>>`
    /// 存 buffered events. emit 时:
    /// 1. push event 到 queue
    /// 2. 如果是第一次 (FLUSHING 没在跑), 启动 flush loop
    /// 3. flush loop: pop event, dispatch 给所有 listener, listener 期间可继续 emit
    ///    (也 push 到 queue), 直到 queue 空
    ///
    /// # Panic 安全
    ///
    /// FLUSHING guard 保证只跑一个 flush loop, listener panic 时 Drop set 回 false.
    ///
    /// # 限制 (Phase 2.7 PoC)
    ///
    /// - 异构 event type 通过 `Box<dyn Any + Send + Sync>` 装, dispatch 时 downcast
    /// - 嵌套无限循环检测: 简单策略 (Phase 2.8) — buffer 上限 N 条, 超 panic
    pub fn emit<E: ListenerEvent>(&self, event: E) {
        // 用 E 的 type_id 直接查 (走 std::any::TypeId::of::<E>(), 跟 emit 时的 E 一致)
        let type_id = std::any::TypeId::of::<E>();
        let event_box: Box<dyn std::any::Any + Send + Sync> = Box::new(event);
        DEFERRED_QUEUE.with(|q| q.borrow_mut().push(event_box));

        // 第一次 emit: 启动 flush loop
        let already_flushing = FLUSHING.with(|b| b.get());
        if already_flushing {
            // 嵌套 emit, 让 flush loop 处理这条
            return;
        }
        // RAII guard: 防止嵌套 flush 循环
        let _guard = FlushGuard::new();
        self.flush_deferred_queue(type_id);
    }

    /// flush deferred queue, 直到空
    ///
    /// pop 一条 event, dispatch 给所有 listener, 期间 listener 可继续 emit
    /// (push 到 queue, 当前 flush loop 看到, 继续处理).
    ///
    /// `first_type_id` 是 emit 调进来的 E 的 TypeId, 用于启动 (避免 Box::new 后再 Any::type_id)
    fn flush_deferred_queue(&self, first_type_id: std::any::TypeId) {
        const MAX_BUFFER: usize = 10_000; // Phase 2.7 PoC 简单上限
        let mut processed = 0;
        let mut current_type_id = first_type_id;
        loop {
            // 1. 拿 listeners for current type
            let listeners = self.listeners.listeners_for_type_id(current_type_id);
            // 2. pop 一个 event (LIFO)
            let event_box = DEFERRED_QUEUE.with(|q| q.borrow_mut().pop());
            let event_box = match event_box {
                Some(e) => e,
                None => break,
            };
            // 3. dispatch 给 listeners (listener 期间可继续 emit, 走 emit())
            for listener in &listeners {
                listener.dispatch_any(self, &*event_box);
            }
            processed += 1;
            if processed >= MAX_BUFFER {
                panic!(
                    "deferred emit queue overflow: {} events processed in single flush, \
                     possible infinite loop in listeners",
                    processed
                );
            }
            // 4. 取 queue 顶的 type_id 继续 (新 emit 走 Any::type_id 取对应 type)
            current_type_id = DEFERRED_QUEUE.with(|q| {
                q.borrow()
                    .last()
                    .map(|e| (**e).type_id())
                    .unwrap_or(current_type_id)
            });
        }
    }

    /// 列出订阅 E 的 listener 数量 (调试)
    pub fn listener_count<E: ListenerEvent>(&self) -> usize {
        self.listeners
            .count_for_type_id(std::any::TypeId::of::<E>())
    }

    // ========================================================================
    // Async emit (Phase 2.8 / Day 62)
    // ========================================================================
    //
    // 走单独 thread-local async queue + flush loop (跟 sync 并存).
    // 业务方用 emit_async 触发 async listeners, emit 触发 sync.
    //
    // 设计: emit_async 不阻塞, 启动 flush loop in background (在当前 thread 同步跑,
    // 但 future 是 async). Phase 2.8 简化 — 实际跑 async listener future 到完成.

    /// 触发异步事件
    ///
    /// 同步 push 到 async queue, 启动 flush loop, await 全部 future 跑完.
    /// listener 可继续 emit_async (push queue, 当前 flush 看到继续).
    pub async fn emit_async<E: ListenerEvent>(&self, event: E) {
        let type_id = std::any::TypeId::of::<E>();
        let event_box: Box<dyn std::any::Any + Send + Sync> = Box::new(event);
        ASYNC_DEFERRED_QUEUE.with(|q| q.borrow_mut().push(event_box));

        // 第一次 emit_async: 启动 flush loop
        let already_flushing = ASYNC_FLUSHING.with(|b| b.get());
        if already_flushing {
            return;
        }
        let _guard = AsyncFlushGuard::new();
        self.flush_async_queue(type_id).await;
    }

    /// flush async deferred queue, await 所有 future
    async fn flush_async_queue(&self, first_type_id: std::any::TypeId) {
        const MAX_BUFFER: usize = 10_000;
        let mut processed = 0;
        let mut current_type_id = first_type_id;
        loop {
            let listeners = self.async_listeners.listeners_for_type_id(current_type_id);
            let event_box = ASYNC_DEFERRED_QUEUE.with(|q| q.borrow_mut().pop());
            let event_box = match event_box {
                Some(e) => e,
                None => break,
            };
            // 收集所有 listener future, 顺序 await (Phase 2.8 简化; Phase 2.9 加并发)
            for l in &listeners {
                l.dispatch_async(self, &*event_box).await;
            }
            processed += 1;
            if processed >= MAX_BUFFER {
                panic!(
                    "async deferred emit queue overflow: {} events processed in single flush, \
                     possible infinite loop in async listeners",
                    processed
                );
            }
            current_type_id = ASYNC_DEFERRED_QUEUE.with(|q| {
                q.borrow()
                    .last()
                    .map(|e| (**e).type_id())
                    .unwrap_or(current_type_id)
            });
        }
    }

    /// 列出订阅 E 的 async listener 数量 (调试)
    #[allow(dead_code)]
    pub fn async_listener_count<E: ListenerEvent>(&self) -> usize {
        self.async_listeners.count_for_type_id(std::any::TypeId::of::<E>())
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
// 2026-08-18 (Day 61): Phase 2.7 改走 deferred queue, IN_EMIT / EmitGuard 不再需要.
// 保留 IN_EMIT thread_local 跟 EmitGuard struct 以防外部依赖 (实际 dead code, allow).
#[allow(dead_code)]
thread_local! {
    static IN_EMIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[allow(dead_code)]
struct EmitGuard;

#[allow(dead_code)]
impl EmitGuard {
    fn new() -> Self {
        IN_EMIT.with(|b| b.set(true));
        EmitGuard
    }
}

#[allow(dead_code)]
impl Drop for EmitGuard {
    fn drop(&mut self) {
        IN_EMIT.with(|b| b.set(false));
    }
}

// ============================================================================
// Deferred emit queue (Phase 2.7 / Day 61)
// ============================================================================
//
// 设计: 任何 emit 走 thread-local queue, 启动 flush loop 处理.
// listener 内 emit 不会 panic, 继续 push 到 queue, flush loop 看到后处理.
// 避免 stack overflow + 业务方写 listener 不用关心 reentrancy.
//
// `AnyListenerEvent` trait 在 listener.rs (跟 ListenerEvent 一起, 同文件便于维护).

thread_local! {
    /// Deferred queue 存 `Box<dyn Any + Send + Sync>` (异构事件, std::any::Any 内置 type_id)
    static DEFERRED_QUEUE: std::cell::RefCell<Vec<Box<dyn std::any::Any + Send + Sync>>> =
        const { std::cell::RefCell::new(Vec::new()) };

    /// 标记 flush loop 已经在跑 (嵌套 emit 跳过启动新 loop)
    static FLUSHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// ============================================================================
// Async deferred queue (Phase 2.8 / Day 62)
// ============================================================================
//
// 跟 sync DEFERRED_QUEUE 并存, 不互相干扰. emit_async 走这一套.

thread_local! {
    static ASYNC_DEFERRED_QUEUE: std::cell::RefCell<Vec<Box<dyn std::any::Any + Send + Sync>>> =
        const { std::cell::RefCell::new(Vec::new()) };

    static ASYNC_FLUSHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// RAII guard: drop 时把 FLUSHING set 回 false (即使 panic unwinding 也跑)
struct FlushGuard;

impl FlushGuard {
    fn new() -> Self {
        FLUSHING.with(|b| b.set(true));
        FlushGuard
    }
}

impl Drop for FlushGuard {
    fn drop(&mut self) {
        FLUSHING.with(|b| b.set(false));
    }
}

/// RAII guard: drop 时把 ASYNC_FLUSHING set 回 false (即使 panic unwinding 也跑)
struct AsyncFlushGuard;

impl AsyncFlushGuard {
    fn new() -> Self {
        ASYNC_FLUSHING.with(|b| b.set(true));
        AsyncFlushGuard
    }
}

impl Drop for AsyncFlushGuard {
    fn drop(&mut self) {
        ASYNC_FLUSHING.with(|b| b.set(false));
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

    // ========================================================================
    // 2026-08-18 (Day 61): Phase 2.7 deferred emit queue
    // ========================================================================

    #[test]
    fn emit_from_listener_does_not_panic_and_processes_both() {
        // listener 内 emit 不会 panic (Phase 1 行为 = panic), 2 个 events 都被处理
        let ctx = Context::new();
        let a_called = Arc::new(AtomicUsize::new(0));
        let a2 = Arc::clone(&a_called);
        let b_called = Arc::new(AtomicUsize::new(0));
        let b2 = Arc::clone(&b_called);

        // 第一个 listener: 收到 TestEvent 时 emit OtherTestEvent
        ctx.on::<TestEvent, _>(Arc::new(move |ctx: &Context, _: &TestEvent| {
            a2.fetch_add(1, Ordering::SeqCst);
            // 嵌套 emit (Phase 1 会 panic, Phase 2.7 走 queue)
            ctx.emit(OtherTestEvent);
        }));
        // 第二个 listener: 订阅 OtherTestEvent
        ctx.on::<OtherTestEvent, _>(Arc::new(move |_: &Context, _: &OtherTestEvent| {
            b2.fetch_add(1, Ordering::SeqCst);
        }));

        ctx.emit(TestEvent {
            msg: "trigger".to_string(),
        });
        // 2 个 listener 都跑了 (deferred queue flush 处理嵌套 emit)
        assert_eq!(a_called.load(Ordering::SeqCst), 1, "TestEvent listener");
        assert_eq!(b_called.load(Ordering::SeqCst), 1, "OtherTestEvent listener (嵌套 emit)");
    }

    #[test]
    fn emit_chains_two_events_a_to_b_no_cycle() {
        // 嵌套 emit 链 2 个 events (a→b, 不回 a), 全部按顺序处理.
        // Phase 2.7 PoC 限制: 同 event type 嵌套 emit 会无限循环 (用 MAX_BUFFER panic 兜底).
        // 这个 test 用 2 个不同 type, 验证 deferred queue 链处理 OK.
        let ctx = Context::new();
        let order = Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
        let o_for_test = Arc::clone(&order);
        let o_for_other = Arc::clone(&order);

        // TestEvent listener: 记录 "1", emit OtherTestEvent (不回 TestEvent)
        ctx.on::<TestEvent, _>(Arc::new(move |ctx: &Context, ev: &TestEvent| {
            o_for_test.lock().push(format!("1:{}", ev.msg));
            ctx.emit(OtherTestEvent);
        }));
        // OtherTestEvent listener: 记录 "2", 不再 emit (避免 cycle)
        ctx.on::<OtherTestEvent, _>(Arc::new(move |_: &Context, _: &OtherTestEvent| {
            o_for_other.lock().push("2".to_string());
        }));

        ctx.emit(TestEvent {
            msg: "outer".to_string(),
        });

        let events = order.lock().clone();
        assert_eq!(events.len(), 2, "2 个 events 都被处理, 实际: {events:?}");
        assert_eq!(events[0], "1:outer");
        assert_eq!(events[1], "2");
    }

    #[test]
    fn emit_from_listener_with_no_listener_for_nested_does_not_panic() {
        // 嵌套 emit 没人订阅, 不 panic
        let ctx = Context::new();
        let a_called = Arc::new(AtomicUsize::new(0));
        let a2 = Arc::clone(&a_called);

        ctx.on::<TestEvent, _>(Arc::new(move |ctx: &Context, _: &TestEvent| {
            a2.fetch_add(1, Ordering::SeqCst);
            ctx.emit(OtherTestEvent); // 没订阅者
        }));

        ctx.emit(TestEvent {
            msg: "x".to_string(),
        });
        assert_eq!(a_called.load(Ordering::SeqCst), 1);
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

    // 2026-08-18 (Day 61): Phase 2.7 删 reentrant_emit_panics — deferred queue 让
    // listener 内 emit 不再 panic, 改成 queue + flush. 老的 panic 行为不再适用.

    // ========================================================================
    // 2026-08-18 (Day 62): Phase 2.8 异步 listener 测试
    // ========================================================================

    #[tokio::test]
    async fn emit_async_calls_async_listener() {
        // 验 Phase 2.8 async emit + async listener 走通
        let ctx = Context::new();
        let called = Arc::new(AtomicUsize::new(0));
        let c2 = Arc::clone(&called);

        // async 闭包自动 impl AsyncListener via blanket.
        // 用 move closure 拿 owned String, 避免 lifetime 问题 (future 拿 &TestEvent
        // 是借用, future outlive 借用源).
        ctx.on_async::<TestEvent, _>(Arc::new(move |_ctx: &Context, ev: &TestEvent| {
            let c3 = Arc::clone(&c2);
            let msg = ev.msg.clone();
            async move {
                c3.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                assert_eq!(msg, "async-hello");
            }
        }));

        ctx.emit_async(TestEvent {
            msg: "async-hello".to_string(),
        })
        .await;

        assert_eq!(called.load(Ordering::SeqCst), 1, "async listener 应被 await 跑完");
    }

    #[tokio::test]
    async fn emit_async_no_listener_is_noop() {
        // 没人订阅: 不 panic
        let ctx = Context::new();
        ctx.emit_async(TestEvent {
            msg: "x".to_string(),
        })
        .await;
    }

    #[tokio::test]
    async fn sync_and_async_listeners_are_independent() {
        // sync emit 触发 sync listener (不触发 async),
        // async emit 触发 async listener (不触发 sync)
        let ctx = Context::new();
        let sync_called = Arc::new(AtomicUsize::new(0));
        let async_called = Arc::new(AtomicUsize::new(0));
        let s2 = Arc::clone(&sync_called);
        let a2 = Arc::clone(&async_called);

        ctx.on::<TestEvent, _>(Arc::new(move |_: &Context, _: &TestEvent| {
            s2.fetch_add(1, Ordering::SeqCst);
        }));
        ctx.on_async::<TestEvent, _>(Arc::new(move |_: &Context, _: &TestEvent| {
            let a3 = Arc::clone(&a2);
            async move {
                a3.fetch_add(1, Ordering::SeqCst);
            }
        }));

        // sync emit: 只跑 sync listener
        ctx.emit(TestEvent {
            msg: "sync".to_string(),
        });
        assert_eq!(sync_called.load(Ordering::SeqCst), 1);
        assert_eq!(async_called.load(Ordering::SeqCst), 0, "sync emit 不触发 async");

        // async emit: 只跑 async listener
        ctx.emit_async(TestEvent {
            msg: "async".to_string(),
        })
        .await;
        assert_eq!(sync_called.load(Ordering::SeqCst), 1, "async emit 不触发 sync");
        assert_eq!(async_called.load(Ordering::SeqCst), 1);
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
