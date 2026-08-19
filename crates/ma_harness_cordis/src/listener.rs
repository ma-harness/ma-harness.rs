//! Listener — 事件订阅
//!
//! Week 1 Day 5 实现: 强类型事件 + 闭包式 listener + 同步 dispatch.
//!
//! 设计见 `docs/ma-harness-arch-map.md` §2 (Cordis 元框架).
//!
//! # 关键约束
//!
//! - **listener 不能 emit** (Phase 1). 如果在 listener handle 里 emit 另一个 event,
//!   触发 reentrancy guard, panic 报 "reentrant emit".
//! - listener 拿 `&self` (immutable), 不能修改 registry.
//! - listener 拿 `&Context`, 可以 ctx.get / ctx.service, **不可以** ctx.emit.
//!
//! Phase 2 加: async listener / listener priority / reentrancy 队列.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::Context;

/// Event marker trait. 用户事件 enum 派生这个 trait.
///
/// **重命名说明**: 之前叫 `Event`, 跟 `event::Event` (Week 1 Day 1-2 占位的事件 enum) 名字冲突.
/// 改名 `ListenerEvent` 避免冲突. Phase 2 收编 `event::Event` 进 listener 子树.
pub trait ListenerEvent: 'static + Send + Sync {}

// ============================================================================
// AsyncListener (Phase 2.8 / Day 62)
// ============================================================================
//
// 异步版 listener, handle 返 Pin<Box<dyn Future + Send>>. 跟同步 Listener 并存,
// Context 同时维护两套 registry. sync emit 触发 sync listeners, async emit_async
// 触发 async listeners. 业务方选合适 path (网络请求 / DB / LLM API 等).
//
// 设计:
// - AsyncListener<E: ListenerEvent>: async fn handle_async -> Pin<Box<dyn Future + Send>>
// - blanket impl: 任何 `Fn(&Context, &E) -> Fut` 都是 AsyncListener
// - 公开 crate 抽象见 ma_harness_seam::AsyncListener (Phase 2.8 后续)
// - listener 不能 spawn 'static future (lifetime bound), 用 Pin<Box<>> + 'static

use std::future::Future;
use std::pin::Pin;

/// 异步 listener trait
pub trait AsyncListener<E: ListenerEvent>: Send + Sync + 'static {
    /// 异步处理事件, 返回 future
    fn handle_async<'a>(
        &'a self,
        ctx: &'a Context,
        event: &'a E,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

// blanket impl: `Fn(&Context, &E) -> Fut` 自动 impl AsyncListener
impl<F, E, Fut> AsyncListener<E> for F
where
    F: for<'a> Fn(&'a Context, &'a E) -> Fut + Send + Sync + 'static,
    E: ListenerEvent,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn handle_async<'a>(
        &'a self,
        ctx: &'a Context,
        event: &'a E,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(self(ctx, event))
    }
}

// ============================================================================
// AsyncListenerRegistry (Phase 2.8)
// ============================================================================

/// type-erased 异步 listener, 内部 downcast + 调用 handle_async
pub(crate) trait AnyAsyncListener: Send + Sync {
    fn dispatch_async<'a>(
        &'a self,
        ctx: &'a Context,
        event: &'a (dyn std::any::Any + Send + Sync),
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

impl<E: ListenerEvent> AnyAsyncListener for std::sync::Arc<dyn AsyncListener<E>> {
    fn dispatch_async<'a>(
        &'a self,
        ctx: &'a Context,
        event: &'a (dyn std::any::Any + Send + Sync),
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        if let Some(e) = event.downcast_ref::<E>() {
            AsyncListener::handle_async(self.as_ref(), ctx, e)
        } else {
            // downcast 失败: 返已完成 future (no-op)
            Box::pin(async {})
        }
    }
}

struct AnyAsyncListenerFromArc<E: ListenerEvent> {
    inner: std::sync::Arc<dyn AsyncListener<E>>,
}

impl<E: ListenerEvent> AnyAsyncListener for AnyAsyncListenerFromArc<E> {
    fn dispatch_async<'a>(
        &'a self,
        ctx: &'a Context,
        event: &'a (dyn std::any::Any + Send + Sync),
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        if let Some(e) = event.downcast_ref::<E>() {
            AsyncListener::handle_async(self.inner.as_ref(), ctx, e)
        } else {
            Box::pin(async {})
        }
    }
}

#[derive(Default)]
pub(crate) struct AsyncListenerRegistry {
    inner: RwLock<std::collections::HashMap<TypeId, Vec<std::sync::Arc<dyn AnyAsyncListener>>>>,
}

