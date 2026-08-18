//! 错误类型

use thiserror::Error;

/// Cordis 元框架错误
#[derive(Debug, Error)]
pub enum CordisError {
    /// Service 未注册
    #[error("service not found: {0}")]
    ServiceNotFound(&'static str),

    /// Plugin 重复注册
    #[error("plugin already registered: {0}")]
    PluginAlreadyRegistered(String),

    /// Plugin 未注册
    #[error("plugin not found: {0}")]
    PluginNotFound(String),

    /// Ctx key 类型不匹配 (编译期已阻止,这里兜底)
    #[error("ctx key type mismatch: expected {expected}, got {actual}")]
    CtxKeyTypeMismatch {
        /// 期望类型名
        expected: &'static str,
        /// 实际类型名
        actual: &'static str,
    },

    /// Listener 重复订阅同一个 event
    #[error("listener already subscribed: {0}")]
    ListenerAlreadySubscribed(&'static str),

    /// 通用错误 (Phase 1 少用, Phase 2 细分)
    #[error("cordis: {0}")]
    Other(String),
}
