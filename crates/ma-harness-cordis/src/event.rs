//! 事件类型 (Phase 1 Day 5 才完整实现 listener, 这里先定义 event 枚举)

/// 事件严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventSeverity {
    /// 调试信息
    Debug,
    /// 普通信息
    Info,
    /// 警告
    Warn,
    /// 错误
    Error,
    /// 致命 (进程即将退出)
    Fatal,
}

impl EventSeverity {
    /// 转字符串 (logging 用)
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }
}

/// 事件 (Phase 1 Day 5 跟 arch-map §4 SessionEvent 配齐)
///
/// Week 1 阶段, 仅占位. Week 2 跟 `ma_harness_core` 的 SessionEvent 对齐.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// 占位: ctx 已创建
    ContextCreated,
    /// 占位: plugin 装载
    PluginInstalled,
    /// 占位: service 注入
    ServiceInjected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_as_str() {
        assert_eq!(EventSeverity::Info.as_str(), "INFO");
        assert_eq!(EventSeverity::Error.as_str(), "ERROR");
    }
}