impl AsyncListenerRegistry {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn on<E: ListenerEvent>(&self, listener: std::sync::Arc<dyn AsyncListener<E>>) {
        let type_id = TypeId::of::<E>();
        let any: std::sync::Arc<dyn AnyAsyncListener> =
            std::sync::Arc::new(AnyAsyncListenerFromArc { inner: listener });
        let mut inner = self.inner.write();
        inner.entry(type_id).or_default().push(any);
    }

    pub(crate) fn listeners_for_type_id(
        &self,
        type_id: TypeId,
    ) -> Vec<std::sync::Arc<dyn AnyAsyncListener>> {
        self.inner
            .read()
            .get(&type_id)
            .cloned()
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub(crate) fn count_for_type_id(&self, type_id: TypeId) -> usize {
        self.inner
            .read()
            .get(&type_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

/// Listener trait. 闭包 `Fn(&Context, &E)` 自动 impl.
///
/// 公开 crate 抽象见 `ma_harness_seam::Listener`.
pub trait Listener<E: ListenerEvent>: Send + Sync + 'static {
    /// 事件触发时调
    fn handle(&self, ctx: &Context, event: &E);
}

// blanket impl: 任何 `Fn(&Context, &E) + Send + Sync + 'static` 都是 Listener
impl<F, E> Listener<E> for F
where
    F: Fn(&Context, &E) + Send + Sync + 'static,
    E: ListenerEvent,
{
    fn handle(&self, ctx: &Context, event: &E) {
        self(ctx, event)
    }
}

/// 内部 listener 注册表: TypeId -> Vec<Arc<dyn AnyListener>>
///
/// `AnyListener` 是 type-erased wrapper, 实际是 `Arc<dyn Listener<E>>`.
///
/// **用 `parking_lot::RwLock<HashMap>`** 而不是 dashmap: listener 写入少 (只在 install 阶段),
/// 读取多 (emit 时频繁). RwLock 读锁可共享, 更合适.
#[derive(Default)]
pub(crate) struct ListenerRegistry {
    inner: RwLock<HashMap<TypeId, Vec<Arc<dyn AnyListener>>>>,
}

/// type-erased listener, 内部 downcast 到具体 E 调 handle
pub(crate) trait AnyListener: Send + Sync {
    #[allow(dead_code)] // Phase 2.7: 走 dispatch_any 路径, dispatch 留给 Phase 1 emit
    fn dispatch(&self, ctx: &Context, event: &(dyn std::any::Any + Send + Sync));
    /// 2026-08-18 (Day 61): dispatch from `&dyn Any` (Box<MyEvent> deref 出来的).
    fn dispatch_any(&self, ctx: &Context, event: &(dyn std::any::Any + Send + Sync));
}

impl<E: ListenerEvent> AnyListener for Arc<dyn Listener<E>> {
    fn dispatch(&self, ctx: &Context, event: &(dyn std::any::Any + Send + Sync)) {
        if let Some(e) = event.downcast_ref::<E>() {
            Listener::handle(self.as_ref(), ctx, e);
        }
        // downcast 失败: 不应发生 (emit 时保证), 静默忽略
    }
    fn dispatch_any(&self, ctx: &Context, event: &(dyn std::any::Any + Send + Sync)) {
        if let Some(e) = event.downcast_ref::<E>() {
            Listener::handle(self.as_ref(), ctx, e);
        }
    }
}

// 2026-08-18 修复 E0308: 加 AnyListenerFromArc wrapper, 让 Arc<dyn Listener<E>> 通过新类型
// 转成 Arc<dyn AnyListener>
struct AnyListenerFromArc<E: ListenerEvent> {
    inner: Arc<dyn Listener<E>>,
}

impl<E: ListenerEvent> AnyListener for AnyListenerFromArc<E> {
    fn dispatch(&self, ctx: &Context, event: &(dyn std::any::Any + Send + Sync)) {
        if let Some(e) = event.downcast_ref::<E>() {
            Listener::handle(self.inner.as_ref(), ctx, e);
        }
    }
    fn dispatch_any(&self, ctx: &Context, event: &(dyn std::any::Any + Send + Sync)) {
        if let Some(e) = event.downcast_ref::<E>() {
            Listener::handle(self.inner.as_ref(), ctx, e);
        }
    }
}

impl ListenerRegistry {
    /// 构造一个新的 listener registry
    #[allow(dead_code)] // 测试用
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 注册一个 listener 给 E 类型事件
    pub(crate) fn on<E: ListenerEvent>(&self, listener: Arc<dyn Listener<E>>) {
        let type_id = TypeId::of::<E>();
        // 2026-08-18 修复: 用 wrap newtype 转换 Arc<dyn Listener<E>> -> Arc<dyn AnyListener>
        // 走 AnyListenerFromArc 包装, 委托给原 Arc<dyn Listener<E>>
        // (trait upcasting 1.86+ 在 Arc<dyn _> 上不稳定, 用显式 wrap 更稳)
        let any: Arc<dyn AnyListener> = Arc::new(AnyListenerFromArc { inner: listener });
        let mut inner = self.inner.write();
        inner.entry(type_id).or_default().push(any);
    }

    /// 触发事件: 同步 dispatch 给所有订阅者
    #[allow(dead_code)] // Phase 2.7: Context 走 deferred queue + dispatch_boxed, 此 emit 暂未用
    pub(crate) fn emit<E: ListenerEvent>(&self, ctx: &Context, event: &E) {
        // 1. clone 出 listener vec, 释放锁
        let type_id = TypeId::of::<E>();
        let listeners: Vec<Arc<dyn AnyListener>> = {
            let inner = self.inner.read();
            inner.get(&type_id).cloned().unwrap_or_default()
        };

        // 2. 循环 dispatch
        for l in listeners {
            l.dispatch(ctx, event);
        }
    }

    /// 拿订阅某 TypeId 的所有 listener (Phase 2.7 deferred queue 用)
    pub(crate) fn listeners_for_type_id(
        &self,
        type_id: TypeId,
    ) -> Vec<Arc<dyn AnyListener>> {
        self.inner
            .read()
            .get(&type_id)
            .cloned()
            .unwrap_or_default()
    }

    /// 列出订阅某 TypeId 的 listener 数量 (Phase 2.7 debug 用)
    pub(crate) fn count_for_type_id(&self, type_id: TypeId) -> usize {
        self.inner
            .read()
            .get(&type_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// 列出订阅某类型事件的所有 listener 数量 (调试用)
    #[allow(dead_code)]
    pub(crate) fn count<E: ListenerEvent>(&self) -> usize {
        let type_id = TypeId::of::<E>();
        self.inner
            .read()
            .get(&type_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // 测试事件
    #[derive(Debug, Clone)]
    struct CounterEvent {
        delta: i32,
    }
    impl ListenerEvent for CounterEvent {}

    #[derive(Debug, Clone)]
    struct OtherEvent;
    impl ListenerEvent for OtherEvent {}

    #[test]
    fn on_and_emit_calls_listener() {
        let reg = ListenerRegistry::new();
        let called = Arc::new(AtomicUsize::new(0));
        let called_clone = Arc::clone(&called);

        reg.on::<CounterEvent>(Arc::new(move |_ctx: &Context, ev: &CounterEvent| {
            assert_eq!(ev.delta, 5);
            called_clone.fetch_add(1, Ordering::SeqCst);
        }));

        reg.emit(&Context::new(), &CounterEvent { delta: 5 });
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn emit_with_no_listener_is_noop() {
        let reg = ListenerRegistry::new();
        // 不 panic
        reg.emit(&Context::new(), &OtherEvent);
    }

    #[test]
    fn multiple_listeners_all_called() {
        let reg = ListenerRegistry::new();
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        let a2 = Arc::clone(&a);
        let b2 = Arc::clone(&b);

        reg.on::<CounterEvent>(Arc::new(move |_: &Context, _: &CounterEvent| {
            a2.fetch_add(1, Ordering::SeqCst);
        }));
        reg.on::<CounterEvent>(Arc::new(move |_: &Context, _: &CounterEvent| {
            b2.fetch_add(1, Ordering::SeqCst);
        }));

        reg.emit(&Context::new(), &CounterEvent { delta: 1 });
        assert_eq!(a.load(Ordering::SeqCst), 1);
        assert_eq!(b.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn listeners_for_different_events_isolated() {
        let reg = ListenerRegistry::new();
        let counter_called = Arc::new(AtomicUsize::new(0));
        let other_called = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&counter_called);
        let oc = Arc::clone(&other_called);

        reg.on::<CounterEvent>(Arc::new(move |_: &Context, _: &CounterEvent| {
            cc.fetch_add(1, Ordering::SeqCst);
        }));
        reg.on::<OtherEvent>(Arc::new(move |_: &Context, _: &OtherEvent| {
            oc.fetch_add(1, Ordering::SeqCst);
        }));

        reg.emit(&Context::new(), &CounterEvent { delta: 1 });
        assert_eq!(counter_called.load(Ordering::SeqCst), 1);
        assert_eq!(other_called.load(Ordering::SeqCst), 0);

        reg.emit(&Context::new(), &OtherEvent);
        assert_eq!(other_called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn count_returns_listener_count() {
        let reg = ListenerRegistry::new();
        assert_eq!(reg.count::<CounterEvent>(), 0);
        reg.on::<CounterEvent>(Arc::new(|_: &Context, _: &CounterEvent| {}));
        assert_eq!(reg.count::<CounterEvent>(), 1);
        reg.on::<CounterEvent>(Arc::new(|_: &Context, _: &CounterEvent| {}));
        assert_eq!(reg.count::<CounterEvent>(), 2);
    }
}
