//! SessionEvent — append-only 日志的事件类型
//!
//! Week 1 Day 6 实现. 完整设计见 `docs/ma-harness-arch-map.md` §4 +
//! `proto/ma_harness/v1/event.proto`.
//!
//! **关键不变量 (model-visible means logged)**: 任何 model context 里能看到的字符串,
//! 都必须能在 SessionEvent 日志里查到对应事件. 落库失败 → panic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// 事件严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    /// 调试
    Debug,
    /// 普通
    Info,
    /// 警告
    Warn,
    /// 错误
    Error,
    /// 致命
    Fatal,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        })
    }
}

/// 事件类型分类 (段位编号, 跟 proto 对齐)
///
/// Phase 1 范围: 200/300/400/600 段位 + 100/700 段位占位
/// Phase 2 加: 500/800/900 段位
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum EventType {
    /// 占位/未指定
    Unspecified = 0,

    // Session 生命周期 (100 段位)
    SessionStart = 100,
    SessionEnd = 101,

    // Agent run (200 段位)
    RunStart = 200,
    RunEnd = 201,

    // Model 调用 (300 段位)
    ModelRequest = 300,
    ModelResponse = 301,
    ModelError = 302,

    // Tool 调用 (400 段位)
    ToolCall = 400,
    ToolResult = 401,
    ToolError = 402,

    // User 主动 (600 段位)
    UserInput = 600,
    UserCancel = 601,

    // Sandbox (700 段位)
    SandboxViolation = 700,
    SandboxConfig = 701,

    // 2026-08-19 (Day 101 / P7-2.6): 审批审计 (800 段位)
    // model_visible = false (内部审计, 不上 model context)
    // payload: ApprovalRequest / ApprovalDecision (含 who/when/decision/reason)
    ApprovalRequest = 800,
    ApprovalDecision = 801,
}

impl EventType {
    /// i32 ↔ EventType 双向转换
    pub fn from_i32(v: i32) -> Self {
        match v {
            100 => Self::SessionStart,
            101 => Self::SessionEnd,
            200 => Self::RunStart,
            201 => Self::RunEnd,
            300 => Self::ModelRequest,
            301 => Self::ModelResponse,
            302 => Self::ModelError,
            400 => Self::ToolCall,
            401 => Self::ToolResult,
            402 => Self::ToolError,
            600 => Self::UserInput,
            601 => Self::UserCancel,
            700 => Self::SandboxViolation,
            701 => Self::SandboxConfig,
            800 => Self::ApprovalRequest,
            801 => Self::ApprovalDecision,
            _ => Self::Unspecified,
        }
    }

    /// 决定这个事件是不是 model 可见的
    ///
    /// Phase 1 规则 (跟 arch-map §4 对齐):
    /// - User* / SessionStart/End / Run* / Model* / Tool* / SandboxViolation → true
    /// - ModelError / SandboxConfig → false (内部)
    pub fn model_visible(&self) -> bool {
        match self {
            // model 看得到
            Self::SessionStart
            | Self::SessionEnd
            | Self::RunStart
            | Self::RunEnd
            | Self::ModelRequest
            | Self::ModelResponse
            | Self::ToolCall
            | Self::ToolResult
            | Self::ToolError
            | Self::UserInput
            | Self::UserCancel
            | Self::SandboxViolation => true,
            // 内部 (不上 model context)
            Self::ModelError
            | Self::SandboxConfig
            | Self::ApprovalRequest
            | Self::ApprovalDecision
            | Self::Unspecified => false,
        }
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unspecified => "UNSPECIFIED",
            Self::SessionStart => "SESSION_START",
            Self::SessionEnd => "SESSION_END",
            Self::RunStart => "RUN_START",
            Self::RunEnd => "RUN_END",
            Self::ModelRequest => "MODEL_REQUEST",
            Self::ModelResponse => "MODEL_RESPONSE",
            Self::ModelError => "MODEL_ERROR",
            Self::ToolCall => "TOOL_CALL",
            Self::ToolResult => "TOOL_RESULT",
            Self::ToolError => "TOOL_ERROR",
            Self::UserInput => "USER_INPUT",
            Self::UserCancel => "USER_CANCEL",
            Self::SandboxViolation => "SANDBOX_VIOLATION",
            Self::SandboxConfig => "SANDBOX_CONFIG",
            Self::ApprovalRequest => "APPROVAL_REQUEST",
            Self::ApprovalDecision => "APPROVAL_DECISION",
        })
    }
}

/// SessionEvent 主消息
///
/// 字段对齐 `proto/ma_harness/v1/event.proto` 的 `SessionEvent` message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// 业务 ID (UUID v4)
    pub id: String,
    /// 所属 session
    pub session_id: String,
    /// 事件类型
    pub event_type: EventType,
    /// 时间戳
    pub ts: DateTime<Utc>,
    /// 严重级别
    pub severity: Severity,
    /// 所属 run (如适用)
    pub run_id: Option<String>,
    /// 触发该事件的插件
    pub plugin_name: Option<String>,
    /// payload (JSON 字符串, 业务方自定义)
    pub payload_json: Option<String>,
    /// 错误信息 (severity >= Error 时)
    pub error_message: Option<String>,
    /// model-visible 标记 (写时校验, 跟 event_type 一致)
    pub model_visible: bool,
}

