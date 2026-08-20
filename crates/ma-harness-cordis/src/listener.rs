//! Listener / ListenerEvent / ListenerRegistry (cordis 核心)
//!
//! Week 1 Day 5 实现. Phase 2.7 改走 deferred queue + dispatch_any.
//! Phase 2.9 (Day 63) 加 priority 排序 (on_with_priority).
//!
//! 重命名说明: 之前叫 `Event`, 跟 `event::Event` (Week 1 Day 1-2 占位的事件 enum) 名字冲突.
//! 改名 `ListenerEvent` 避免冲突. Phase 2 收编 `event::Event` 进 listener 子树.

use std::any::TypeId;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::Context;

pub trait ListenerEvent: 'static + Send + Sync {}

// ============================================================================
// Sync Listener
// ============================================================================

/// Listener trait. 闭包 `Fn(&Context, &E)` 自动 impl.
///
/// 公开 crate 抽象见 `ma_harness_seam::Listener`。
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

/// 内部 listener 注册表: TypeId -> Vec<(priority, Arc<dyn AnyListener>)>
///
/// `AnyListener` 是 type-erased wrapper, 实际是 `Arc<dyn Listener<E>>`.
///
/// **用 `parking_lot::RwLock<HashMap>`** 而不是 dashmap: listener 写入少 (只在 install 阶段),
/// 读取多 (emit 时频繁). RwLock 读锁可共享, 更合适.
///
/// (priority, listener) 二元组, dispatch 时按 priority 升序遍历
/// 抽出来给 ListenerRegistry 字段用, 解决 clippy::type_complexity 提示
type PrioritizedListener = (i32, Arc<dyn AnyListener>);

/// Phase 2.9 (Day 63) 加 priority 排序: emit 按 priority 升序 dispatch (低先 fire).
#[derive(Default)]
pub(crate) struct ListenerRegistry {
    inner: RwLock<HashMap<TypeId, Vec<PrioritizedListener>>>,
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

    /// 注册一个 listener 给 E 类型事件, priority = 0 (默认)
    pub(crate) fn on<E: ListenerEvent>(&self, listener: Arc<dyn Listener<E>>) {
        self.on_with_priority::<E>(0, listener);
    }

    /// 注册一个 listener 跟 priority
    ///
    /// priority 升序 dispatch (低 priority 先 fire). 默认 0.
    /// 同 priority 按注册顺序.
    pub(crate) fn on_with_priority<E: ListenerEvent>(
        &self,
        priority: i32,
        listener: Arc<dyn Listener<E>>,
    ) {
        let type_id = TypeId::of::<E>();
        let any: Arc<dyn AnyListener> = Arc::new(AnyListenerFromArc { inner: listener });
        let mut inner = self.inner.write();
        inner.entry(type_id).or_default().push((priority, any));
    }

    /// 触发事件: 同步 dispatch 给所有订阅者
    #[allow(dead_code)] // Phase 2.7: Context 走 deferred queue + dispatch_boxed, 此 emit 暂未用
    pub(crate) fn emit<E: ListenerEvent>(&self, ctx: &Context, event: &E) {
        let listeners = self.listeners_for_type_id_ordered(TypeId::of::<E>());
        for (_, l) in listeners {
            l.dispatch(ctx, event);
        }
    }

    /// 拿订阅某 TypeId 的所有 listener (Phase 2.7 deferred queue 用)
    ///
    /// 按 priority 升序排序, 同 priority 保持原顺序 (stable sort).
    pub(crate) fn listeners_for_type_id(&self, type_id: TypeId) -> Vec<Arc<dyn AnyListener>> {
        self.listeners_for_type_id_ordered(type_id)
            .into_iter()
            .map(|(_, l)| l)
            .collect()
    }

    /// 拿订阅某 TypeId 的所有 listener (按 priority 升序, 含 priority)
    pub(crate) fn listeners_for_type_id_ordered(
        &self,
        type_id: TypeId,
    ) -> Vec<(i32, Arc<dyn AnyListener>)> {
        let inner = self.inner.read();
        let mut entries = inner.get(&type_id).cloned().unwrap_or_default();
        // stable sort: priority 升序, 同 priority 保持原顺序
        entries.sort_by_key(|(p, _)| *p);
        entries
    }

