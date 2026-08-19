//! AgentServiceImpl — gRPC AgentService 实现 (走 ma_harness_proto)
//!
//! Week 1 Day 18 实现: 同步 Run / GetRun 接口 (RunStream Phase 2).

use std::pin::Pin;
use std::sync::Arc;

use ma_harness_core::{AgentLoop, AgentRunRequest, AgentRunResponse, EventLog, ModelAdapter};
use ma_harness_proto::ma_harness::v1::{
    agent_service_server::AgentService, AgentRunResponse as ProtoAgentRunResponse,
    AgentState as ProtoAgentState, CancelRequest, CancelResponse, FinishReason as ProtoFinishReason,
    GetRunRequest,
};
use ma_harness_proto::convert::session_event_to_proto;
use tonic::{Request, Response, Status};

/// 内部 SessionEvent → proto::SessionEvent helper (server 用)
fn run_response_to_proto(r: AgentRunResponse) -> ProtoAgentRunResponse {
    ProtoAgentRunResponse {
        run_id: r.run_id,
        final_state: ProtoAgentState::Finished as i32,
        finish_reason: match r.model_response.finish_reason {
            ma_harness_core::FinishReason::Stop => ProtoFinishReason::Stop as i32,
            ma_harness_core::FinishReason::Length => ProtoFinishReason::Length as i32,
            ma_harness_core::FinishReason::ToolCalls => {
                ProtoFinishReason::ToolCalls as i32
            }
            ma_harness_core::FinishReason::ContentFilter => {
                ProtoFinishReason::ContentFilter as i32
            }
            ma_harness_core::FinishReason::Error => ProtoFinishReason::Error as i32,
        },
        messages: vec![], // Phase 1 不返 messages 列表
        session_id: r.session_id,
        started_at: None, // Phase 1 简化, 不填 ts
        finished_at: None,
        prompt_tokens: r.total_prompt_tokens,
        completion_tokens: r.total_completion_tokens,
        total_tokens: r.total_prompt_tokens + r.total_completion_tokens,
    }
}

/// AgentServiceImpl — 持有 EventLog + ModelAdapter
pub struct AgentServiceImpl {
    log: EventLog,
    adapter: Arc<dyn ModelAdapter>,
    // Phase 1 内存状态, Phase 2 接 SessionService
    runs: dashmap::DashMap<String, ProtoAgentRunResponse>,
}

impl AgentServiceImpl {
    pub fn new(log: EventLog, adapter: Arc<dyn ModelAdapter>) -> Self {
        Self {
            log,
            adapter,
            runs: dashmap::DashMap::new(),
        }
    }
}

type RunStream = Pin<Box<dyn futures::Stream<Item = Result<ma_harness_proto::ma_harness::v1::AgentStreamEvent, Status>> + Send>>;

#[tonic::async_trait]
impl AgentService for AgentServiceImpl {
    type RunStreamStream = RunStream;


    async fn run(
        &self,
        request: Request<ma_harness_proto::ma_harness::v1::AgentRunRequest>,
    ) -> Result<Response<ProtoAgentRunResponse>, Status> {
        let proto_req = request.into_inner();
        let session_id = proto_req.session_id.clone();

        // 构造 core AgentRunRequest (从 proto 简化, Phase 2 完整 convert)
        let user_message = proto_req
            .input
            .as_ref()
            .and_then(|m| {
                m.content
                    .first()
                    .and_then(|cb| cb.content.as_ref())
                    .and_then(|c| match c {
                        ma_harness_proto::ma_harness::v1::content_block::Content::Text(t) => {
                            Some(t.text.clone())
                        }
                        _ => None,
                    })
            })
            .ok_or_else(|| Status::invalid_argument("input.content[0] 必须有 text"))?;

        let model_config = proto_req.model_config.as_ref();
        let model = model_config
            .map(|c| c.model.clone())
            .unwrap_or_else(|| "stub".to_string());
        let temperature = model_config.map(|c| c.temperature).unwrap_or(0.7);
        let max_tokens = model_config.map(|c| c.max_tokens).unwrap_or(1024);
        let system_prompt = model_config.map(|c| c.system_prompt.clone());

        let core_req = AgentRunRequest {
            session_id: session_id.clone(),
            user_message,
            model,
            temperature,
            max_tokens,
            system_prompt,
        };

        // 构造 AgentLoop 跑
        let agent = AgentLoop::new(self.log.clone(), self.adapter.clone());
        let result = agent
            .run(core_req)
            .await
            .map_err(|e| Status::internal(format!("agent run failed: {}", e)))?;

        // 缓存 + 返
        let proto_resp = run_response_to_proto(result);
        self.runs.insert(proto_resp.run_id.clone(), proto_resp.clone());
        Ok(Response::new(proto_resp))
    }

