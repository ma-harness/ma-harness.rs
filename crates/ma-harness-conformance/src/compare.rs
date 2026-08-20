//! Compare engine: 比对实际事件序列 vs 期望事件序列, 产出 diff。
//!
//! 算法见 `docs/conformance-design.md` § 5。

use crate::fixture::{ExpectedEvent, FixtureEvent};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 一个 diff 单元 (一个事件或一个字段的不匹配)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Diff {
    /// 实际比期望多一个事件
    ExtraEvent {
        /// 实际事件 index
        index: usize,
        /// 实际事件类型
        actual_type: String,
    },
    /// 实际比期望少一个事件
    MissingEvent {
        /// 期望事件 index
        index: usize,
        /// 期望事件类型
        expected_type: String,
    },
    /// 事件类型不匹配
    TypeMismatch {
        /// 事件 index
        index: usize,
        /// 期望类型
        expected_type: String,
        /// 实际类型
        actual_type: String,
    },
    /// payload 缺字段
    MissingField {
        /// 事件 index
        index: usize,
        /// 缺哪个字段
        key: String,
    },
    /// payload 字段值不匹配
    FieldMismatch {
        /// 事件 index
        index: usize,
        /// 哪个字段
        key: String,
        /// 期望值
        expected: serde_json::Value,
        /// 实际值
        actual: serde_json::Value,
    },
}

impl Diff {
    /// diff 简述 (用于报告)
    pub fn summary(&self) -> String {
        match self {
            Self::ExtraEvent { index, actual_type } => {
                format!("[#{index}] extra event: {actual_type}")
            }
            Self::MissingEvent {
                index,
                expected_type,
            } => {
                format!("[#{index}] missing event: {expected_type}")
            }
            Self::TypeMismatch {
                index,
                expected_type,
                actual_type,
            } => {
                format!("[#{index}] type mismatch: expected={expected_type}, actual={actual_type}")
            }
            Self::MissingField { index, key } => {
                format!("[#{index}] missing field: {key}")
            }
            Self::FieldMismatch {
                index,
                key,
                expected,
                actual,
            } => {
                format!("[#{index}] field {key}: expected={expected}, actual={actual}")
            }
        }
    }
}

/// 单次 fixture 比对结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareResult {
    /// 通过 (无 diff)
    pub passed: bool,
    /// 所有 diff (空 = 通过)
    pub diffs: Vec<Diff>,
    /// 实际事件数
    pub actual_count: usize,
    /// 期望事件数
    pub expected_count: usize,
}

impl CompareResult {
    /// 全过 (passed=true, diffs=空)
    pub fn ok(actual_count: usize, expected_count: usize) -> Self {
        Self {
            passed: true,
            diffs: Vec::new(),
            actual_count,
            expected_count,
        }
    }

    /// 失败 (有 diff)
    pub fn failed(diffs: Vec<Diff>, actual_count: usize, expected_count: usize) -> Self {
        Self {
            passed: false,
            diffs,
            actual_count,
            expected_count,
        }
    }
}

/// Compare 引擎。
pub struct CompareEngine;

impl CompareEngine {
    /// 比对实际事件 vs 期望事件。
    ///
    /// 算法: 按 index 配对, 每个事件比:
    /// 1. event_type 必须相等
    /// 2. payload_match 的每个 key 必须在 actual.payload 存在 + 值相等
    pub fn compare(actual: &[FixtureEvent], expected: &[ExpectedEvent]) -> CompareResult {
        let mut diffs = Vec::new();
        let n = actual.len().max(expected.len());

        for i in 0..n {
            match (actual.get(i), expected.get(i)) {
                (Some(a), Some(e)) => {
                    if a.event_type != e.event_type {
                        diffs.push(Diff::TypeMismatch {
                            index: i,
                            expected_type: e.event_type.clone(),
                            actual_type: a.event_type.clone(),
                        });
                        // 类型不匹配, 跳过 payload 比对 (避免噪音)
                        continue;
                    }
                    // payload 浅比对
                    for (key, expected_value) in &e.payload_match {
                        let actual_value = a.payload.get(key);
                        match actual_value {
                            None => diffs.push(Diff::MissingField {
                                index: i,
                                key: key.clone(),
                            }),
                            Some(v) if v != expected_value => diffs.push(Diff::FieldMismatch {
                                index: i,
                                key: key.clone(),
                                expected: expected_value.clone(),
                                actual: v.clone(),
                            }),
                            _ => {} // 相等
                        }
                    }
                }
                (Some(a), None) => {
                    diffs.push(Diff::ExtraEvent {
                        index: i,
                        actual_type: a.event_type.clone(),
                    });
                }
                (None, Some(e)) => {
                    diffs.push(Diff::MissingEvent {
                        index: i,
                        expected_type: e.event_type.clone(),
                    });
                }
                (None, None) => break,
            }
        }

        if diffs.is_empty() {
            CompareResult::ok(actual.len(), expected.len())
        } else {
            CompareResult::failed(diffs, actual.len(), expected.len())
        }
    }
}