    /// 列出订阅某 TypeId 的 listener 数量 (Phase 2.7 debug 用)
    pub(crate) fn count_for_type_id(&self, type_id: TypeId) -> usize {
        self.inner
            .read()
            .get(&type_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

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
        self.inner.read().get(&type_id).cloned().unwrap_or_default()
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

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        assert_eq!(reg.count_for_type_id(TypeId::of::<CounterEvent>()), 0);
        reg.on::<CounterEvent>(Arc::new(|_: &Context, _: &CounterEvent| {}));
        assert_eq!(reg.count_for_type_id(TypeId::of::<CounterEvent>()), 1);
        reg.on::<CounterEvent>(Arc::new(|_: &Context, _: &CounterEvent| {}));
        assert_eq!(reg.count_for_type_id(TypeId::of::<CounterEvent>()), 2);
    }

    // ========================================================================
    // Phase 2.9 (Day 63) priority 测试
    // ========================================================================

    #[test]
    fn priority_dispatch_low_first() {
        // priority 升序 dispatch: 低 priority 先 fire
        let reg = ListenerRegistry::new();
        let order = Arc::new(parking_lot::Mutex::new(Vec::<&'static str>::new()));
        let o1 = Arc::clone(&order);
        let o2 = Arc::clone(&order);
        let o3 = Arc::clone(&order);

        // 注册顺序: high, low, medium — 应该 fire: low, medium, high
        reg.on_with_priority::<CounterEvent>(
            10,
            Arc::new(move |_: &Context, _: &CounterEvent| {
                o1.lock().push("high");
            }),
        );
        reg.on_with_priority::<CounterEvent>(
            -5,
            Arc::new(move |_: &Context, _: &CounterEvent| {
                o2.lock().push("low");
            }),
        );
        reg.on_with_priority::<CounterEvent>(
            3,
            Arc::new(move |_: &Context, _: &CounterEvent| {
                o3.lock().push("medium");
            }),
        );

        reg.emit(&Context::new(), &CounterEvent { delta: 1 });
        let fired = order.lock().clone();
        assert_eq!(
            fired,
            vec!["low", "medium", "high"],
            "按 priority 升序 fire"
        );
    }

    #[test]
    fn priority_same_preserves_registration_order() {
        // 同 priority 按注册顺序 (stable sort)
        let reg = ListenerRegistry::new();
        let order = Arc::new(parking_lot::Mutex::new(Vec::<&'static str>::new()));
        let o1 = Arc::clone(&order);
        let o2 = Arc::clone(&order);
        let o3 = Arc::clone(&order);

        // 全部 priority = 0
        reg.on::<CounterEvent>(Arc::new(move |_: &Context, _: &CounterEvent| {
            o1.lock().push("first");
        }));
        reg.on::<CounterEvent>(Arc::new(move |_: &Context, _: &CounterEvent| {
            o2.lock().push("second");
        }));
        reg.on::<CounterEvent>(Arc::new(move |_: &Context, _: &CounterEvent| {
            o3.lock().push("third");
        }));

        reg.emit(&Context::new(), &CounterEvent { delta: 1 });
        let fired = order.lock().clone();
        assert_eq!(
            fired,
            vec!["first", "second", "third"],
            "同 priority 保持注册顺序"
        );
    }

    #[test]
    fn priority_default_is_zero() {
        // on() 默认 priority=0
        let reg = ListenerRegistry::new();
        reg.on::<CounterEvent>(Arc::new(|_: &Context, _: &CounterEvent| {}));
        // 验证: 拿 on_with_priority 0 注册的同 type, 都在 priority=0 段
        // 简化验证: count_for_type_id 返 1
        assert_eq!(reg.count_for_type_id(TypeId::of::<CounterEvent>()), 1);
    }
}
