//! dsh (DeepSeek Harness) fixture 格式 + 转换器.
//!
//! 设计: 见 `docs/conformance-design.md` § 7 (双轨 fixture).
//!
//! ## dsh 真实格式 (待 Week 12 校准)
//!
//! **本文件按通用 JSONL 假设实现**. 真实 dsh 仓库的 fixture 格式可能跟这里不同,
//! Week 12 跑 dsh 真实 fixture 时按需调整.
//!
//! ## 假设的 dsh fixture 格式
//!
//! ```json
//! {
//!   "name": "agent_basic_run",
//!   "category": "agent_run",
//!   "input": {
//!     "session_id": "...",
//!     "messages": [{"role": "user", "content": "..."}],
//!     "tools": [{"name": "bash", "description": "..."}]
//!   },
//!   "expected_output": {
//!     "messages": [...],
//!     "events": [{"type": "...", "data": {...}}]
//!   }
//! }
//! ```
//!
//! **关键差异** (跟 ma-harness shape 比):
//! - dsh 用 `expected_output`, ma-harness 用 `output`
//! - dsh 用 `type` + `data`, ma-harness 用 `type` + `payload`
//! - dsh `messages` 数组在 input/output, ma-harness 只有 events
//! - dsh `tools` 是工具定义, ma-harness 是 plugin name
//!
//! ## 转换规则
//!
//! 1. `input.messages` 第一个 user message → `RunStart` event
//! 2. `input.tools` → `input.plugins` (按 name 取)
//! 3. `expected_output.events` → `output.events` (data → payload)
//! 4. `expected_output.messages` (assistant) → `ModelResponse` events
//!
//! ## Week 12 TODO
//!
//! - [ ] 拉 dsh 仓库, 实际看 `tests/fixtures/*.jsonl` 格式
//! - [ ] 校准本文件的转换规则
//! - [ ] 如果 dsh 用了 yaml 或别的格式, 加对应 loader

use crate::fixture::{ExpectedEvent, Fixture, FixtureCategory, FixtureEvent, FixtureInput, FixtureOutput};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;

/// dsh 风格 fixture (待校准).
///
/// 用 `#[serde(alias = ...)]` 兼容 ma-harness 自己生成的同名字段,
/// 也允许老 dsh fixture 用驼峰命名 (camelCase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshFixture {
    /// Fixture 名
    pub name: String,
    /// 分类 (字符串, 跟 ma-harness 不同命名)
    #[serde(default)]
    pub category: Option<String>,
    /// 人类描述
    #[serde(default)]
    pub description: Option<String>,
    /// 输入
    pub input: DshInput,
    /// 期望输出 (dsh 命名)
    #[serde(alias = "expected", alias = "expectedOutput")]
    pub expected_output: DshExpectedOutput,
}

/// dsh input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshInput {
    /// session id
    pub session_id: String,
    /// 装载的 plugin (dsh 可能叫 tools)
    #[serde(default, alias = "tools")]
    pub plugins: Vec<String>,
    /// 消息列表 (dsh 用 message 数组)
    #[serde(default)]
    pub messages: Vec<DshMessage>,
    /// 事件列表 (跟 ma-harness 类似)
    #[serde(default)]
    pub events: Vec<DshEvent>,
}

/// dsh message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshMessage {
    /// 角色
    pub role: String,
    /// 内容
    pub content: String,
}

/// dsh event (待校准, 可能是 type + data 风格)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshEvent {
    /// 事件类型
    #[serde(rename = "type")]
    pub event_type: String,
    /// 事件 data (dsh 命名, ma-harness 叫 payload)
    #[serde(default, alias = "payload")]
    pub data: serde_json::Value,
}

/// dsh 期望输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DshExpectedOutput {
    /// 事件列表
    #[serde(default)]
    pub events: Vec<DshEvent>,
    /// 消息列表 (assistant 视角)
    #[serde(default)]
    pub messages: Vec<DshMessage>,
}

