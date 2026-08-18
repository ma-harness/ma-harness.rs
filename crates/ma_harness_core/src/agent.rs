//! AgentLoop — Default 模式 agent loop 骨架
//!
//! Week 1 Day 8 实现. 设计见 `docs/ma-harness-arch-map.md` §5.
//!
//! # 流程
//!
//! 1. 接收 AgentRunRequest
//! 2. emit RunStart
//! 3. 构造 ModelRequest, emit ModelRequest
//! 4. 调 ModelAdapter::complete()
//! 5. emit ModelResponse
//! 6. 终止 (finish_reason=stop)
//! 7. emit RunEnd
//!
//! Phase 1 不支持 tool_call 循环 (no tools), finish_reason 总是 stop.
//! Phase 2 加: tool_call 循环 + multi-iteration.

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use ma_harness_cordis::Context;

use crate::event::{EventType, SessionEvent, Severity};
use crate::log::EventLog;

/// ModelRequest — 调 LLM 之前的请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequest {
    /// 模型 ID
    pub model: String,
    /// 消息列表
    pub messages: Vec<ModelMessage>,
    /// 温度
    pub temperature: f32,
    /// max_tokens
    pub max_tokens: u32,
    /// system prompt
    pub system_prompt: Option<String>,
}

/// ModelMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMessage {
    /// 角色 ("user" / "assistant" / "system")
    pub role: String,
    /// 内容
    pub content: String,
}

/// ModelResponse — LLM 返回
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    /// 模型 ID (跟请求一致)
    pub model: String,
    /// 助手回复内容
    pub content: String,
    /// 停止原因
    pub finish_reason: FinishReason,
    /// prompt tokens
    pub prompt_tokens: u32,
    /// completion tokens
    pub completion_tokens: u32,
}

/// FinishReason
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    /// 正常完成
    Stop,
    /// 超过 max_tokens
    Length,
    /// 要求工具调用 (Phase 2 才用)
    ToolCalls,
    /// 内容过滤
    ContentFilter,
    /// 错误
    Error,
}

/// ModelAdapter trait — 抽象 LLM API
///
/// Phase 1 stub: 返回 zero response. Phase 2 加 OpenAI / Anthropic 实现.
#[async_trait]
pub trait ModelAdapter: Send + Sync + 'static {
    /// 唯一标识 (例 "openai" / "anthropic" / "stub")
    fn name(&self) -> &str;

    /// 同步调用 (Phase 1), 返回响应或错误
    async fn complete(&self, req: &ModelRequest) -> anyhow::Result<ModelResponse>;
}

/// StubModelAdapter — Phase 1 默认, 返回零响应
pub struct StubModelAdapter;

#[async_trait]
impl ModelAdapter for StubModelAdapter {
    fn name(&self) -> &str {
        "stub"
    }

    async fn complete(&self, req: &ModelRequest) -> anyhow::Result<ModelResponse> {
        // 简单 echo: 返回 "stub response to: <last user message>"
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("(no user message)");

        Ok(ModelResponse {
            model: req.model.clone(),
            content: format!("[stub] echo: {}", last_user),
            finish_reason: FinishReason::Stop,
            prompt_tokens: req.messages.len() as u32 * 10,
            completion_tokens: 20,
        })
    }
}

/// AgentRunRequest — 入口
#[derive(Debug, Clone)]
pub struct AgentRunRequest {
    /// 必填
    pub session_id: String,
    /// 用户消息
    pub user_message: String,
    /// 模型 (默认 "stub")
    pub model: String,
    /// 温度
    pub temperature: f32,
    /// max_tokens
    pub max_tokens: u32,
    /// system prompt
    pub system_prompt: Option<String>,
}

/// AgentRunResponse — 出口
#[derive(Debug, Clone)]
pub struct AgentRunResponse {
    /// session_id
    pub session_id: String,
    /// run_id (本 run 的 UUID)
    pub run_id: String,
    /// 模型回复
    pub model_response: ModelResponse,
    /// 整个 run 累计 token
    pub total_prompt_tokens: u32,
    pub total_completion_tokens: u32,
}

