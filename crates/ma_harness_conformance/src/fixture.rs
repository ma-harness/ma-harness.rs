//! Fixture schema + JSONL 加载器。
//!
//! 格式见 `docs/conformance-design.md` § 3。

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use thiserror::Error;

/// 一个 fixture = 一个测试场景。
///
/// 描述: 给定 input 事件序列, 期望 output 事件序列。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fixture {
    /// Fixture 唯一名 (用于报告)
    pub name: String,
    /// Fixture 分类
    pub category: FixtureCategory,
    /// 人类可读描述 (可选)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 输入 (要 replay 的事件序列)
    pub input: FixtureInput,
    /// 期望输出 (要被比对的)
    pub output: FixtureOutput,
}

/// Fixture 分类。
///
/// 选 enum 是为了报告聚合 (按 category 看 pass rate)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureCategory {
    /// 单个 tool call + result
    ToolCall,
    /// Agent 完整跑一轮 (含 model request)
    AgentRun,
    /// Session 创建/恢复/销毁
    SessionLifecycle,
    /// 事件顺序 + 计数
    EventOrdering,
    /// 错误路径 (tool 失败 / plugin 拒绝 / 等)
    ErrorPath,
}

impl FixtureCategory {
    /// 字符串表示 (snake_case)
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ToolCall => "tool_call",
            Self::AgentRun => "agent_run",
            Self::SessionLifecycle => "session_lifecycle",
            Self::EventOrdering => "event_ordering",
            Self::ErrorPath => "error_path",
        }
    }
}

/// Fixture 输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureInput {
    /// 任意 session id, 用于日志关联
    pub session_id: String,
    /// 装载哪些 plugin (按 plugin name)
    #[serde(default)]
    pub plugins: Vec<String>,
    /// 输入事件序列 (按时间顺序)
    pub events: Vec<FixtureEvent>,
}

/// Fixture 期望输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureOutput {
    /// 期望事件序列 (按时间顺序, 浅比对)
    pub events: Vec<ExpectedEvent>,
    /// 期望最终状态 (key-value, 跑完后 ctx 状态)
    #[serde(default)]
    pub final_state: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Fixture 中的事件 (输入用)。
///
/// 跟 ma_harness_core::event::SessionEvent 1:1, 但用 JSON Value 保留灵活性。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureEvent {
    /// 事件类型字符串 (e.g. "RunStart", "ToolCall", "ModelResponse")
    #[serde(rename = "type")]
    pub event_type: String,
    /// 事件 payload (任意 JSON)
    pub payload: serde_json::Value,
    /// 事件时间戳 (毫秒, None = 不校验)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,
}

/// 期望事件 (输出比对用)。
///
/// 跟 FixtureEvent 区别: payload_match 是"关心的字段"浅集合, 缺失字段被接受。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedEvent {
    /// 期望事件类型
    #[serde(rename = "type")]
    pub event_type: String,
    /// 期望的 payload 字段 (浅比较, BTreeMap 保序)
    #[serde(default)]
    pub payload_match: std::collections::BTreeMap<String, serde_json::Value>,
    /// 期望时间戳范围 (None = 不校验)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,
}

/// Fixture 加载器。
///
/// 一次加载一个 JSONL 文件, 每行一个 fixture。
pub struct FixtureLoader;

impl FixtureLoader {
    /// 从 JSONL 文件加载 fixture 列表。
    ///
    /// 空行跳过, 注释行 (`#` 开头) 跳过。
    pub fn from_jsonl(path: impl AsRef<Path>) -> Result<Vec<Fixture>, FixtureError> {
        let file = File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        let mut fixtures = Vec::new();
        for (line_idx, line) in reader.lines().enumerate() {
            let line_no = line_idx + 1;
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let fixture: Fixture = serde_json::from_str(trimmed).map_err(|e| {
                FixtureError::Parse {
                    line: line_no,
                    source: e,
                    raw: trimmed.to_string(),
                }
            })?;
            fixtures.push(fixture);
        }
        Ok(fixtures)
    }

    /// 从目录加载所有 .jsonl 文件。
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Vec<Fixture>, FixtureError> {
        let mut all = Vec::new();
        for entry in std::fs::read_dir(dir.as_ref())? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                let mut fixtures = Self::from_jsonl(&path)?;
                all.append(&mut fixtures);
            }
        }
        // 按 name 排序 (报告稳定)
        all.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(all)
    }
}

/// Fixture 错误。
#[derive(Debug, Error)]
pub enum FixtureError {
    /// IO 错误 (文件不存在 / 读失败)
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 解析错误 (某一行 parse 失败)
    #[error("parse error on line {line}: {source}\n  raw: {raw:?}")]
    Parse {
        /// 行号 (1-based)
        line: usize,
        /// 底层 serde 错误
        #[source]
        source: serde_json::Error,
        /// 原始行内容 (debug 友好)
        raw: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_category_as_str() {
        assert_eq!(FixtureCategory::ToolCall.as_str(), "tool_call");
        assert_eq!(FixtureCategory::AgentRun.as_str(), "agent_run");
        assert_eq!(FixtureCategory::SessionLifecycle.as_str(), "session_lifecycle");
        assert_eq!(FixtureCategory::EventOrdering.as_str(), "event_ordering");
        assert_eq!(FixtureCategory::ErrorPath.as_str(), "error_path");
    }

    #[test]
    fn fixture_roundtrip_json() {
        let json = r#"{
            "name": "test_one",
            "category": "tool_call",
            "input": {
                "session_id": "s1",
                "plugins": ["bash"],
                "events": [
                    {"type": "ToolCall", "payload": {"tool": "bash"}}
                ]
            },
            "output": {
                "events": [
                    {"type": "ToolCall", "payload_match": {"tool": "bash"}}
                ]
            }
        }"#;
        let f: Fixture = serde_json::from_str(json).unwrap();
        assert_eq!(f.name, "test_one");
        assert_eq!(f.category, FixtureCategory::ToolCall);
        assert_eq!(f.input.events.len(), 1);
        assert_eq!(f.output.events.len(), 1);

        // roundtrip
        let s = serde_json::to_string(&f).unwrap();
        let f2: Fixture = serde_json::from_str(&s).unwrap();
        assert_eq!(f.name, f2.name);
    }

    #[test]
    fn fixture_skip_optional_fields() {
        let json = r#"{
            "name": "minimal",
            "category": "agent_run",
            "input": {
                "session_id": "s2",
                "events": []
            },
            "output": {
                "events": []
            }
        }"#;
        let f: Fixture = serde_json::from_str(json).unwrap();
        assert!(f.description.is_none());
        assert!(f.input.plugins.is_empty());
        assert!(f.output.final_state.is_empty());
    }
}