impl SessionEvent {
    /// 构造一个新事件 (auto-fill id + ts)
    pub fn new(session_id: impl Into<String>, event_type: EventType) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            event_type,
            ts: Utc::now(),
            severity: Severity::Info,
            run_id: None,
            plugin_name: None,
            payload_json: None,
            error_message: None,
            // **不变量**: 写死 event_type.model_visible(), 防止误标
            model_visible: event_type.model_visible(),
        }
    }

    /// builder: severity
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// builder: run_id
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// builder: plugin_name
    pub fn with_plugin(mut self, plugin: impl Into<String>) -> Self {
        self.plugin_name = Some(plugin.into());
        self
    }

    /// builder: payload (任何 Serialize 类型)
    pub fn with_payload<T: Serialize>(mut self, payload: &T) -> Result<Self, serde_json::Error> {
        self.payload_json = Some(serde_json::to_string(payload)?);
        Ok(self)
    }

    /// builder: error_message
    pub fn with_error(mut self, msg: impl Into<String>) -> Self {
        self.error_message = Some(msg.into());
        // 错误事件自动升级 severity (除非用户已设)
        if matches!(self.severity, Severity::Info) {
            self.severity = Severity::Error;
        }
        self
    }

    /// **不变量校验**: 写时调, 失败 panic
    ///
    /// Phase 1 规则:
    /// 1. model_visible == true 时 payload_json 必须非空 (没有空 payload 给 model 看)
    /// 2. severity >= Error 时 error_message 必须非空
    /// 3. model_visible 必须跟 event_type.model_visible() 一致 (防止误标)
    pub fn validate(&self) -> Result<(), String> {
        if self.model_visible && self.payload_json.as_deref().map_or(true, str::is_empty) {
            return Err(format!(
                "model_visible event {} ({}) 必须有非空 payload_json",
                self.event_type, self.id
            ));
        }
        if matches!(self.severity, Severity::Error | Severity::Fatal)
            && self.error_message.as_deref().map_or(true, str::is_empty)
        {
            return Err(format!(
                "severity={} 事件 {} ({}) 必须有非空 error_message",
                self.severity, self.event_type, self.id
            ));
        }
        if self.model_visible != self.event_type.model_visible() {
            return Err(format!(
                "model_visible 标志 ({}) 跟 event_type.model_visible() ({}) 不一致 (type={})",
                self.model_visible,
                self.event_type.model_visible(),
                self.event_type
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_model_visible_classification() {
        assert!(EventType::SessionStart.model_visible());
        assert!(EventType::RunStart.model_visible());
        assert!(EventType::ModelRequest.model_visible());
        assert!(EventType::ToolCall.model_visible());
        assert!(!EventType::ModelError.model_visible());
        assert!(!EventType::SandboxConfig.model_visible());
    }

    #[test]
    fn event_type_round_trip() {
        for v in [
            0, 100, 200, 300, 400, 600, 700, 999, // 999 走 default Unspecified
        ] {
            let t = EventType::from_i32(v);
            let back = t as i32;
            if v == 0 || v == 999 {
                assert_eq!(back, 0, "out-of-range 应回 Unspecified");
            } else {
                assert_eq!(back, v);
            }
        }
    }

    #[test]
    fn new_event_auto_fills_id_ts_model_visible() {
        let e = SessionEvent::new("sess-1", EventType::SessionStart);
        assert!(!e.id.is_empty());
        assert_eq!(e.session_id, "sess-1");
        assert_eq!(e.event_type, EventType::SessionStart);
        assert!(e.model_visible, "SessionStart 应是 model_visible");
    }

    #[test]
    fn validate_rejects_empty_payload_for_model_visible() {
        let mut e = SessionEvent::new("s", EventType::SessionStart);
        e.model_visible = true;
        e.payload_json = None;
        let result = e.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("payload_json"));
    }

    #[test]
    fn validate_rejects_missing_error_message() {
        let mut e = SessionEvent::new("s", EventType::ModelError);
        e.model_visible = false; // ModelError 不是 model-visible
        e.severity = Severity::Error;
        e.error_message = None;
        let result = e.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("error_message"));
    }

    #[test]
    fn validate_rejects_mismatched_model_visible_flag() {
        let mut e = SessionEvent::new("s", EventType::SessionStart);
        e.model_visible = false; // 跟 event_type.model_visible() 不一致
        e.payload_json = Some("{}".to_string());
        let result = e.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("model_visible 标志"));
    }

    #[test]
    fn with_payload_serializes_struct() {
        #[derive(Serialize)]
        struct Prompt {
            text: String,
        }
        let e = SessionEvent::new("s", EventType::ModelRequest)
            .with_payload(&Prompt { text: "hi".into() })
            .unwrap();
        let payload = e.payload_json.unwrap();
        assert!(payload.contains("hi"));
    }

    #[test]
    fn with_error_upgrades_severity() {
        let e = SessionEvent::new("s", EventType::ModelError).with_error("boom");
        assert_eq!(e.severity, Severity::Error);
        assert_eq!(e.error_message.as_deref(), Some("boom"));
    }

    #[test]
    fn full_valid_event_passes_validate() {
        let e = SessionEvent::new("s", EventType::ModelRequest)
            .with_payload(&serde_json::json!({"model": "gpt-4o"}))
            .unwrap();
        assert!(e.validate().is_ok());
    }
}
