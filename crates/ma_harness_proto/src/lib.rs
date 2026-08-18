//! ma_harness_proto — Protobuf 生成代码 + 类型转换
//!
//! **公开 crate** (2026-08-18 锁定). 字段稳定 (Prost 自动生成).
//! 详细 .proto 定义见 `proto/ma_harness/v1/`.
//!
//! # 跟 core 的关系
//!
//! - `ma_harness_core` (内部) 用 `SessionEvent` enum 等强类型
//! - `ma_harness_proto` (公开) 用 Protobuf wire 格式
//! - 两者通过 `From` / `TryFrom` 互转
//!
//! # 用法
//!
//! ```ignore
//! use ma_harness_proto::ma_harness::v1::*;
//!
//! let event = SessionEvent {
//!     id: "...".to_string(),
//!     session_id: "...".to_string(),
//!     event_type: 100, // SessionStart
//!     ts: Some(prost_types::Timestamp::from(SystemTime::now())),
//!     severity: 1, // Info
//!     run_id: None,
//!     plugin_name: None,
//!     payload_json: None,
//!     error_message: None,
//!     model_visible: true,
//! };
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]

/// tonic-build 生成的 Protobuf 代码 (3 个 .proto 的 message + service)
pub mod ma_harness {
    /// proto/ma_harness/v1/agent.proto
    pub mod v1 {
        tonic::include_proto!("ma_harness.v1");
    }
}

// ============================================================================
// 类型转换: core ↔ proto
// ============================================================================

/// proto types 重导出, 方便用户 use 一处
pub use prost_types::Timestamp;
pub use tonic;

/// core ↔ proto 转换 trait
///
/// 用 `From` / `TryFrom` 实现, Phase 1 提供 SessionEvent 转换.
pub mod convert {
    use super::ma_harness::v1;
    use ma_harness_core::{EventType, SessionEvent, Severity};

    use chrono::{DateTime, TimeZone, Utc};

    /// proto::SessionEvent → core::SessionEvent
    ///
    /// # Errors
    ///
    /// - 未知 event_type i32 → Unspecified
    /// - 未知 severity i32 → Info (fallback)
    /// - ts 解析失败 → 用 Utc::now()
    pub fn session_event_from_proto(p: v1::SessionEvent) -> SessionEvent {
        let event_type = EventType::from_i32(p.event_type);
        let severity = match p.severity {
            0 => Severity::Debug,
            1 => Severity::Info,
            2 => Severity::Warn,
            3 => Severity::Error,
            4 => Severity::Fatal,
            _ => Severity::Info,
        };
        let ts = p
            .ts
            .as_ref()
            .and_then(|t| Utc.timestamp_opt(t.seconds, t.nanos as u32).single())
            .unwrap_or_else(Utc::now);

        SessionEvent {
            id: p.id,
            session_id: p.session_id,
            event_type,
            ts,
            severity,
            run_id: p.run_id,
            plugin_name: p.plugin_name,
            payload_json: p.payload_json,
            error_message: p.error_message,
            model_visible: p.model_visible,
        }
    }

    /// core::SessionEvent → proto::SessionEvent
    pub fn session_event_to_proto(s: &SessionEvent) -> v1::SessionEvent {
        v1::SessionEvent {
            id: s.id.clone(),
            session_id: s.session_id.clone(),
            event_type: s.event_type as i32,
            ts: Some(prost_types::Timestamp {
                seconds: s.ts.timestamp(),
                nanos: s.ts.timestamp_subsec_nanos() as i32,
            }),
            severity: match s.severity {
                Severity::Debug => 0,
                Severity::Info => 1,
                Severity::Warn => 2,
                Severity::Error => 3,
                Severity::Fatal => 4,
            },
            run_id: s.run_id.clone(),
            plugin_name: s.plugin_name.clone(),
            payload_json: s.payload_json.clone(),
            error_message: s.error_message.clone(),
            model_visible: s.model_visible,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ma_harness::v1;
    use super::convert::{session_event_from_proto, session_event_to_proto};
    use ma_harness_core::{EventType, Severity};

    #[test]
    fn proto_round_trip_session_event() {
        let original = ma_harness_core::SessionEvent::new("sess-1", EventType::SessionStart)
            .with_payload(&serde_json::json!({"k": "v"}))
            .unwrap();
        let proto = session_event_to_proto(&original);
        let back = session_event_from_proto(proto);
        assert_eq!(back.id, original.id);
        assert_eq!(back.session_id, original.session_id);
        assert_eq!(back.event_type, EventType::SessionStart);
        assert_eq!(back.severity, Severity::Info);
        assert_eq!(back.model_visible, original.model_visible);
        // ts 可能差几纳秒 (nanos <-> timestamp_subsec_nanos 转换), 不严格相等
        let diff = (back.ts - original.ts).num_milliseconds().abs();
        assert!(diff < 1, "ts diff should be < 1ms, got {}ms", diff);
    }

    #[test]
    fn unknown_event_type_defaults_to_unspecified() {
        let mut p = v1::SessionEvent::default();
        p.id = "id-1".to_string();
        p.session_id = "sess-1".to_string();
        p.event_type = 9999; // 不在已知范围
        p.severity = 1;
        p.model_visible = false;
        let converted = session_event_from_proto(p);
        assert_eq!(converted.event_type, EventType::Unspecified);
    }

    #[test]
    fn unknown_severity_defaults_to_info() {
        let mut p = v1::SessionEvent::default();
        p.id = "id-1".to_string();
        p.session_id = "sess-1".to_string();
        p.event_type = 100;
        p.severity = 9999;
        p.model_visible = true;
        let converted = session_event_from_proto(p);
        assert_eq!(converted.severity, Severity::Info);
    }
}
