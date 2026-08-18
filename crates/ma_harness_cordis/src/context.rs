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

use crate::key::CtxKey;
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
    pub fn get<T: Clone + Send + Sync + 'static>(&self, key: CtxKey<T>) -> Option<T> {
        self.storage
            .get(key.name())
            .and_then(|entry| entry.downcast_ref::<T>())
            .cloned()
    }

    /// 取一个 typed 值 (引用, 零拷贝)
    pub fn get_ref<T: Send + Sync + 'static>(&self, key: CtxKey<T>) -> Option<&T> {
        self.storage
            .get(key.name())
            .and_then(|entry| entry.downcast_ref::<T>())
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
    pub fn inject<S: Service>(&self, service: Arc<S>) {
        self.services.insert(TypeId::of::<S>(), service);
    }

    /// 注入一个由 install 构造的 service (便捷方法)
    ///
    /// 错误处理: install 返回 `S::Error`,通过 `Display + Send + Sync` 边界强转 string,
    /// 不要求 `S::Error: Into<CordisError>` (避免给每个 Service 强加 Into bound).
    pub fn inject_from<S: Service>(&self) -> Result<Arc<S>, CordisError> {
        let svc = S::install(self).map_err(|e| {
            tracing::error!(error = %e, service = std::any::type_name::<S>(), "service install failed");
            CordisError::Other(format!("service install failed: {}", e))
        })?;
        let arc = Arc::new(svc);
        self.services.insert(TypeId::of::<S>(), arc.clone());
        Ok(arc)
    }

    /// 取一个 service (克隆 Arc, 不消耗原注册)
    pub fn service<S: Service>(&self) -> Option<Arc<S>> {
        self.services
            .get(&TypeId::of::<S>())
            .and_then(|entry| entry.downcast_ref::<Arc<S>>().cloned())
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
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("storage_keys", &self.storage.len())
            .field("services", &self.services.len())
            .field("plugins", &self.plugins.list())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CtxKey, Plugin, Service};

    static SESSION_ID: CtxKey<String> = CtxKey::new("session_id");
    static MAX_TOKENS: CtxKey<u32> = CtxKey::new("max_tokens");

    struct GreetingService;

    impl Service for GreetingService {
        type Error = anyhow::Error;
        fn install(_ctx: &Context) -> anyhow::Result<Self> {
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
}
