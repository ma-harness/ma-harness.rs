//! FixtureEvent ↔ SessionEvent 转换层.
//!
//! 设计: 见 `docs/conformance-design.md` § 4.
//!
//! **重要**: 双向转换是 lossless 的 (payload 完整保留).
//! 唯一丢的是 `id` (UUID, 每次重放新生成) 和 `ts` (时间戳, 不可重现).
//! `run_id` / `plugin_name` / `severity` 也都保留 (从 payload 或 context 推导).

use crate::fixture::FixtureEvent;
use ma_harness_core::event::{EventType, SessionEvent, Severity};
use serde_json::Value;
use thiserror::Error;

/// 事件类型字符串 ↔ EventType enum 映射
pub fn event_type_from_str(s: &str) -> Option<EventType> {
    match s {
        "SessionStart" => Some(EventType::SessionStart),
        "SessionEnd" => Some(EventType::SessionEnd),
        "RunStart" => Some(EventType::RunStart),
        "RunEnd" => Some(EventType::RunEnd),
        "ModelRequest" => Some(EventType::ModelRequest),
        "ModelResponse" => Some(EventType::ModelResponse),
        "ModelError" => Some(EventType::ModelError),
        "ToolCall" => Some(EventType::ToolCall),
        "ToolResult" => Some(EventType::ToolResult),
        "ToolError" => Some(EventType::ToolError),
        "UserInput" => Some(EventType::UserInput),
        "UserCancel" => Some(EventType::UserCancel),
        "SandboxViolation" => Some(EventType::SandboxViolation),
        "SandboxConfig" => Some(EventType::SandboxConfig),
        "ApprovalRequest" => Some(EventType::ApprovalRequest),
        "ApprovalDecision" => Some(EventType::ApprovalDecision),
        _ => None,
    }
}

/// EventType → 字符串
pub fn event_type_to_str(t: EventType) -> &'static str {
    match t {
        EventType::Unspecified => "Unspecified",
        EventType::SessionStart => "SessionStart",
        EventType::SessionEnd => "SessionEnd",
        EventType::RunStart => "RunStart",
        EventType::RunEnd => "RunEnd",
        EventType::ModelRequest => "ModelRequest",
        EventType::ModelResponse => "ModelResponse",
        EventType::ModelError => "ModelError",
        EventType::ToolCall => "ToolCall",
        EventType::ToolResult => "ToolResult",
        EventType::ToolError => "ToolError",
        EventType::UserInput => "UserInput",
        EventType::UserCancel => "UserCancel",
        EventType::SandboxViolation => "SandboxViolation",
        EventType::SandboxConfig => "SandboxConfig",
        EventType::ApprovalRequest => "ApprovalRequest",
        EventType::ApprovalDecision => "ApprovalDecision",
    }
}

/// FixtureEvent → SessionEvent (用于写 EventLog)
pub fn fixture_to_session(
    session_id: &str,
    fixture_event: &FixtureEvent,
) -> Result<SessionEvent, ConvertError> {
    let event_type = event_type_from_str(&fixture_event.event_type)
        .ok_or_else(|| ConvertError::UnknownEventType(fixture_event.event_type.clone()))?;

    let mut event = SessionEvent::new(session_id, event_type);

    // severity 推断: payload 含 "severity" 字段就用, 否则 Info
    if let Some(sev) = fixture_event.payload.get("severity").and_then(|v| v.as_str()) {
        event = match sev {
            "Debug" => event.with_severity(Severity::Debug),
            "Info" => event.with_severity(Severity::Info),
            "Warn" => event.with_severity(Severity::Warn),
            "Error" => event.with_severity(Severity::Error),
            "Fatal" => event.with_severity(Severity::Fatal),
            _ => event,
        };
    }

    // run_id 从 payload 推导
    if let Some(rid) = fixture_event.payload.get("run_id").and_then(|v| v.as_str()) {
        event = event.with_run_id(rid);
    }

    // plugin_name 从 payload 推导
    if let Some(pn) = fixture_event.payload.get("plugin_name").and_then(|v| v.as_str()) {
        event = event.with_plugin(pn);
    }

    // error_message 从 payload 推导
    if let Some(err) = fixture_event.payload.get("error_message").and_then(|v| v.as_str()) {
        event = event.with_error(err);
    }

    // 整体 payload 作为 payload_json
    // 注意: run_id / plugin_name / error_message 已经在 builder 里抽出来,
    // 这里 payload 仍然塞原值 (业务方要查时能找到)
    event = event
        .with_payload(&fixture_event.payload)
        .map_err(ConvertError::Serialize)?;

    Ok(event)
}