/// dsh → ma-harness fixture 转换
pub fn dsh_to_fixture(dsh: DshFixture) -> Fixture {
    let category = parse_category(dsh.category.as_deref());
    let input = convert_input(dsh.input);
    let output = convert_expected(dsh.expected_output);
    Fixture {
        name: dsh.name,
        category,
        description: dsh.description,
        input,
        output,
    }
}

/// 字符串 category → FixtureCategory enum
fn parse_category(s: Option<&str>) -> FixtureCategory {
    match s {
        Some("tool_call") | Some("tool") => FixtureCategory::ToolCall,
        Some("agent_run") | Some("agent") => FixtureCategory::AgentRun,
        Some("session_lifecycle") | Some("session") => FixtureCategory::SessionLifecycle,
        Some("event_ordering") | Some("ordering") => FixtureCategory::EventOrdering,
        Some("error_path") | Some("error") => FixtureCategory::ErrorPath,
        _ => FixtureCategory::AgentRun, // 默认
    }
}

/// 转换 input (P11-1.5 改进: messages 派生完整 events)
///
/// 优先级:
/// 1. input.events 非空: 直接用 (dsh 风格 + explicit events)
/// 2. input.events 空 + messages 非空: 派生完整 ma-harness event 序列
///    - 第一个 user message → `RunStart` (前置, 表示 session 启动)
///    - user message → `UserInput`
///    - assistant message → `ModelResponse`
///    - system message → `SystemMessage`
///    - tool message → `ToolResult`
/// 3. tools 字段非空 + user message 触发 model 调用 → 自动派生 `ToolCall` (P11-1.5 v1 简化: 不派生, 让 fixture 显式给)
fn convert_input(input: DshInput) -> FixtureInput {
    let mut events = input.events
        .into_iter()
        .map(|e| FixtureEvent {
            event_type: e.event_type,
            payload: e.data,
            timestamp_ms: None,
        })
        .collect::<Vec<_>>();

    // P11-1.5: input.events 空 + messages 非空, 派生完整 events (跟 framework 视角对齐)
    if events.is_empty() && !input.messages.is_empty() {
        // 第一个 user message 触发 RunStart (前置)
        if input.messages.iter().any(|m| m.role == "user") {
            events.push(FixtureEvent {
                event_type: "RunStart".to_string(),
                payload: serde_json::json!({"model": "stub"}),
                timestamp_ms: None,
            });
        }
        for msg in &input.messages {
            let event_type = match msg.role.as_str() {
                "user" => "UserInput",
                "assistant" => "ModelResponse",
                "system" => "SystemMessage",
                "tool" => "ToolResult",
                _ => continue,
            };
            events.push(FixtureEvent {
                event_type: event_type.to_string(),
                payload: serde_json::json!({
                    match event_type {
                        "UserInput" => "content",
                        "ModelResponse" => "content",
                        "SystemMessage" => "content",
                        "ToolResult" => "result",
                        _ => "content",
                    }: msg.content
                }),
                timestamp_ms: None,
            });
        }
    }

    FixtureInput {
        session_id: input.session_id,
        plugins: input.plugins,
        events,
    }
}

/// 转换 expected output (P11-1.5 改进: ModelResponse 包装成 `{content: "..."}` 跟 ma-harness 视角对齐)
fn convert_expected(expected: DshExpectedOutput) -> FixtureOutput {
    let events: Vec<ExpectedEvent> = expected
        .events
        .into_iter()
        .map(|e| {
            // dsh data → ma-harness payload_match (浅 map)
            let payload_match: BTreeMap<String, serde_json::Value> = match e.data {
                serde_json::Value::Object(m) => m.into_iter().collect(),
                other => {
                    // P11-1.5: 特殊处理 ModelResponse (ma-harness 视角: {content: "..."})
                    // 跟 UserInput / SystemMessage / ToolResult 都按 content 包装
                    let mut map = BTreeMap::new();
                    let key = match e.event_type.as_str() {
                        "UserInput" | "ModelResponse" | "SystemMessage" | "ToolError" => "content",
                        "ToolResult" => "result",
                        _ => "data",
                    };
                    map.insert(key.to_string(), other);
                    map
                }
            };
            ExpectedEvent {
                event_type: e.event_type,
                payload_match,
                timestamp_ms: None,
            }
        })
        .collect();

    // 派生 model response events from messages (assistant role → ModelResponse)
    let mut all_events = events;
    for msg in expected.messages.iter().filter(|m| m.role == "assistant") {
        let mut payload_match = BTreeMap::new();
        payload_match.insert("content".to_string(), serde_json::Value::String(msg.content.clone()));
        all_events.push(ExpectedEvent {
            event_type: "ModelResponse".to_string(),
            payload_match,
            timestamp_ms: None,
        });
    }

    FixtureOutput {
        events: all_events,
        final_state: BTreeMap::new(),
    }
}

