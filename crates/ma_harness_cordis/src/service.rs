//! Service trait
//!
//! 所有可注入到 Context 的服务都实现这个 trait.
//! 用户通常通过 `#[dsh_service]` 宏自动 impl, 见 docs/macro-design.md §2.
//!
//! # 关键点 (见 arch-map §2)
//!
//! - `name(&self)` 返回实例名, 调试用
//! - `install(ctx)` 用户实现, 通过 ctx 构造自身
//! - `Self: Sized` 限制: 不支持 dyn Service (Phase 1 简化, Phase 2 加 trait object 注册)

use crate::Context;

/// Service trait (内部视角)
///
/// 公开 crate 抽象见 `ma_harness_seam::Service`.
/// 两者关系: seam 层的 Service 通常通过 `ctx.inject::<MyService>()` 拿实例,
/// 而 cordis 层是 seam 的实现细节.
pub trait Service: Send + Sync + 'static {
    /// 关联的 ctx 类型 (固定是 `Context`)
    type Ctx = Context;

    /// 关联的错误类型
    type Error: std::error::Error + Send + Sync + 'static;

    /// 通过 ctx 构造自身 (impl 必须自己实现, 不提供默认)
    fn install(ctx: &Self::Ctx) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// 实例名 (调试用)
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MyService {
        _ctx_marker: (),
    }

    impl Service for MyService {
        type Error = anyhow::Error;
        fn install(_ctx: &Context) -> anyhow::Result<Self> {
            Ok(MyService { _ctx_marker: () })
        }
        fn name(&self) -> &str {
            "my_service"
        }
    }

    #[test]
    fn install_returns_instance() {
        let ctx = Context::new();
        let svc = MyService::install(&ctx).unwrap();
        assert_eq!(svc.name(), "my_service");
    }
}
