//! Disposable — 资源管理 + RAII scope
//!
//! Week 1 Day 5 实现: `Disposable` trait + `Scope` (RAII 包装) + `Context::dispose`.
//!
//! Phase 2.4 (T2.4) 加 `AsyncDisposable` trait: `async fn dispose_async`,
//! 配合 `Context::dispose_all_async` 走 `futures::future::join_all` 并发释放.
//!
//! 设计见 `docs/ma-harness-arch-map.md` §2 (Cordis 元框架).
//!
//! # 关键约束
//!
//! - **dispose 一次性**: 一个 disposable dispose 后再调 dispose 应该 no-op 或 panic (Phase 1: no-op, 记 `disposed: AtomicBool`)
//! - **顺序**: dispose 按 LIFO (后注册先释放, 跟 stack unwinding 一致)
//! - **错误处理**: dispose 失败 (e.g. close file error) 返回 Result, 但 ctx.dispose 不一定 stop
//!   (Phase 1: 收集所有错误, 最后返回第一个)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

/// Disposable 资源 trait (sync)
///
/// 用户资源 (file handle / db connection / child process) impl 这个 trait,
/// 注册到 ctx.scope() 或 ctx.on_dispose().
/// dispose() 在 ctx 销毁时或 scope drop 时调用.
pub trait Disposable: Send + Sync + 'static {
    /// 释放资源
    ///
    /// Phase 1: 失败时返回 Err, ctx.dispose 会收集
    /// Phase 2: 改成 `fn dispose(&self) -> ();` 失败 panic (harness 资源泄漏 = 严重)
    fn dispose(&self) -> anyhow::Result<()>;
}

/// **Phase 2.4 (T2.4) 新增**: 异步 Disposable
///
/// 跟 `Disposable` 并存, 业务方按需选. 异步 dispose 用于:
/// - 网络连接关闭 (async close)
/// - 子进程等待 (async wait)
/// - 远程资源释放 (HTTP DELETE 调下游)
/// - tokio task 取消 / join
///
/// 实现细节:
/// - `dispose_async` 走 `async fn`, 返回 Pin<Box<dyn Future>> 也行
///   (用 `#[async_trait]` 或者用 native async fn 走 blanket impl)
/// - 注册走 `ctx.on_dispose_async(arc)` (独立于 sync entry)
/// - 释放走 `ctx.dispose_all_async().await`, 内部 `futures::future::join_all` 并发跑
/// - 跟 sync `Disposable` 不互通, 业务方要么 sync 要么 async 选一个
///
/// **未来 (Phase 3)**: 合并 sync + async, 走 `async fn dispose(&self) -> Result<()>` 单一 trait,
/// sync 的 wrapper 成 `async fn dispose(&self) { sync_body }`. 当前为 backward compat 保留两份.
pub trait AsyncDisposable: Send + Sync + 'static {
    /// 异步释放资源
    ///
    /// 默认实现: spawn block 给 sync 路径用. 业务方可以 override 走真 async.
    /// 失败时返回 Err, ctx.dispose_all_async 收集第一个错误.
    fn dispose_async<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>;
}

/// 内部 disposable 包装, 记录 disposed 状态避免重复释放
pub(crate) struct DisposableEntry {
    inner: Arc<dyn Disposable>,
    disposed: AtomicBool,
}

impl DisposableEntry {
    pub(crate) fn new(d: Arc<dyn Disposable>) -> Self {
        Self {
            inner: d,
            disposed: AtomicBool::new(false),
        }
    }

    pub(crate) fn dispose(&self) -> anyhow::Result<()> {
        // compare_exchange 保证只 dispose 一次
        if self
            .disposed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.inner.dispose()
        } else {
            // 已 dispose, 静默 no-op
            Ok(())
        }
    }
}

/// 内部 async disposable 包装, 跟 sync entry 完全独立
pub(crate) struct AsyncDisposableEntry {
    inner: Arc<dyn AsyncDisposable>,
    disposed: AtomicBool,
}

impl AsyncDisposableEntry {
    pub(crate) fn new(d: Arc<dyn AsyncDisposable>) -> Self {
        Self {
            inner: d,
            disposed: AtomicBool::new(false),
        }
    }

