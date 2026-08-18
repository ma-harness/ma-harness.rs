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
    fn dispatch(&self, ctx: &Context, event: &(dyn std::any::Any + Send + Sync));
}

impl<E: ListenerEvent> AnyListener for Arc<dyn Listener<E>> {
    fn dispatch(&self, ctx: &Context, event: &(dyn std::any::Any + Send + Sync)) {
        if let Some(e) = event.downcast_ref::<E>() {
            Listener::handle(self.as_ref(), ctx, e);
        }
        // downcast 失败: 不应发生 (emit 时保证), 静默忽略
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

    /// 列出订阅某类型事件的所有 listener 数量 (调试用)
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
