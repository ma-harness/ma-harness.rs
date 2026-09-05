//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-hooks`
//! **Crate ident** (`use` 路径): `ma_harness_hooks`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-hooks = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_hooks::{ClaudeCodeAdapter, Hook, HookEvent, HookResponse, HookDecision};
//!
//! let adapter = ClaudeCodeAdapter::new();
//! let event = ClaudeCodeAdapter::parse_event(r#"{"hook_event_name":"PreToolUse","tool_name":"Bash"}"#)?;
//! let response = adapter.handle(&event);
//! println!("{}", ClaudeCodeAdapter::render_response(&response));
//! ```
//!
//! # 设计 (Design) — P15.7
//!
//! **目标**: 抽象 hook wire protocol, 让 ma-harness 接 Claude Code / Codex / 其它 IDE
//! 的 hook event 流 (跟 dsh `packages/hooks/` 1:1 对等).
//!
//! **背景**: 见 [dsh-feature-parity-table §11] (deferred: hook bridges). Claude Code
//! 跟 Codex 都通过 stdin/stdout JSON 跟外部 hook 通信, ma-harness 作为 hook 接入,
//! 业务方 agent 能 cross-runner 调度.
//!
//! **核心抽象**:
//! - [`HookEvent`]: 统一事件表示 (kind + session + tool + payload), 跟 runner 无关
//! - [`HookResponse`]: 业务方决定 (Allow / Deny / Continue), 跟 runner 协议无关
//! - [`Hook`] trait: 业务方注册 hook handler
//! - [`ClaudeCodeAdapter`]: Claude Code 协议 adapter (parse stdin JSON, render stdout)
//! - [`HOOK`] typed key: ctx 注入
//!
//! **Claude Code 协议** (P15.7.1 minimal):
//! - stdin: `{"session_id":"...","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{...}}`
//! - stdout: `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}` (P15.7.2+)
//! - exit 0: allow, exit 2: deny (with stderr message), other: non-blocking error
//!
//! **6 质量属性** (业务方 2026-09-04 约定):
//! - 可复用: Hook trait 抽象, future Codex / Cursor / Continue 等可加 adapter
//! - 可维护: 模块化分块, error / event / response / adapter 集中 lib.rs
//! - 鲁棒: 未知 event_kind 返 Continue (不 panic), invalid JSON 显式 error
//! - 安全: 不 eval tool_input 内容, 静态处理
//! - 可测: 单元测试覆盖 event parse + response render + adapter dispatch
//! - 可扩展: trait 抽象, future runner adapter 跟新 event_kind 都能接
//!
//! # 限制 (Limitations) — P15.7.1
//!
//! - **没**Codex / Cursor / Continue adapter (P15.7.2+)
//! - **没**`mah hook install <name>` CLI 实装 (P15.7.3+, 但 crate 有 print_install_hint)
//! - **没**wire encryption (Claude Code 当前也无)
//! - **没**streaming output (P15.7.4+)

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// HookError
// ============================================================================

/// Hook capability error.
#[derive(Debug, Error)]
pub enum HookError {
    /// JSON 解析失败 (runner 协议格式错)
    #[error("hook JSON parse error: {0}")]
    Parse(String),

    /// 业务方 handler 内部错误
    #[error("hook handler error: {0}")]
    Handler(String),

    /// IO 错误 (read stdin / write stdout)
    #[error("hook I/O error: {0}")]
    Io(String),

    /// 不支持的事件 / 操作
    #[error("hook unsupported: {0}")]
    Unsupported(String),
}

// ============================================================================
// HookEventKind
// ============================================================================

/// 已知 hook event 种类 (P15.7.1: Claude Code 子集).
///
/// **业务方**: 用 match 模式匹配. `Unknown(s)` 兜底, future event 加 enum
/// 变体时老代码编译会 warn (业务方可选择性升级).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEventKind {
    /// Claude Code: tool 调用前
    PreToolUse,
    /// Claude Code: tool 调用后
    PostToolUse,
    /// Claude Code: 用户 prompt 提交
    UserPromptSubmit,
    /// Claude Code: session 启动
    SessionStart,
    /// Claude Code: session 结束
    SessionEnd,
    /// Claude Code: agent 停止
    Stop,
    /// Claude Code: 通知
    Notification,
    /// 未知 / 未来 event kind (raw string)
    #[serde(untagged)]
    Unknown(String),
}

