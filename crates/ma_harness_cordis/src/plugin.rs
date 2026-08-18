//! Plugin trait + 注册表
//!
//! 插件是 "一组 service / tool / listener / command / handler 的打包".
//! 启动时通过 `ctx.plugin(MyPlugin)` 装载, 卸载时 `uninstall()`.
//!
//! # 关键点 (见 arch-map §2)
//!
//! - `install` 是 `&self` 不是 `&mut self` (Phase 1 简化, 装载阶段写入 ctx 即可)
//! - `name(&self)` 调试用
//! - `uninstall` 默认 no-op, Phase 2 真正实现资源释放

use crate::{Context, CordisError};
use std::collections::HashMap;
use std::sync::Arc;

/// Plugin trait
///
/// 公开 crate 抽象见 `ma_harness_seam::Plugin`.
pub trait Plugin: Send + Sync + 'static {
    /// 安装到 ctx (可写 ctx 注入 service / 注册 tool / 订阅 listener)
    fn install(&self, ctx: &Context) -> anyhow::Result<()>;

    /// 插件名
    fn name(&self) -> &str;

    /// 卸载 (Phase 1 默认 no-op)
    fn uninstall(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// 全局插件注册表 (内部用)
///
/// Context 内部持有一个. Phase 1 用 `parking_lot::Mutex<HashMap>`,
/// Phase 2 改成 `dashmap` 提升并发读.
#[derive(Default)]
pub(crate) struct PluginRegistry {
    inner: parking_lot::Mutex<HashMap<String, Arc<dyn Plugin>>>,
}

impl PluginRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&self, plugin: Arc<dyn Plugin>) -> Result<(), CordisError> {
        let mut inner = self.inner.lock();
        let name = plugin.name().to_string();
        if inner.contains_key(&name) {
            return Err(CordisError::PluginAlreadyRegistered(name));
        }
        inner.insert(name, plugin);
        Ok(())
    }

    pub(crate) fn unregister(&self, name: &str) -> Result<Arc<dyn Plugin>, CordisError> {
        let mut inner = self.inner.lock();
        inner
            .remove(name)
            .ok_or_else(|| CordisError::PluginNotFound(name.to_string()))
    }

    pub(crate) fn get(&self, name: &str) -> Option<Arc<dyn Plugin>> {
        self.inner.lock().get(name).cloned()
    }

    pub(crate) fn list(&self) -> Vec<String> {
        self.inner.lock().keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Context;

    struct HelloPlugin;

    impl Plugin for HelloPlugin {
        fn install(&self, _ctx: &Context) -> anyhow::Result<()> {
            Ok(())
        }
        fn name(&self) -> &str {
            "hello"
        }
    }

    #[test]
    fn register_and_list() {
        let reg = PluginRegistry::new();
        reg.register(Arc::new(HelloPlugin)).unwrap();
        assert_eq!(reg.list(), vec!["hello".to_string()]);
    }

    #[test]
    fn duplicate_register_errors() {
        let reg = PluginRegistry::new();
        reg.register(Arc::new(HelloPlugin)).unwrap();
        let err = reg.register(Arc::new(HelloPlugin)).unwrap_err();
        assert!(matches!(err, CordisError::PluginAlreadyRegistered(_)));
    }

    #[test]
    fn unregister_returns_plugin() {
        let reg = PluginRegistry::new();
        reg.register(Arc::new(HelloPlugin)).unwrap();
        let p = reg.unregister("hello").unwrap();
        assert_eq!(p.name(), "hello");
        assert!(reg.list().is_empty());
    }
}