/// Compare 错误。
#[derive(Debug, Error)]
pub enum CompareError {
    /// 输入维度不合法
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(ty: &str, payload: serde_json::Value) -> FixtureEvent {
        FixtureEvent {
            event_type: ty.to_string(),
            payload,
            timestamp_ms: None,
        }
    }

    fn exp(
        ty: &str,
        payload_match: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> ExpectedEvent {
        ExpectedEvent {
            event_type: ty.to_string(),
            payload_match,
            timestamp_ms: None,
        }
    }

    #[test]
    fn compare_passes_when_exact_match() {
        let actual = vec![
            ev("RunStart", json!({"prompt": "hi"})),
            ev("RunEnd", json!({"status": "ok"})),
        ];
        let mut m1 = std::collections::BTreeMap::new();
        m1.insert("prompt".to_string(), json!("hi"));
        let mut m2 = std::collections::BTreeMap::new();
        m2.insert("status".to_string(), json!("ok"));
        let expected = vec![exp("RunStart", m1), exp("RunEnd", m2)];

        let r = CompareEngine::compare(&actual, &expected);
        assert!(r.passed, "diffs = {:?}", r.diffs);
        assert_eq!(r.actual_count, 2);
        assert_eq!(r.expected_count, 2);
    }

    #[test]
    fn compare_fails_on_extra_event() {
        let actual = vec![ev("RunStart", json!({})), ev("RunEnd", json!({}))];
        let expected = vec![exp("RunStart", std::collections::BTreeMap::new())];
        let r = CompareEngine::compare(&actual, &expected);
        assert!(!r.passed);
        assert_eq!(r.diffs.len(), 1);
        assert!(matches!(r.diffs[0], Diff::ExtraEvent { index: 1, .. }));
    }

    #[test]
    fn compare_fails_on_missing_event() {
        let actual = vec![ev("RunStart", json!({}))];
        let expected = vec![
            exp("RunStart", std::collections::BTreeMap::new()),
            exp("RunEnd", std::collections::BTreeMap::new()),
        ];
        let r = CompareEngine::compare(&actual, &expected);
        assert!(!r.passed);
        assert_eq!(r.diffs.len(), 1);
        assert!(matches!(r.diffs[0], Diff::MissingEvent { index: 1, .. }));
    }

    #[test]
    fn compare_fails_on_type_mismatch() {
        let actual = vec![ev("ModelResponse", json!({}))];
        let expected = vec![exp("ModelRequest", std::collections::BTreeMap::new())];
        let r = CompareEngine::compare(&actual, &expected);
        assert!(!r.passed);
        assert_eq!(r.diffs.len(), 1);
        assert!(matches!(r.diffs[0], Diff::TypeMismatch { .. }));
    }

    #[test]
    fn compare_fails_on_field_mismatch() {
        let actual = vec![ev("RunStart", json!({"prompt": "hi"}))];
        let mut m = std::collections::BTreeMap::new();
        m.insert("prompt".to_string(), json!("bye"));
        let expected = vec![exp("RunStart", m)];
        let r = CompareEngine::compare(&actual, &expected);
        assert!(!r.passed);
        assert_eq!(r.diffs.len(), 1);
        assert!(matches!(r.diffs[0], Diff::FieldMismatch { .. }));
    }

    #[test]
    fn compare_fails_on_missing_field() {
        let actual = vec![ev("RunStart", json!({}))];
        let mut m = std::collections::BTreeMap::new();
        m.insert("prompt".to_string(), json!("hi"));
        let expected = vec![exp("RunStart", m)];
        let r = CompareEngine::compare(&actual, &expected);
        assert!(!r.passed);
        assert_eq!(r.diffs.len(), 1);
        assert!(matches!(r.diffs[0], Diff::MissingField { .. }));
    }

    #[test]
    fn compare_ignores_extra_actual_fields() {
        // payload_match 只比关心的字段, actual 多塞字段 OK
        let actual = vec![ev("RunStart", json!({"prompt": "hi", "trace_id": "abc"}))];
        let mut m = std::collections::BTreeMap::new();
        m.insert("prompt".to_string(), json!("hi"));
        let expected = vec![exp("RunStart", m)];
        let r = CompareEngine::compare(&actual, &expected);
        assert!(r.passed, "diffs = {:?}", r.diffs);
    }

    #[test]
    fn diff_summary_strings() {
        let d = Diff::TypeMismatch {
            index: 2,
            expected_type: "ModelRequest".to_string(),
            actual_type: "ModelResponse".to_string(),
        };
        assert!(d.summary().contains("ModelRequest"));
        assert!(d.summary().contains("ModelResponse"));
    }
}
