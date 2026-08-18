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
        final_state: ProtoAgentState::AgentStateFinished as i32,
        finish_reason: match r.model_response.finish_reason {
            ma_harness_core::FinishReason::Stop => ProtoFinishReason::FinishReasonStop as i32,
            ma_harness_core::FinishReason::Length => ProtoFinishReason::FinishReasonLength as i32,
            ma_harness_core::FinishReason::ToolCalls => {
                ProtoFinishReason::FinishReasonToolCalls as i32
            }
            ma_harness_core::FinishReason::ContentFilter => {
                ProtoFinishReason::FinishReasonContentFilter as i32
            }
            ma_harness_core::FinishReason::Error => ProtoFinishReason::FinishReasonError as i32,
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
        let system_prompt = model_config.and_then(|c| c.system_prompt.clone());

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
        _request: Request<ma_harness_proto::ma_harness::v1::AgentRunRequest>,
    ) -> Result<Response<RunStream>, Status> {
        Err(Status::unimplemented("RunStream 留 Phase 2"))
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
                role: ma_harness_proto::ma_harness::v1::ToolRole::ToolRoleUser as i32,
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
                adapter: ma_harness_proto::ma_harness::v1::ModelAdapter::ModelAdapterOpenai as i32,
                model: "stub".to_string(),
                temperature: 0.0,
                max_tokens: 100,
                system_prompt: None,
            }),
            options: None,
        };

        let result = svc
            .run(Request::new(proto_req))
            .await
            .unwrap();
        let resp = result.into_inner();
        assert!(!resp.run_id.is_empty());
        assert_eq!(resp.final_state, ProtoAgentState::AgentStateFinished as i32);

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
                role: ma_harness_proto::ma_harness::v1::ToolRole::ToolRoleUser as i32,
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
}
