//! Cordis 错误类型
//!
//! 公开 `CordisError` (ctx 内部错误) + `BoxedError` (Service::Error 的 box newtype).
//!
//! # BoxedError 设计动机
//!
//! 2026-08-18: 为什么不能直接用 `Box<dyn StdError + Send + Sync>` 当 Service::Error?
//!
//! rust 标准库 `impl<T: StdError + ?Sized> StdError for Box<T>` 允许 unsized inner,
//! 但 `?` 操作符 + `Result<T, E>` 隐含 `E: Sized` (因为 `Ok`/`Err` 存 E 到内存).
//! 实际编译时会报 `the size for values of type dyn StdError cannot be known`.
//!
//! 修法: newtype wrapper `BoxedError(Box<dyn StdError + Send + Sync>)` — outer struct 是 sized,
//! 手动 impl `StdError` (转发到 inner), 就能用作 Service::Error, 且 `?` 转换无障碍.
//!
//! # 转换
//!
//! ```ignore
//! use ma_harness_cordis::BoxedError;
//!
//! // 从任何 StdError 构造 (通过 blanket From)
//! let e: BoxedError = BashError::NotFound.into();
//! let e: BoxedError = anyhow::Error::msg("oops").into();
//! ```

use std::fmt;

/// 内部 ctx 错误 (Service 装载 / Plugin 装载 / service not found 等场景)
#[derive(Debug, thiserror::Error)]
pub enum CordisError {
    /// service 没注册到 ctx
    #[error("service not found: {0}")]
    ServiceNotFound(&'static str),
    /// plugin 没注册到 ctx
    #[error("plugin not found: {0}")]
    PluginNotFound(String),
    /// plugin 已经注册, 不能重复
    #[error("plugin already registered: {0}")]
    PluginAlreadyRegistered(String),
    /// 通用错误 (从字符串转换)
    #[error("{0}")]
    Other(String),
    /// listener 已经被 dispose (不能 emit)
    #[error("listener registry disposed")]
    ListenerDisposed,
    /// key 跟已注册的 key 类型冲突
    #[error("key type mismatch: expected {expected}, got {actual}")]
    KeyTypeMismatch {
        /// 期望类型
        expected: &'static str,
        /// 实际类型
        actual: &'static str,
    },
    /// emit 递归调用 (listener 里又 emit)
    #[error("re-entrant emit detected (listener triggered another emit on same ctx)")]
    ReentrantEmit,
}

/// Service / Plugin 用的 boxed error newtype
///
/// 用作 `type Error = BoxedError;` 当 Service trait 的关联类型.
/// 内部包一个 `Box<dyn StdError + Send + Sync>`, 通过 `From<E>` 自动转换.
pub struct BoxedError(pub Box<dyn std::error::Error + Send + Sync>);

impl BoxedError {
    /// 构造一个 boxed error (从任何 Error 类型)
    pub fn new<E: std::error::Error + Send + Sync + 'static>(e: E) -> Self {
        Self(Box::new(e))
    }

    /// 借用内部 error
    pub fn as_ref(&self) -> &(dyn std::error::Error + Send + Sync) {
        &*self.0
    }

    /// 拆出 inner Box
    pub fn into_inner(self) -> Box<dyn std::error::Error + Send + Sync> {
        self.0
    }
}

impl fmt::Debug for BoxedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("BoxedError").field(&self.0).finish()
    }
}

impl fmt::Display for BoxedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BoxedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}

// 注意: 2026-08-18 不加 blanket `From<E: StdError + Send + Sync + 'static> for BoxedError`!
// 原因: BoxedError impl StdError, blanket 覆盖 E=BoxedError, 跟 std blanket
// `impl<T> From<T> for T` 冲突 (rustc E0119 "conflicting implementations").
//
// 用法: 显式 `BoxedError::new(e)` 构造, 或 `Box<dyn StdError + Send + Sync>::from(e) into()`.
// 转换是单向的 (外->BoxedError 通过 Box::new), 反向 (BoxedError->外) 走 `?` 操作符
// (因为 BoxedError impl StdError, anyhow::Error 等都有 `From<E: StdError> From` impl).

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct DummyErr(&'static str);
    impl fmt::Display for DummyErr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }
    impl std::error::Error for DummyErr {}

    #[test]
    fn boxed_error_from_concrete() {
        let e = BoxedError::new(DummyErr("oops"));
        assert_eq!(e.to_string(), "oops");
    }

    #[test]
    fn boxed_error_debug_display() {
        let e = BoxedError::new(DummyErr("hi"));
        assert!(format!("{:?}", e).contains("BoxedError"));
        assert_eq!(e.to_string(), "hi");
    }

    #[test]
    fn boxed_error_source() {
        let e = BoxedError::new(DummyErr("root"));
        // BoxedError::source 转发到 inner
        let src = std::error::Error::source(&e).unwrap();
        assert_eq!(src.to_string(), "root");
    }

    #[test]
    fn boxed_error_from_boxed_inner() {
        let inner: Box<dyn std::error::Error + Send + Sync> = Box::new(DummyErr("inner"));
        let e = BoxedError(inner);
        assert_eq!(e.to_string(), "inner");
    }

    #[test]
    fn cordis_error_service_not_found() {
        let e = CordisError::ServiceNotFound("hello");
        assert!(e.to_string().contains("hello"));
    }

    #[test]
    fn cordis_error_plugin_not_found() {
        let e = CordisError::PluginNotFound("bash".to_string());
        assert!(e.to_string().contains("bash"));
    }

    #[test]
    fn cordis_error_plugin_already_registered() {
        let e = CordisError::PluginAlreadyRegistered("bash".to_string());
        assert!(e.to_string().contains("bash"));
    }
}