    pub(crate) async fn dispose(&self) -> anyhow::Result<()> {
        if self
            .disposed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.inner.dispose_async().await
        } else {
            Ok(())
        }
    }
}

/// RAII scope: drop 时 dispose 所有注册的资源
///
/// 用法:
/// ```ignore
/// let scope = ctx.scope();
/// scope.add(file_handle);
/// // scope drop 时, file_handle.dispose() 被调
/// ```
pub struct Scope {
    /// 共享 dispose list (跟 ctx 共享, 以便 ctx.dispose 也能处理)
    entries: Arc<Mutex<Vec<DisposableEntry>>>,
    /// 标记 scope 自身是否已 dispose
    consumed: AtomicBool,
}

impl Scope {
    pub(crate) fn new(entries: Arc<Mutex<Vec<DisposableEntry>>>) -> Self {
        Self {
            entries,
            consumed: AtomicBool::new(false),
        }
    }

    /// 注册一个 disposable
    pub fn add<D: Disposable>(&self, d: Arc<D>) {
        let mut entries = self.entries.lock();
        entries.push(DisposableEntry::new(d));
    }

    /// 主动 dispose (drop 时自动调, 但业务方可以提前)
    pub fn dispose(&self) -> anyhow::Result<()> {
        if self
            .consumed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            // LIFO: 反向遍历
            let mut entries = self.entries.lock();
            let mut first_err: Option<anyhow::Error> = None;
            while let Some(e) = entries.pop() {
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
}

impl Drop for Scope {
    fn drop(&mut self) {
        // best effort, 不 panic
        let _ = self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingResource {
        disposed_count: Arc<AtomicUsize>,
    }
    impl Disposable for CountingResource {
        fn dispose(&self) -> anyhow::Result<()> {
            self.disposed_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FailingResource;
    impl Disposable for FailingResource {
        fn dispose(&self) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("dispose failed"))
        }
    }

    #[test]
    fn scope_disposes_on_drop() {
        let entries: Arc<Mutex<Vec<DisposableEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let resource = Arc::new(CountingResource {
            disposed_count: Arc::clone(&count),
        });

        {
            let scope = Scope::new(Arc::clone(&entries));
            scope.add(resource);
            // scope 还没 drop, 不应 dispose
            assert_eq!(count.load(Ordering::SeqCst), 0);
        } // scope drop here

        // drop 后 dispose 应跑过
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn scope_explicit_dispose() {
        let entries: Arc<Mutex<Vec<DisposableEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let r1 = Arc::new(CountingResource {
            disposed_count: Arc::clone(&count),
        });
        let r2 = Arc::new(CountingResource {
            disposed_count: Arc::clone(&count),
        });

        let scope = Scope::new(entries);
        scope.add(r1);
        scope.add(r2);
        scope.dispose().unwrap();

        // 两个都 dispose 了
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn scope_double_dispose_is_noop() {
        let entries: Arc<Mutex<Vec<DisposableEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let r = Arc::new(CountingResource {
            disposed_count: Arc::clone(&count),
        });

        let scope = Scope::new(entries);
        scope.add(r);
        scope.dispose().unwrap();
        scope.dispose().unwrap(); // 第二次 no-op

        // 只 dispose 一次
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn scope_lifo_order() {
        let entries: Arc<Mutex<Vec<DisposableEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let order: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        struct Ordered(&'static str, Arc<Mutex<Vec<&'static str>>>);
        impl Disposable for Ordered {
            fn dispose(&self) -> anyhow::Result<()> {
                self.1.lock().push(self.0);
                Ok(())
            }
        }

        let scope = Scope::new(entries);
        scope.add(Arc::new(Ordered("first", Arc::clone(&order))));
        scope.add(Arc::new(Ordered("second", Arc::clone(&order))));
        scope.add(Arc::new(Ordered("third", Arc::clone(&order))));
        scope.dispose().unwrap();

        // LIFO: third, second, first
        let order_vec = order.lock();
        assert_eq!(order_vec.as_slice(), &["third", "second", "first"]);
    }

    #[test]
    fn scope_first_error_returned() {
        let entries: Arc<Mutex<Vec<DisposableEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let scope = Scope::new(entries);
        scope.add(Arc::new(FailingResource));
        scope.add(Arc::new(FailingResource));
        let result = scope.dispose();
        assert!(result.is_err());
    }
}