/// AgentLoop 主结构
pub struct AgentLoop {
    log: EventLog,
    adapter: Arc<dyn ModelAdapter>,
    /// 可选 ctx 引用 (Phase 2 用来拿 service / 注入 model adapter)
    _ctx_marker: Mutex<Option<Arc<Context>>>,
}

impl std::fmt::Debug for AgentLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentLoop")
            .field("log", &self.log)
            .field("adapter", &self.adapter.name())
            .finish()
    }
}

impl AgentLoop {
    /// 构造
    pub fn new(log: EventLog, adapter: Arc<dyn ModelAdapter>) -> Self {
        Self {
            log,
            adapter,
            _ctx_marker: Mutex::new(None),
        }
    }

    /// 用 stub adapter 构造 (Phase 1 便利)
    pub fn with_stub(log: EventLog) -> Self {
        Self::new(log, Arc::new(StubModelAdapter))
    }

    /// 关联一个 ctx (Phase 2 用, Phase 1 保留接口)
    pub fn with_ctx(self, ctx: Arc<Context>) -> Self {
        *self._ctx_marker.lock() = Some(ctx);
        self
    }

    /// 跑一次 agent run
    pub async fn run(&self, req: AgentRunRequest) -> anyhow::Result<AgentRunResponse> {
        // 1. RunStart
        let run_id = uuid::Uuid::new_v4().to_string();
        self.log.append(
            SessionEvent::new(&req.session_id, EventType::RunStart)
                .with_run_id(&run_id)
                .with_payload(&serde_json::json!({
                    "model": req.model,
                    "temperature": req.temperature,
                    "max_tokens": req.max_tokens,
                }))?,
        );

        // 2. 构造 ModelRequest
        let model_req = ModelRequest {
            model: req.model.clone(),
            messages: vec![ModelMessage {
                role: "user".to_string(),
                content: req.user_message.clone(),
            }],
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            system_prompt: req.system_prompt.clone(),
        };

        // 3. ModelRequest 事件
        self.log.append(
            SessionEvent::new(&req.session_id, EventType::ModelRequest)
                .with_run_id(&run_id)
                .with_payload(&serde_json::json!({
                    "model": model_req.model,
                    "messages": model_req.messages.len(),
                }))?,
        );

        // 4. 调 adapter
        let model_resp = match self.adapter.complete(&model_req).await {
            Ok(r) => r,
            Err(e) => {
                // 错误: emit ModelError (不 model_visible), 失败
                self.log.append(
                    SessionEvent::new(&req.session_id, EventType::ModelError)
                        .with_run_id(&run_id)
                        .with_severity(Severity::Error)
                        .with_error(format!("{}", e))
                        .with_payload(&serde_json::json!({
                            "model": model_req.model,
                        }))?,
                );
                return Err(e);
            }
        };

        // 5. ModelResponse 事件
        self.log.append(
            SessionEvent::new(&req.session_id, EventType::ModelResponse)
                .with_run_id(&run_id)
                .with_payload(&serde_json::json!({
                    "model": model_resp.model,
                    "content_length": model_resp.content.len(),
                    "finish_reason": model_resp.finish_reason,
                    "prompt_tokens": model_resp.prompt_tokens,
                    "completion_tokens": model_resp.completion_tokens,
                }))?,
        );

        // 6. RunEnd
        self.log.append(
            SessionEvent::new(&req.session_id, EventType::RunEnd)
                .with_run_id(&run_id)
                .with_payload(&serde_json::json!({
                    "finish_reason": model_resp.finish_reason,
                }))?,
        );

        // 7. 返回
        Ok(AgentRunResponse {
            session_id: req.session_id,
            run_id,
            total_prompt_tokens: model_resp.prompt_tokens,
            total_completion_tokens: model_resp.completion_tokens,
            model_response: model_resp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::EventLog;

    #[tokio::test]
    async fn stub_adapter_run_end_to_end() {
        let log = EventLog::open_in_memory().unwrap();
        let agent = AgentLoop::with_stub(log.clone());

        let req = AgentRunRequest {
            session_id: "s1".to_string(),
            user_message: "hello".to_string(),
            model: "stub".to_string(),
            temperature: 0.7,
            max_tokens: 1024,
            system_prompt: None,
        };

        let resp = agent.run(req).await.unwrap();
        assert_eq!(resp.session_id, "s1");
        assert!(!resp.run_id.is_empty());
        assert!(resp.model_response.content.contains("hello"));
        assert_eq!(resp.model_response.finish_reason, FinishReason::Stop);
        assert!(resp.total_prompt_tokens > 0);
        assert!(resp.total_completion_tokens > 0);

        // 日志应该有 4 个事件: RunStart, ModelRequest, ModelResponse, RunEnd
        let page = log.get_model_visible("s1").unwrap();
        assert_eq!(page.events.len(), 4, "应该 4 个 model-visible 事件");
        assert_eq!(page.events[0].event.event_type, EventType::RunStart);
        assert_eq!(page.events[1].event.event_type, EventType::ModelRequest);
        assert_eq!(page.events[2].event.event_type, EventType::ModelResponse);
        assert_eq!(page.events[3].event.event_type, EventType::RunEnd);
    }

    #[tokio::test]
    async fn run_with_error_emits_model_error() {
        // 构造一个总是 fail 的 adapter
        struct FailAdapter;
        #[async_trait]
        impl ModelAdapter for FailAdapter {
            fn name(&self) -> &str { "fail" }
            async fn complete(&self, _req: &ModelRequest) -> anyhow::Result<ModelResponse> {
                Err(anyhow::anyhow!("intentional fail"))
            }
        }

        let log = EventLog::open_in_memory().unwrap();
        let agent = AgentLoop::new(log.clone(), Arc::new(FailAdapter));

        let req = AgentRunRequest {
            session_id: "s1".to_string(),
            user_message: "x".to_string(),
            model: "fail".to_string(),
            temperature: 0.0,
            max_tokens: 100,
            system_prompt: None,
        };

        let result = agent.run(req).await;
        assert!(result.is_err());

        // 日志应该有 3 个事件: RunStart + ModelRequest + ModelError
        // (ModelRequest 永远先于 complete(), 即使 complete() 失败)
        // RunEnd 没 emit (错误路径早返回)
        let count = log.count("s1").unwrap();
        assert_eq!(count, 3, "error path 应 emit RunStart + ModelRequest + ModelError");

        let all = log.query(&crate::log::EventQuery {
            session_id: "s1".to_string(),
            ..Default::default()
        }).unwrap();
        assert_eq!(all.events[0].event.event_type, EventType::RunStart);
        assert_eq!(all.events[1].event.event_type, EventType::ModelRequest);
        assert_eq!(all.events[2].event.event_type, EventType::ModelError);
        assert!(!all.events[2].event.model_visible, "ModelError 不是 model-visible");
    }

    #[tokio::test]
    async fn run_seq_is_monotonic() {
        let log = EventLog::open_in_memory().unwrap();
        let agent = AgentLoop::with_stub(log.clone());

        for i in 0..3 {
            let req = AgentRunRequest {
                session_id: "s1".to_string(),
                user_message: format!("msg {}", i),
                model: "stub".to_string(),
                temperature: 0.0,
                max_tokens: 100,
                system_prompt: None,
            };
            agent.run(req).await.unwrap();
        }

        let all = log.query(&crate::log::EventQuery {
            session_id: "s1".to_string(),
            ..Default::default()
        }).unwrap();
        // 3 runs × 4 events = 12 个
        assert_eq!(all.events.len(), 12);
        // seq 单调递增
        for i in 1..all.events.len() {
            assert!(all.events[i].seq > all.events[i - 1].seq);
        }
    }
}