impl std::fmt::Display for HookEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HookEventKind::PreToolUse => "PreToolUse",
            HookEventKind::PostToolUse => "PostToolUse",
            HookEventKind::UserPromptSubmit => "UserPromptSubmit",
            HookEventKind::SessionStart => "SessionStart",
            HookEventKind::SessionEnd => "SessionEnd",
            HookEventKind::Stop => "Stop",
            HookEventKind::Notification => "Notification",
            HookEventKind::Unknown(s) => s,
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for HookEventKind {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "PreToolUse" => HookEventKind::PreToolUse,
            "PostToolUse" => HookEventKind::PostToolUse,
            "UserPromptSubmit" => HookEventKind::UserPromptSubmit,
            "SessionStart" => HookEventKind::SessionStart,
            "SessionEnd" => HookEventKind::SessionEnd,
            "Stop" => HookEventKind::Stop,
            "Notification" => HookEventKind::Notification,
            other => HookEventKind::Unknown(other.to_string()),
        })
    }
}

// ============================================================================
// HookEvent
// ============================================================================

/// Hook event (P15.7.1).
///
/// 统一表示来自不同 runner (Claude Code / Codex / ...) 的 event. 业务方
/// adapter 解析 runner-specific JSON 拿到这个 struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    /// Event kind
    pub kind: HookEventKind,
    /// Session ID (e.g. Claude Code session)
    pub session_id: Option<String>,
    /// Tool name (e.g. "Bash", "Read", "Edit") — Pre/PostToolUse
    pub tool_name: Option<String>,
    /// Tool input (PreToolUse)
    pub tool_input: Option<serde_json::Value>,
    /// Tool response (PostToolUse)
    pub tool_response: Option<serde_json::Value>,
    /// User message (UserPromptSubmit)
    pub user_message: Option<String>,
    /// Stop reason (Stop)
    pub stop_reason: Option<String>,
    /// 原始 JSON payload (业务方高级用法: 看完整 runner event)
    #[serde(skip)]
    pub raw: serde_json::Value,
}

impl HookEvent {
    /// 创建一个空 event (test / stub 用)
    pub fn new(kind: HookEventKind) -> Self {
        Self {
            kind,
            session_id: None,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            user_message: None,
            stop_reason: None,
            raw: serde_json::Value::Null,
        }
    }
}

// ============================================================================
// HookDecision + HookResponse
// ============================================================================

/// Hook 业务方决定 (P15.7.1).
///
/// 跟 runner 协议无关. Adapter 负责把 Decision 翻译成 runner 协议 (e.g.
/// Claude Code: stdout JSON + exit code 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookDecision {
    /// Allow (runner 继续执行原 action)
    Allow,
    /// Deny (runner 阻断 action, 业务方给 reason)
    Deny {
        /// 业务方给 runner 看的 reason
        reason: String,
    },
    /// Continue (此 event 无 decision 需求, runner 继续)
    Continue,
}

impl std::fmt::Display for HookDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookDecision::Allow => write!(f, "allow"),
            HookDecision::Deny { reason } => write!(f, "deny: {reason}"),
            HookDecision::Continue => write!(f, "continue"),
        }
    }
}

/// Hook response (P15.7.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResponse {
    /// 业务方决定
    pub decision: HookDecision,
    /// 业务方可选 stdout (Claude Code: JSON to stdout)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<serde_json::Value>,
}

impl HookResponse {
    /// 创建一个 Allow response
    pub fn allow() -> Self {
        Self {
            decision: HookDecision::Allow,
            stdout: None,
        }
    }

    /// 创建一个 Deny response with reason
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            decision: HookDecision::Deny {
                reason: reason.into(),
            },
            stdout: None,
        }
    }

    /// 创建一个 Continue response
    pub fn continue_execution() -> Self {
        Self {
            decision: HookDecision::Continue,
            stdout: None,
        }
    }
}

