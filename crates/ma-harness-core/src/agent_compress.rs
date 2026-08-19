//! 上下文压缩 (P8-1 / Day 101)
//!
//! 长 session messages 累计超过 model 上下文窗口时, 压缩历史 messages.
//! 两种策略:
//! - **Sliding window** (默认): 保留最近 N 条, 丢最早
//! - **Summarize** (v2 TODO): 用 LLM 摘要压缩 (跟 dsh design 一样, 调 LLM 生成摘要替换老 messages)
//!
//! ## Token 估算
//!
//! 简化: `tokens = chars / 4` (OpenAI 经验值 1 token ≈ 4 char 英文; 中文 1.5 char).
//! 业务方可改用 tiktoken-rs 精估 (P8-3 跟多模型一起做).
//!
//! ## History loading
//!
//! 业务方调 `load_history_from_log(log, session_id, max_messages)`:
//! - 拿该 session 之前所有 ModelRequest / ModelResponse / UserInput events
//! - 按时间顺序重建 messages
//! - 自动跳过 model-invisible events
//! - 用于续接前文, 跟 dsh `ctx.history` 设计对齐

use serde::{Deserialize, Serialize};

use crate::{
    agent::ModelMessage,
    event::{EventType, SessionEvent},
    log::EventLog,
};

/// 压缩策略 (P8-1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum CompressionPolicy {
    /// 不压缩 (默认 v1 简化, 业务方要求时才压缩)
    Never,
    /// Sliding window: 保留最近 keep_last_n 条, 丢最早 (适合 4k-32k context)
    SlidingWindow {
        /// 保留最近 N 条 messages
        keep_last_n: usize,
    },
    /// Summarize (v2 TODO): 调 LLM 摘要压缩
    Summarize,
}

impl Default for CompressionPolicy {
    fn default() -> Self {
        // P8-1 v1: 默认 sliding window keep 20 (够大多数场景)
        CompressionPolicy::SlidingWindow { keep_last_n: 20 }
    }
}

/// 估 token 数 (粗估, P8-3 改用精确)
pub fn estimate_tokens(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    // 简化: ASCII 字符 1/4 token, 其他字符 (CJK) 1/1.5 token
    let mut tokens = 0u32;
    let mut ascii_count = 0u32;
    let mut other_count = 0u32;
    for c in text.chars() {
        if c.is_ascii() {
            ascii_count += 1;
        } else {
            other_count += 1;
        }
    }
    tokens += ascii_count / 4;
    tokens += (other_count as f32 / 1.5) as u32;
    tokens.max(1) // 至少 1 token (避免空 string 0)
}

/// 估 messages 总 token
pub fn estimate_messages_tokens(messages: &[ModelMessage]) -> u32 {
    messages
        .iter()
        .map(|m| {
            estimate_tokens(&m.role) + estimate_tokens(&m.content)
        })
        .sum()
}

/// 从 EventLog 加载历史 messages (P8-1)
///
/// 重建该 session 的对话历史:
/// - UserInput → user message
/// - ModelRequest → (不在 messages 里, 是 LLM 调用的输入)
/// - ModelResponse → assistant message
/// - ToolCall + ToolResult → 暂不展开 (P8-1 v1 简化, 跟 dsh v1 一致)
pub fn load_history_from_log(
    log: &EventLog,
    session_id: &str,
    max_messages: usize,
) -> Result<Vec<ModelMessage>, anyhow::Error> {
    let page = log.query(&crate::log::EventQuery {
        session_id: session_id.to_string(),
        model_visible_only: true,
        ..Default::default()
    })?;
    let mut messages: Vec<ModelMessage> = Vec::new();
    for stored in &page.events {
        let event = &stored.event;
        if event.event_type == EventType::UserInput {
            messages.push(ModelMessage {
                role: "user".to_string(),
                content: extract_user_message(event),
            });
        } else if event.event_type == EventType::ModelResponse {
            messages.push(ModelMessage {
                role: "assistant".to_string(),
                content: extract_assistant_content(event),
            });
        }
    }
    // 限制最大条数
    if messages.len() > max_messages {
        let drop = messages.len() - max_messages;
        messages.drain(0..drop);
    }
    Ok(messages)
}

fn extract_user_message(event: &SessionEvent) -> String {
    event
        .payload_json
        .as_ref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("content").and_then(|c| c.as_str()).map(String::from))
        .unwrap_or_default()
}

