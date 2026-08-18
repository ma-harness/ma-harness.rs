//! ma_harness_cordis — 元框架 (Cordis-rs)
//!
//! **内部 crate** (2026-08-18 锁定). API 频繁变, 改它不要走 ADR.
//! 插件作者**不**直接 use 这个 crate, 走 [`ma_harness_seam`](https://docs.rs/ma_harness_seam) 抽象层.
//!
//! 本 crate 提供:
//! - [`Context`] — DI 容器, 强类型 ctx key
//! - [`Service`] — 服务 trait, 跟 Context 双向注入
//! - [`Plugin`] — 插件 trait, 声明 install / uninstall
//! - [`Listener`] — 事件订阅, typed event enum
//! - 内部事件总线 + disposable 资源管理
//!
//! Week 1-2 计划: 最小可用 (`Context` + `Service` + `Plugin`), hello-world 端到端跑通.
//! 完整 API (listener / command / disposable / fork / dispose) Week 2 完成.
//!
//! 详细设计见 [`docs/ma-harness-arch-map.md`](../docs/ma-harness-arch-map.html) §2.
//! Spec 阶段, 尚未实现. 占位文件.

#![deny(unsafe_code)]
#![warn(missing_docs)]

// 占位阶段, 尚未实现真实逻辑. Week 1 Day 1-2 替换.

/// DI 容器
///
/// Week 1 Day 1 详细设计 + 实现. 占位阶段是空 struct.
#[derive(Debug, Default)]
pub struct Context {
    _private: (),
}

/// 服务 trait (所有可注入到 ctx 的服务都要 impl)
pub trait Service: Send + Sync + 'static {
    /// 关联的 ctx 类型
    type Ctx;

    /// 关联的错误类型
    type Error: std::error::Error + Send + Sync + 'static;

    /// 通过 ctx 构造自身 (impl 必须自己实现, 不提供默认)
    fn install(ctx: &Self::Ctx) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// 服务名 (调试用, 跟 plugin 无关)
    fn name(&self) -> &str;
}

/// 插件 trait
pub trait Plugin: Send + Sync + 'static {
    /// 安装到 ctx
    fn install(&self, ctx: &Context) -> anyhow::Result<()>;

    /// 插件名
    fn name(&self) -> &str;

    /// 卸载 (Phase 2 实现, Phase 1 默认 no-op)
    fn uninstall(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_compiles() {
        let _ctx = Context::default();
    }

    #[test]
    fn service_name_via_instance() {
        struct MyService;
        impl Service for MyService {
            type Ctx = Context;
            type Error = anyhow::Error;
            fn install(_: &Context) -> anyhow::Result<Self> {
                Ok(MyService)
            }
            fn name(&self) -> &str { "my_service" }
        }
        let s = MyService;
        assert_eq!(s.name(), "my_service");
    }
}