    async fn run_stream(
        &self,
        request: Request<ma_harness_proto::ma_harness::v1::AgentRunRequest>,
    ) -> Result<Response<Self::RunStreamStream>, Status> {
        use futures::StreamExt;
        use ma_harness_core::ModelRequest;
        use ma_harness_proto::ma_harness::v1::{
            agent_stream_event::Event as StreamEvent, content_block::Content, AgentStreamEvent,
            ContentBlock, Message, TextBlock, ToolRole,
        };

        let proto_req = request.into_inner();
        let run_id = uuid::Uuid::new_v4().to_string();
        let session_id = proto_req.session_id.clone();

        // 1. 构造 ModelRequest (跟 run() 同样的逻辑简化版)
        let user_message = proto_req
            .input
            .as_ref()
            .and_then(|m| {
                m.content.first().and_then(|cb| cb.content.as_ref()).and_then(|c| {
                    match c {
                        Content::Text(t) => Some(t.text.clone()),
                        _ => None,
                    }
                })
            })
            .unwrap_or_default();
        let model_config = proto_req.model_config.as_ref();
        let model = model_config
            .map(|c| c.model.clone())
            .unwrap_or_else(|| "stub".to_string());
        let temperature = model_config.map(|c| c.temperature).unwrap_or(0.7);
        let max_tokens = model_config.map(|c| c.max_tokens).unwrap_or(1024);
        let system_prompt = model_config
            .map(|c| c.system_prompt.clone())
            .filter(|s| !s.is_empty());

        // 2. 调 adapter.complete_stream (走 default impl = complete 单 chunk yield)
        //    用 tokio::task::spawn_blocking 把同步 stream 移到 blocking thread pool
        //    因为 complete_stream 内部是 sync stream (不是 async, 来自 async_stream::stream!)
        let adapter = self.adapter.clone();
        let run_id_for_stream = run_id.clone();
        let event_stream = async_stream::try_stream! {
            // 把 ModelRequest 构造到 stream 里 (不超出 stack)
            let model_req = ModelRequest {
                model,
                messages: vec![ma_harness_core::ModelMessage {
                    role: "user".to_string(),
                    content: user_message,
                }],
                temperature,
                max_tokens,
                system_prompt,
            };

            // 调 complete_stream (返回 `Pin<Box<dyn Stream + Send + 'a>>`, 'a 绑 req/self 生命周期)
            // 我们手动 pin_mut 拿完整生命周期
            let token_stream = adapter.complete_stream(&model_req);
            futures::pin_mut!(token_stream);

            while let Some(token) = token_stream.next().await {
                let now = prost_types::Timestamp::from(std::time::SystemTime::now());
                let msg = Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    role: ToolRole::Assistant as i32,
                    content: vec![ContentBlock {
                        content: Some(Content::Text(TextBlock { text: token })),
                    }],
                    created_at: Some(now),
                    session_id: session_id.clone(),
                };
                yield AgentStreamEvent {
                    run_id: run_id_for_stream.clone(),
                    event: Some(StreamEvent::Message(msg)),
                };
            }
        };

