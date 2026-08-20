//! # 命名约定
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-proto`
//! **Crate ident** (`use` 路径): `ma_harness_proto`
//!
//! Rust 自动从 kebab-case package name 推 snake_case crate ident.
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! ma-harness-proto = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_proto::*;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-proto
//!
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

#![deny(unsafe_code)]
// protoc 生成的 .rs 没 doc comments, 抑制 `missing_docs` lint (rust 1.94+ 默认 warn)
// 整个 crate 都是 generated code (mod ma_harness::v1), 没必要 enforce doc
#![allow(missing_docs)]

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

/// proto types 重导出
pub use prost_types::Timestamp;
pub use tonic;

/// core ↔ proto 转换
pub mod convert {
    use super::ma_harness::v1;
    use ma_harness_core::{EventType, SessionEvent, Severity};

    use chrono::{TimeZone, Utc};

    /// proto::SessionEvent → core::SessionEvent
    pub fn session_event_from_proto(p: v1::SessionEvent) -> SessionEvent {
        let event_type = EventType::from_i32(p.r#type);
        let severity = match p.severity {
            0 => Severity::Debug,
            1 => Severity::Info,
            2 => Severity::Warn,
            3 => Severity::Error,
            4 => Severity::Fatal,
            _ => Severity::Info,
        };
        let ts =
            p.ts.as_ref()
                .and_then(|t| Utc.timestamp_opt(t.seconds, t.nanos as u32).single())
                .unwrap_or_else(Utc::now);

        let opt_str = |s: String| if s.is_empty() { None } else { Some(s) };

        SessionEvent {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: p.session_id,
            event_type,
            ts,
            severity,
            run_id: opt_str(p.run_id),
            plugin_name: opt_str(p.plugin_name),
            payload_json: opt_str(p.payload_json),
            error_message: opt_str(p.error_message),
            model_visible: p.model_visible,
        }
    }

    /// core::SessionEvent → proto::SessionEvent
    pub fn session_event_to_proto(s: &SessionEvent) -> v1::SessionEvent {
        let s_str = |o: &Option<String>| o.clone().unwrap_or_default();

        v1::SessionEvent {
            seq: 0,
            session_id: s.session_id.clone(),
            r#type: s.event_type as i32,
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
            run_id: s_str(&s.run_id),
            plugin_name: s_str(&s.plugin_name),
            payload_json: s_str(&s.payload_json),
            error_message: s_str(&s.error_message),
            model_visible: s.model_visible,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::convert::{session_event_from_proto, session_event_to_proto};
    use ma_harness_core::{EventType, Severity};

    #[test]
    fn proto_round_trip_session_event() {
        let original = ma_harness_core::SessionEvent::new("sess-1", EventType::SessionStart)
            .with_payload(&serde_json::json!({"k": "v"}))
            .unwrap();
        let proto = session_event_to_proto(&original);
        let back = session_event_from_proto(proto);
        assert_eq!(back.session_id, original.session_id);
        assert_eq!(back.event_type, EventType::SessionStart);
        assert_eq!(back.severity, Severity::Info);
        assert_eq!(back.model_visible, original.model_visible);
    }
}