/// SessionEvent → FixtureEvent (用于 EventLog 读回 + 比对)
pub fn session_to_fixture(event: &SessionEvent) -> FixtureEvent {
    let payload: Value = event
        .payload_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);

    FixtureEvent {
        event_type: event_type_to_str(event.event_type).to_string(),
        payload,
        timestamp_ms: Some(event.ts.timestamp_millis() as u64),
    }
}

/// Convert 错误
#[derive(Debug, Error)]
pub enum ConvertError {
    /// 未知的 event_type 字符串
    #[error("unknown event type: {0}")]
    UnknownEventType(String),

    /// payload 序列化失败
    #[error("payload serialize error: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::FixtureEvent;

    #[test]
    fn event_type_roundtrip() {
        let names = [
            "SessionStart", "SessionEnd", "RunStart", "RunEnd", "ModelRequest", "ModelResponse",
            "ModelError", "ToolCall", "ToolResult", "ToolError", "UserInput", "UserCancel",
            "SandboxViolation", "SandboxConfig",
        ];
        for name in names {
            let t = event_type_from_str(name).unwrap_or_else(|| panic!("missing: {name}"));
            assert_eq!(event_type_to_str(t), name);
        }
    }

    #[test]
    fn event_type_from_str_unknown_returns_none() {
        assert!(event_type_from_str("NotAType").is_none());
        assert!(event_type_from_str("").is_none());
    }

    #[test]
    fn fixture_to_session_basic() {
        let fe = FixtureEvent {
            event_type: "ToolCall".to_string(),
            payload: serde_json::json!({"tool": "bash", "args": {"command": "echo hi"}}),
            timestamp_ms: None,
        };
        let se = fixture_to_session("s1", &fe).unwrap();
        assert_eq!(se.event_type, EventType::ToolCall);
        assert_eq!(se.session_id, "s1");
        assert!(se.model_visible); // ToolCall is model-visible
    }

    #[test]
    fn fixture_to_session_with_severity() {
        let fe = FixtureEvent {
            event_type: "ModelError".to_string(),
            payload: serde_json::json!({
                "severity": "Error",
                "error_message": "timeout",
            }),
            timestamp_ms: None,
        };
        let se = fixture_to_session("s1", &fe).unwrap();
        assert_eq!(se.severity, Severity::Error);
        assert_eq!(se.error_message.as_deref(), Some("timeout"));
    }

    #[test]
    fn fixture_to_session_with_run_id() {
        let fe = FixtureEvent {
            event_type: "RunStart".to_string(),
            payload: serde_json::json!({"run_id": "r-001", "model": "stub"}),
            timestamp_ms: None,
        };
        let se = fixture_to_session("s1", &fe).unwrap();
        assert_eq!(se.run_id.as_deref(), Some("r-001"));
    }

    #[test]
    fn fixture_to_session_unknown_type_errors() {
        let fe = FixtureEvent {
            event_type: "Garbage".to_string(),
            payload: serde_json::json!({}),
            timestamp_ms: None,
        };
        let r = fixture_to_session("s1", &fe);
        assert!(matches!(r, Err(ConvertError::UnknownEventType(_))));
    }

    #[test]
    fn session_to_fixture_roundtrip() {
        let fe = FixtureEvent {
            event_type: "RunStart".to_string(),
            payload: serde_json::json!({"model": "stub", "temperature": 0.7}),
            timestamp_ms: None,
        };
        let se = fixture_to_session("sess", &fe).unwrap();
        let fe2 = session_to_fixture(&se);
        assert_eq!(fe2.event_type, "RunStart");
        assert_eq!(fe2.payload, fe.payload);
    }

    #[test]
    fn session_to_fixture_empty_payload() {
        // SessionEvent 无 payload (None)
        let se = SessionEvent::new("s1", EventType::RunStart);
        let fe = session_to_fixture(&se);
        assert_eq!(fe.event_type, "RunStart");
        assert_eq!(fe.payload, Value::Null);
    }
}