        let pinned: Self::RunStreamStream = Box::pin(event_stream);
        Ok(Response::new(pinned))
    }

    async fn cancel(&self, _request: Request<CancelRequest>) -> Result<Response<CancelResponse>, Status> {
        Err(Status::unimplemented("Cancel 留 Phase 2"))
    }

    async fn get_run(
        &self,
        request: Request<GetRunRequest>,
    ) -> Result<Response<ProtoAgentRunResponse>, Status> {
        let run_id = request.into_inner().run_id;
        self.runs
            .get(&run_id)
            .map(|entry| Response::new(entry.value().clone()))
            .ok_or_else(|| Status::not_found(format!("run not found: {}", run_id)))
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_core::EventType;

    #[tokio::test]
    async fn agent_service_run_with_stub() {
        let log = EventLog::open_in_memory().unwrap();
        let svc = AgentServiceImpl::new(log.clone(), Arc::new(ma_harness_core::StubModelAdapter));

        let proto_req = ma_harness_proto::ma_harness::v1::AgentRunRequest {
            session_id: "s1".to_string(),
            input: Some(ma_harness_proto::ma_harness::v1::Message {
                id: "m1".to_string(),
                role: ma_harness_proto::ma_harness::v1::ToolRole::User as i32,
                content: vec![ma_harness_proto::ma_harness::v1::ContentBlock {
                    content: Some(
                        ma_harness_proto::ma_harness::v1::content_block::Content::Text(
                            ma_harness_proto::ma_harness::v1::TextBlock {
                                text: "hi".to_string(),
                            },
                        ),
                    ),
                }],
                created_at: None,
                session_id: "s1".to_string(),
            }),
            model_config: Some(ma_harness_proto::ma_harness::v1::ModelConfig {
                adapter: ma_harness_proto::ma_harness::v1::ModelAdapter::Openai as i32,
                model: "stub".to_string(),
                temperature: 0.0,
                max_tokens: 100,
                system_prompt: "".to_string(),
            }),
            options: None,
        };

        let result = svc
            .run(Request::new(proto_req))
            .await
            .unwrap();
        let resp = result.into_inner();
        assert!(!resp.run_id.is_empty());
        assert_eq!(resp.final_state, ProtoAgentState::Finished as i32);

        // 日志验证: 4 个 model-visible 事件
        let page = log.get_model_visible("s1").unwrap();
        assert_eq!(page.events.len(), 4);
        assert_eq!(page.events[0].event.event_type, EventType::RunStart);
    }

    #[tokio::test]
    async fn agent_service_get_run_caches() {
        let log = EventLog::open_in_memory().unwrap();
        let svc = AgentServiceImpl::new(log, Arc::new(ma_harness_core::StubModelAdapter));

        // 跑一次
        let proto_req = ma_harness_proto::ma_harness::v1::AgentRunRequest {
            session_id: "s1".to_string(),
            input: Some(ma_harness_proto::ma_harness::v1::Message {
                id: "m1".to_string(),
                role: ma_harness_proto::ma_harness::v1::ToolRole::User as i32,
                content: vec![ma_harness_proto::ma_harness::v1::ContentBlock {
                    content: Some(
                        ma_harness_proto::ma_harness::v1::content_block::Content::Text(
                            ma_harness_proto::ma_harness::v1::TextBlock {
                                text: "x".to_string(),
                            },
                        ),
                    ),
                }],
                created_at: None,
                session_id: "s1".to_string(),
            }),
            model_config: None,
            options: None,
        };

        let resp = svc.run(Request::new(proto_req)).await.unwrap();
        let run_id = resp.into_inner().run_id;

        // get_run 拿得到
        let get_req = GetRunRequest { run_id: run_id.clone() };
        let get_resp = svc.get_run(Request::new(get_req)).await.unwrap();
        assert_eq!(get_resp.into_inner().run_id, run_id);
    }

    #[tokio::test]
    async fn agent_service_get_run_not_found() {
        let log = EventLog::open_in_memory().unwrap();
        let svc = AgentServiceImpl::new(log, Arc::new(ma_harness_core::StubModelAdapter));
        let result = svc
            .get_run(Request::new(GetRunRequest {
                run_id: "nonexistent".to_string(),
            }))
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
    }

    // === P5-6 (Day 95): RunStream RPC ===

    /// RunStream RPC 走 StubModelAdapter, 验证 word-by-word streaming
    /// (StubModelAdapter::complete_stream 把 user message 拆成 word 依次 yield)
    #[tokio::test]
    async fn run_stream_yields_words_via_stub() {
        use futures::StreamExt;
        use ma_harness_core::StubModelAdapter;
        use ma_harness_proto::ma_harness::v1::{
            agent_stream_event::Event, AgentRunRequest, ModelConfig, ContentBlock,
            content_block::Content, Message, TextBlock, ToolRole,
        };

        let log = EventLog::open_in_memory().unwrap();
        let svc = AgentServiceImpl::new(log, Arc::new(StubModelAdapter));

        // 构造 AgentRunRequest, user message 是 3 个 word
        let req = AgentRunRequest {
            session_id: "stream-test".to_string(),
            input: Some(Message {
                id: "m1".to_string(),
                role: ToolRole::User as i32,
                content: vec![ContentBlock {
                    content: Some(Content::Text(TextBlock {
                        text: "alpha beta gamma".to_string(),
                    })),
                }],
                created_at: None,
                session_id: "stream-test".to_string(),
            }),
            model_config: Some(ModelConfig {
                adapter: 0,  // ModelAdapter::Unspecified, 不影响
                model: "stub".to_string(),
                temperature: 0.0,
                max_tokens: 100,
                system_prompt: "".to_string(),
            }),
            options: None,
        };

        let mut stream = svc
            .run_stream(Request::new(req))
            .await
            .unwrap()
            .into_inner();
        let mut collected = Vec::new();
        while let Some(event) = stream.next().await {
            let event = event.unwrap();
            match event.event {
                Some(Event::Message(msg)) => {
                    if let Some(ContentBlock {
                        content: Some(Content::Text(t)),
                    }) = msg.content.first()
                    {
                        collected.push(t.text.clone());
                    }
                }
                _ => {}
            }
        }
        // 3 个 word: "alpha ", "beta ", "gamma "
        assert_eq!(collected.len(), 3, "3 words expected, got {:?}", collected);
        assert_eq!(collected[0], "alpha ");
        assert_eq!(collected[1], "beta ");
        assert_eq!(collected[2], "gamma ");
    }
}