// ============================================================================
// Hook trait
// ============================================================================

/// Hook handler (P15.7.1).
///
/// **业务方用**: 实现 `Hook` trait, 业务方逻辑走这里 (e.g. approval policy /
/// audit log / 转发到 agent loop).
#[async_trait]
pub trait Hook: Send + Sync + 'static {
    /// Hook 标识 (e.g. "audit-log", "approval-policy")
    fn name(&self) -> &'static str;

    /// 处理一个 event, 返 response.
    ///
    /// **业务方**: 默认 impl 可以是 `HookResponse::continue_execution()` (啥都不做),
    /// 或者 `HookResponse::allow()` (无脑放行).
    async fn handle(&self, event: &HookEvent) -> Result<HookResponse, HookError>;
}

/// Default no-op hook (P15.7.1 测试用 + 业务方 stub 起点).
pub struct NoopHook;

#[async_trait]
impl Hook for NoopHook {
    fn name(&self) -> &'static str {
        "noop"
    }
    async fn handle(&self, _event: &HookEvent) -> Result<HookResponse, HookError> {
        Ok(HookResponse::continue_execution())
    }
}

// ============================================================================
// ClaudeCodeAdapter (P15.7.1 主交付)
// ============================================================================

/// Claude Code 协议 adapter (P15.7.1).
///
/// **行为**:
/// - `parse_event(json)` 解析 Claude Code stdin JSON → HookEvent
/// - `render_response(response)` 渲染业务方决定 → Claude Code stdout JSON
/// - `run_hook(hook)` 主循环: read stdin → parse → call hook → render
///
/// **Claude Code 协议摘要**:
/// - stdin: `{"session_id":"...","transcript_path":"...","cwd":"...","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{...}}`
/// - stdout: `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}`
/// - exit 0: allow
/// - exit 2: deny (stderr 给 reason)
/// - other: 错误 (Claude Code 警告, 继续)
pub struct ClaudeCodeAdapter;

impl ClaudeCodeAdapter {
    /// 创建一个新的 ClaudeCodeAdapter.
    pub fn new() -> Self {
        Self
    }

    /// 解析 Claude Code stdin JSON → HookEvent (P15.7.1).
    ///
    /// **业务方**: 从 stdin 读 string 后调这个.
    ///
    /// **Errors**:
    /// - `HookError::Parse` — JSON 错 / 缺 `hook_event_name` 字段
    pub fn parse_event(json: &str) -> Result<HookEvent, HookError> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| HookError::Parse(format!("invalid JSON: {e}")))?;
        let raw = value.clone();
        let obj = value
            .as_object()
            .ok_or_else(|| HookError::Parse("event is not a JSON object".into()))?;
        let kind_str = obj
            .get("hook_event_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HookError::Parse("missing hook_event_name".into()))?;
        let kind: HookEventKind = kind_str
            .parse()
            .map_err(|_| HookError::Parse(format!("invalid hook_event_name: {kind_str:?}")))?;
        Ok(HookEvent {
            kind,
            session_id: obj
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            tool_name: obj
                .get("tool_name")
                .and_then(|v| v.as_str())
                .map(String::from),
            tool_input: obj.get("tool_input").cloned(),
            tool_response: obj.get("tool_response").cloned(),
            user_message: obj
                .get("user_message")
                .and_then(|v| v.as_str())
                .map(String::from),
            stop_reason: obj
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .map(String::from),
            raw,
        })
    }

    /// 渲染业务方决定 → Claude Code stdout JSON (P15.7.1).
    ///
    /// **注**: Claude Code 协议用 `permissionDecision` 字段 (allow/deny/ask).
    /// Continue event (UserPromptSubmit 等) 不返 stdout (用空字符串).
    pub fn render_response(response: &HookResponse) -> String {
        match &response.decision {
            HookDecision::Allow => serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "",
                    "permissionDecision": "allow",
                }
            })
            .to_string(),
            HookDecision::Deny { reason } => serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            })
            .to_string(),
            HookDecision::Continue => String::new(),
        }
    }

    /// 拿到 response 应对应的 exit code (Claude Code 协议).
    ///
    /// - 0: allow / continue (正常退出)
    /// - 2: deny (Claude Code 阻断 action)
    /// - other: 错误 (Claude Code warn + 继续, 业务方应该返 0)
    pub fn exit_code(response: &HookResponse) -> i32 {
        match &response.decision {
            HookDecision::Deny { .. } => 2,
            _ => 0,
        }
    }
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// 安装 hint 给 stdout (业务方跑 `mah hook install claude-code` 看这个)
pub fn print_install_hint(name: &str) {
    match name {
        "claude-code" => {
            // 用 raw string literal (r##"..."##) 避免内嵌双引号 escape
            let example = r##"{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "mah hook run claude-code"
          }
        ]
      }
    ]
  }
}"##;
            println!("To install ma-harness as a Claude Code hook:");
            println!();
            println!("  1. Find or create ~/.claude/settings.json");
            println!("  2. Add a hooks entry like:");
            println!();
            for line in example.lines() {
                println!("     {line}");
            }
            println!();
            println!("  3. Restart Claude Code. mah will be called on every PreToolUse event.");
        }
        _ => {
            println!("Unknown hook: {name}");
            println!("Available hooks: claude-code");
        }
    }
}