/// dsh 错误
#[derive(Debug, Error)]
pub enum DshError {
    /// 解析失败
    #[error("dsh parse error: {0}")]
    Parse(#[from] serde_json::Error),
    /// IO
    #[error("dsh io error: {0}")]
    Io(#[from] std::io::Error),
}

/// 从 JSONL 字符串加载 dsh fixture 列表
pub fn parse_dsh_jsonl(content: &str) -> Result<Vec<Fixture>, DshError> {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let dsh: DshFixture = serde_json::from_str(trimmed)?;
        out.push(dsh_to_fixture(dsh));
    }
    Ok(out)
}

/// P12-1 性能优化: dsh fixture 缓存 (按 path + mtime)
///
/// 业务方反复跑同一文件时, 跳过重复 parse. 业务方线程安全 (用 Mutex).
#[derive(Debug, Default)]
pub struct DshFixtureCache {
    cache: std::sync::Mutex<BTreeMap<PathBuf, CacheEntry>>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    /// File mtime (for invalidation)
    mtime: SystemTime,
    /// Parsed fixtures
    fixtures: Vec<Fixture>,
}

impl DshFixtureCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get parsed fixtures from file, with mtime-based caching.
    ///
    /// 业务方重复调 `from_jsonl_cached` 同一 path, 只 parse 一次 (file 不变).
    /// 业务方修改 file 后, mtime 变, 重新 parse.
    pub fn from_jsonl_cached(&self, path: impl AsRef<Path>) -> Result<Vec<Fixture>, DshError> {
        let path = path.as_ref().to_path_buf();
        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(DshError::Io)?;

        let mut cache = self.cache.lock().expect("cache lock poisoned");

        // 检查 cache 命中
        if let Some(entry) = cache.get(&path) {
            if entry.mtime == mtime {
                return Ok(entry.fixtures.clone());
            }
        }

        // Cache miss 或 mtime 变 → 重 parse
        let content = std::fs::read_to_string(&path).map_err(DshError::Io)?;
        let fixtures = parse_dsh_jsonl(&content)?;
        cache.insert(
            path,
            CacheEntry {
                mtime,
                fixtures: fixtures.clone(),
            },
        );
        Ok(fixtures)
    }

    /// 清空缓存 (业务方主动, e.g. file 改动频繁场景)
    pub fn clear(&self) {
        self.cache.lock().expect("cache lock poisoned").clear();
    }