fn extract_assistant_content(event: &SessionEvent) -> String {
    event
        .payload_json
        .as_ref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("content").and_then(|c| c.as_str()).map(String::from))
        .unwrap_or_else(|| "(no content)".to_string())
}

/// 应用压缩策略到 messages (P8-1)
///
/// 返压缩后的 messages (也可能返原 slice if policy = Never).
pub fn compress(
    messages: Vec<ModelMessage>,
    policy: CompressionPolicy,
) -> Vec<ModelMessage> {
    match policy {
        CompressionPolicy::Never => messages,
        CompressionPolicy::SlidingWindow { keep_last_n } => {
            if messages.len() <= keep_last_n {
                messages
            } else {
                let drop = messages.len() - keep_last_n;
                messages.into_iter().skip(drop).collect()
            }
        }
        CompressionPolicy::Summarize => {
            // v2 TODO: 调 LLM 摘要
            // v1 fallback: 走 sliding window keep 5
            if messages.len() <= 5 {
                messages
            } else {
                let drop = messages.len() - 5;
                messages.into_iter().skip(drop).collect()
            }
        }
    }
}

/// 检查是否需要压缩 (估 token > max_context_tokens)
pub fn should_compress(messages: &[ModelMessage], max_context_tokens: u32) -> bool {
    estimate_messages_tokens(messages) > max_context_tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::EventLog;

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_ascii() {
        // 16 ASCII chars / 4 = 4 tokens
        let t = estimate_tokens("abcdefghijklmnop");
        assert_eq!(t, 4);
    }

    #[test]
    fn estimate_tokens_cjk() {
        // 6 CJK chars / 1.5 = 4 tokens
        let t = estimate_tokens("中文测试字符");
        assert!(t >= 3 && t <= 5, "CJK 应估 3-5 tokens, got {}", t);
    }

    #[test]
    fn estimate_messages_total() {
        let msgs = vec![
            ModelMessage { role: "user".into(), content: "hello".into() },
            ModelMessage { role: "assistant".into(), content: "world".into() },
        ];
        let t = estimate_messages_tokens(&msgs);
        assert!(t > 0);
    }

    #[test]
    fn compression_never_keeps_all() {
        let msgs: Vec<ModelMessage> = (0..10)
            .map(|i| ModelMessage { role: "user".into(), content: format!("m{}", i) })
            .collect();
        let result = compress(msgs.clone(), CompressionPolicy::Never);
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn compression_sliding_window_drops_oldest() {
        let msgs: Vec<ModelMessage> = (0..10)
            .map(|i| ModelMessage { role: "user".into(), content: format!("m{}", i) })
            .collect();
        let result = compress(msgs, CompressionPolicy::SlidingWindow { keep_last_n: 3 });
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].content, "m7");
        assert_eq!(result[2].content, "m9");
    }

    #[test]
    fn compression_sliding_window_keeps_all_when_under() {
        let msgs: Vec<ModelMessage> = (0..3)
            .map(|i| ModelMessage { role: "user".into(), content: format!("m{}", i) })
            .collect();
        let result = compress(msgs.clone(), CompressionPolicy::SlidingWindow { keep_last_n: 5 });
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn compression_summarize_fallback_to_sliding_5() {
        let msgs: Vec<ModelMessage> = (0..10)
            .map(|i| ModelMessage { role: "user".into(), content: format!("m{}", i) })
            .collect();
        let result = compress(msgs, CompressionPolicy::Summarize);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn should_compress_threshold() {
        let msgs: Vec<ModelMessage> = (0..100)
            .map(|i| ModelMessage { role: "user".into(), content: format!("message {} with some content to make it longer", i) })
            .collect();
        assert!(should_compress(&msgs, 100));
        assert!(!should_compress(&msgs, 100_000));
    }

    #[test]
    fn load_history_empty() {
        let log = EventLog::open_in_memory().unwrap();
        let msgs = load_history_from_log(&log, "nonexistent", 100).unwrap();
        assert_eq!(msgs.len(), 0);
    }

    #[test]
    fn load_history_user_and_assistant() {
        use crate::event::EventType;
        let log = EventLog::open_in_memory().unwrap();
        log.append(
            SessionEvent::new("s1", EventType::UserInput)
                .with_payload(&serde_json::json!({"content": "hi"}))
                .unwrap(),
        );
        log.append(
            SessionEvent::new("s1", EventType::ModelResponse)
                .with_payload(&serde_json::json!({"content": "hello there"}))
                .unwrap(),
        );
        let msgs = load_history_from_log(&log, "s1", 100).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "hi");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "hello there");
    }
}