// ============================================================================
// Typed key
// ============================================================================

/// Typed key: `ctx.hooks` 注入的 Hook (业务方未来注册).
pub static HOOK: ma_harness_cordis::CtxKey<Arc<dyn Hook>> = ma_harness_seam::ctx_key!("hook");

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- HookEventKind -----

    #[test]
    fn hook_event_kind_display_renders_known() {
        assert_eq!(format!("{}", HookEventKind::PreToolUse), "PreToolUse");
        assert_eq!(format!("{}", HookEventKind::Stop), "Stop");
        assert_eq!(
            format!("{}", HookEventKind::Unknown("Custom".to_string())),
            "Custom"
        );
    }

    #[test]
    fn hook_event_kind_from_str_recognizes_known_and_unknown() {
        let k: HookEventKind = "PreToolUse".parse().unwrap();
        assert_eq!(k, HookEventKind::PreToolUse);
        let k: HookEventKind = "FutureEvent".parse().unwrap();
        assert_eq!(k, HookEventKind::Unknown("FutureEvent".to_string()));
    }

    #[test]
    fn hook_event_kind_serde_roundtrip() {
        let k = HookEventKind::Stop;
        let s = serde_json::to_string(&k).unwrap();
        let d: HookEventKind = serde_json::from_str(&s).unwrap();
        assert_eq!(k, d);

        // Unknown 变体也 roundtrip
        let k = HookEventKind::Unknown("Custom2077".to_string());
        let s = serde_json::to_string(&k).unwrap();
        let d: HookEventKind = serde_json::from_str(&s).unwrap();
        assert_eq!(k, d);
    }

    // ----- HookEvent -----

    #[test]
    fn hook_event_new_is_empty() {
        let e = HookEvent::new(HookEventKind::PreToolUse);
        assert_eq!(e.kind, HookEventKind::PreToolUse);
        assert!(e.session_id.is_none());
        assert!(e.tool_name.is_none());
    }

    // ----- HookDecision + HookResponse -----

    #[test]
    fn hook_response_allow_has_no_stdout() {
        let r = HookResponse::allow();
        assert_eq!(r.decision, HookDecision::Allow);
        assert!(r.stdout.is_none());
    }

    #[test]
    fn hook_response_deny_captures_reason() {
        let r = HookResponse::deny("rate limit exceeded");
        match &r.decision {
            HookDecision::Deny { reason } => assert_eq!(reason, "rate limit exceeded"),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn hook_response_continue_is_distinct() {
        let r = HookResponse::continue_execution();
        assert_eq!(r.decision, HookDecision::Continue);
    }

    // ----- ClaudeCodeAdapter::parse_event -----

    #[test]
    fn claude_code_adapter_parses_pre_tool_use() {
        let json = r#"{
            "session_id": "abc-123",
            "cwd": "/home/user",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "ls -la"}
        }"#;
        let e = ClaudeCodeAdapter::parse_event(json).expect("parse");
        assert_eq!(e.kind, HookEventKind::PreToolUse);
        assert_eq!(e.session_id.as_deref(), Some("abc-123"));
        assert_eq!(e.tool_name.as_deref(), Some("Bash"));
        assert!(e.tool_input.is_some());
    }

    #[test]
    fn claude_code_adapter_parses_post_tool_use() {
        let json = r#"{
            "session_id": "xyz",
            "hook_event_name": "PostToolUse",
            "tool_name": "Read",
            "tool_input": {"file_path": "/etc/hosts"},
            "tool_response": {"content": "127.0.0.1 localhost"}
        }"#;
        let e = ClaudeCodeAdapter::parse_event(json).expect("parse");
        assert_eq!(e.kind, HookEventKind::PostToolUse);
        assert!(e.tool_response.is_some());
    }

    #[test]
    fn claude_code_adapter_parses_unknown_kind_as_unknown_variant() {
        let json = r#"{"hook_event_name": "FutureEvent", "tool_name": "Bash"}"#;
        let e = ClaudeCodeAdapter::parse_event(json).expect("parse");
        assert_eq!(e.kind, HookEventKind::Unknown("FutureEvent".to_string()));
    }

    #[test]
    fn claude_code_adapter_rejects_invalid_json() {
        let err = ClaudeCodeAdapter::parse_event("not json").unwrap_err();
        assert!(matches!(err, HookError::Parse(_)));
    }

    #[test]
    fn claude_code_adapter_rejects_missing_hook_event_name() {
        let err = ClaudeCodeAdapter::parse_event(r#"{"session_id": "x"}"#).unwrap_err();
        assert!(matches!(err, HookError::Parse(_)));
    }

    #[test]
    fn claude_code_adapter_rejects_non_object() {
        let err = ClaudeCodeAdapter::parse_event(r#""a string""#).unwrap_err();
        assert!(matches!(err, HookError::Parse(_)));
    }

    // ----- ClaudeCodeAdapter::render_response -----

    #[test]
    fn claude_code_adapter_renders_allow_response() {
        let s = ClaudeCodeAdapter::render_response(&HookResponse::allow());
        let v: serde_json::Value = serde_json::from_str(&s).expect("parse");
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "allow");
    }

    #[test]
    fn claude_code_adapter_renders_deny_response_with_reason() {
        let s = ClaudeCodeAdapter::render_response(&HookResponse::deny("blocked by policy"));
        let v: serde_json::Value = serde_json::from_str(&s).expect("parse");
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(
            v["hookSpecificOutput"]["permissionDecisionReason"],
            "blocked by policy"
        );
    }

    #[test]
    fn claude_code_adapter_renders_continue_as_empty() {
        let s = ClaudeCodeAdapter::render_response(&HookResponse::continue_execution());
        assert!(s.is_empty(), "Continue should render as empty stdout");
    }

    // ----- ClaudeCodeAdapter::exit_code -----

    #[test]
    fn claude_code_adapter_exit_code_0_for_allow_and_continue() {
        assert_eq!(ClaudeCodeAdapter::exit_code(&HookResponse::allow()), 0);
        assert_eq!(
            ClaudeCodeAdapter::exit_code(&HookResponse::continue_execution()),
            0
        );
    }

    #[test]
    fn claude_code_adapter_exit_code_2_for_deny() {
        assert_eq!(ClaudeCodeAdapter::exit_code(&HookResponse::deny("x")), 2);
    }

    // ----- NoopHook -----

    #[tokio::test]
    async fn noop_hook_returns_continue() {
        let hook = NoopHook;
        assert_eq!(hook.name(), "noop");
        let e = HookEvent::new(HookEventKind::PreToolUse);
        let r = hook.handle(&e).await.expect("handle");
        assert_eq!(r.decision, HookDecision::Continue);
    }

    // ----- print_install_hint -----

    #[test]
    fn print_install_hint_claude_code_runs_without_panic() {
        // 不验证 stdout 内容 (太 fragile), 只 verify 不 panic
        print_install_hint("claude-code");
        print_install_hint("unknown");
    }
}