    /// 当前缓存大小
    pub fn len(&self) -> usize {
        self.cache.lock().expect("cache lock poisoned").len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn dsh_fixture_cache_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"name":"a","input":{{"session_id":"s","events":[]}},"expected_output":{{"events":[]}}}}"#).unwrap();
        drop(f);

        let cache = DshFixtureCache::new();
        assert!(cache.is_empty());

        // 第一次: cache miss
        let f1 = cache.from_jsonl_cached(&path).unwrap();
        assert_eq!(f1.len(), 1);
        assert_eq!(cache.len(), 1);

        // 第二次: cache hit (mtime 没变)
        let f2 = cache.from_jsonl_cached(&path).unwrap();
        assert_eq!(f2.len(), 1);
        assert_eq!(cache.len(), 1);  // 还是 1 个 entry
    }

    #[test]
    fn dsh_fixture_cache_mtime_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        std::fs::write(
            &path,
            r#"{"name":"a","input":{"session_id":"s","events":[]},"expected_output":{"events":[]}}"#,
        )
        .unwrap();

        let cache = DshFixtureCache::new();
        let _f1 = cache.from_jsonl_cached(&path).unwrap();

        // Sleep 让 mtime 变 (Windows 文件系统 mtime 精度 100ns+)
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(
            &path,
            r#"{"name":"a","input":{"session_id":"s","events":[]},"expected_output":{"events":[]}}
{"name":"b","input":{"session_id":"s","events":[]},"expected_output":{"events":[]}}"#,
        )
        .unwrap();

        let f2 = cache.from_jsonl_cached(&path).unwrap();
        assert_eq!(f2.len(), 2, "mtime change should invalidate cache");
    }

    #[test]
    fn dsh_fixture_cache_clear() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        std::fs::write(
            &path,
            r#"{"name":"a","input":{"session_id":"s","events":[]},"expected_output":{"events":[]}}"#,
        )
        .unwrap();

        let cache = DshFixtureCache::new();
        cache.from_jsonl_cached(&path).unwrap();
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn dsh_fixture_cache_nonexistent() {
        let cache = DshFixtureCache::new();
        let result = cache.from_jsonl_cached("/nonexistent/path.jsonl");
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dsh_with_events() {
        let json = r#"{
            "name": "dsh_test_one",
            "category": "tool_call",
            "description": "dsh simple",
            "input": {
                "session_id": "s1",
                "plugins": ["bash"],
                "events": [
                    {"type": "ToolCall", "data": {"tool": "bash"}}
                ]
            },
            "expected_output": {
                "events": [
                    {"type": "ToolResult", "data": {"result": "hi\n"}}
                ]
            }
        }"#;
        let f: DshFixture = serde_json::from_str(json).unwrap();
        let ma = dsh_to_fixture(f);
        assert_eq!(ma.name, "dsh_test_one");
        assert_eq!(ma.category, FixtureCategory::ToolCall);
        assert_eq!(ma.input.events.len(), 1);
        assert_eq!(ma.input.plugins, vec!["bash"]);
        assert_eq!(ma.output.events.len(), 1);
        assert_eq!(ma.output.events[0].event_type, "ToolResult");
        assert_eq!(ma.output.events[0].payload_match.get("result").unwrap(), "hi\n");
    }

    #[test]
    fn parse_dsh_alias_expected_output() {
        let json = r#"{
            "name": "alias_test",
            "input": {"session_id": "s", "events": []},
            "expected": {"events": []}
        }"#;
        let f: DshFixture = serde_json::from_str(json).unwrap();
        let ma = dsh_to_fixture(f);
        assert_eq!(ma.name, "alias_test");
    }

    #[test]
    fn parse_dsh_alias_tools() {
        let json = r#"{
            "name": "tools_alias",
            "input": {
                "session_id": "s",
                "tools": ["bash", "fs"],
                "events": []
            },
            "expected_output": {"events": []}
        }"#;
        let f: DshFixture = serde_json::from_str(json).unwrap();
        let ma = dsh_to_fixture(f);
        assert_eq!(ma.input.plugins, vec!["bash", "fs"]);
    }

    #[test]
    fn parse_dsh_data_alias_payload() {
        let json = r#"{
            "name": "payload_alias",
            "input": {"session_id": "s", "events": []},
            "expected_output": {
                "events": [
                    {"type": "ModelResponse", "payload": {"text": "hi"}}
                ]
            }
        }"#;
        let f: DshFixture = serde_json::from_str(json).unwrap();
        let ma = dsh_to_fixture(f);
        assert_eq!(ma.output.events[0].payload_match.get("text").unwrap(), "hi");
    }

    #[test]
    fn parse_dsh_derives_user_input_from_messages() {
        // P11-1.5: messages 派生完整 ma-harness 视角 events
        // - 第一个 user message 触发 RunStart (前置)
        // - user → UserInput, assistant → ModelResponse
        let json = r#"{
            "name": "from_messages",
            "input": {
                "session_id": "s",
                "messages": [
                    {"role": "user", "content": "hi"},
                    {"role": "assistant", "content": "hello"}
                ]
            },
            "expected_output": {"events": []}
        }"#;
        let f: DshFixture = serde_json::from_str(json).unwrap();
        let ma = dsh_to_fixture(f);
        // 期望 3 events: RunStart + UserInput + ModelResponse
        assert_eq!(ma.input.events.len(), 3);
        assert_eq!(ma.input.events[0].event_type, "RunStart");
        assert_eq!(ma.input.events[1].event_type, "UserInput");
        assert_eq!(ma.input.events[1].payload["content"], "hi");
        assert_eq!(ma.input.events[2].event_type, "ModelResponse");
        assert_eq!(ma.input.events[2].payload["content"], "hello");
    }

    #[test]
    fn parse_dsh_derives_model_response_from_assistant_messages() {
        let json = r#"{
            "name": "assistant_msg",
            "input": {"session_id": "s", "events": []},
            "expected_output": {
                "events": [],
                "messages": [
                    {"role": "assistant", "content": "I can help"}
                ]
            }
        }"#;
        let f: DshFixture = serde_json::from_str(json).unwrap();
        let ma = dsh_to_fixture(f);
        assert_eq!(ma.output.events.len(), 1);
        assert_eq!(ma.output.events[0].event_type, "ModelResponse");
        assert_eq!(ma.output.events[0].payload_match.get("content").unwrap(), "I can help");
    }

    #[test]
    fn parse_dsh_category_alias() {
        let cases = [
            ("tool", FixtureCategory::ToolCall),
            ("agent", FixtureCategory::AgentRun),
            ("session", FixtureCategory::SessionLifecycle),
            ("ordering", FixtureCategory::EventOrdering),
            ("error", FixtureCategory::ErrorPath),
            ("unknown", FixtureCategory::AgentRun),
        ];
        for (s, expected) in cases {
            assert_eq!(parse_category(Some(s)), expected, "s={s}");
        }
        assert_eq!(parse_category(None), FixtureCategory::AgentRun);
    }

    #[test]
    fn parse_dsh_jsonl_skips_blank_and_comment() {
        let content = r#"
# 注释
{"name":"a","input":{"session_id":"s","events":[]},"expected_output":{"events":[]}}

{"name":"b","input":{"session_id":"s","events":[]},"expected_output":{"events":[]}}
"#;
        let fs = parse_dsh_jsonl(content).unwrap();
        assert_eq!(fs.len(), 2);
        assert_eq!(fs[0].name, "a");
        assert_eq!(fs[1].name, "b");
    }

    #[test]
    fn parse_dsh_non_object_data() {
        // P11-1.5: non-object `data` 字段被包成 map
        // 用非特殊 event type (Log), 让 fallback "data" key 路径被覆盖
        // (特殊 event type 走 "content"/"result" key, 不走 "data" key)
        let json = r#"{
            "name": "string_data",
            "input": {"session_id": "s", "events": []},
            "expected_output": {
                "events": [
                    {"type": "Log", "data": "raw_string_response"}
                ]
            }
        }"#;
        let f: DshFixture = serde_json::from_str(json).unwrap();
        let ma = dsh_to_fixture(f);
        // Log 是非特殊 event type, key 走 fallback "data"
        assert_eq!(ma.output.events[0].event_type, "Log");
        assert_eq!(ma.output.events[0].payload_match.get("data").unwrap(), "raw_string_response");
    }

    #[test]
    fn parse_dsh_non_object_data_for_model_response_uses_content_key() {
        // P11-1.5: ModelResponse 的 string data 走 "content" key (ma-harness 视角对齐)
        let json = r#"{
            "name": "string_data_mr",
            "input": {"session_id": "s", "events": []},
            "expected_output": {
                "events": [
                    {"type": "ModelResponse", "data": "raw_string_response"}
                ]
            }
        }"#;
        let f: DshFixture = serde_json::from_str(json).unwrap();
        let ma = dsh_to_fixture(f);
        // ModelResponse → "content" key (特殊 event type)
        assert_eq!(ma.output.events[0].event_type, "ModelResponse");
        assert_eq!(ma.output.events[0].payload_match.get("content").unwrap(), "raw_string_response");
    }
}
