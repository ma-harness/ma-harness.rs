//! ma_harness_server — 服务层 (axum + tonic 拼装, 内部 crate)
//!
//! **内部 crate** (2026-08-18 锁定). axum + tonic 拼装, 频繁变.
//! Week 7-9 起, 把 `ma_harness_seam` 的 5 个 registry 暴露成 gRPC service + HTTP endpoint.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use ma_harness_cordis::Context;
use ma_harness_core::{AgentLoop, EventLog, ModelAdapter, StubModelAdapter};

pub mod agent_service;
pub mod session_service;
pub mod http;

pub use agent_service::AgentServiceImpl;
pub use session_service::SessionServiceImpl;

/// ServerBuilder — 拼装 server 所需的全部资源
pub struct ServerBuilder {
    log: EventLog,
    adapter: Arc<dyn ModelAdapter>,
    sessions: dashmap::DashMap<String, ma_harness_core::SessionEvent>, // session_id -> 创建事件
}

impl ServerBuilder {
    /// 用 stub adapter 构造 (Phase 1 默认)
    pub fn with_stub(log: EventLog) -> Self {
        Self {
            log,
            adapter: Arc::new(StubModelAdapter),
            sessions: dashmap::DashMap::new(),
        }
    }

    /// 注入自定义 ModelAdapter
    pub fn with_adapter(mut self, adapter: Arc<dyn ModelAdapter>) -> Self {
        self.adapter = adapter;
        self
    }

    /// 构造 AgentServiceImpl
    pub fn build_agent_service(&self) -> AgentServiceImpl {
        AgentServiceImpl::new(self.log.clone(), self.adapter.clone())
    }

    /// 构造 SessionServiceImpl
    pub fn build_session_service(&self) -> SessionServiceImpl {
        SessionServiceImpl::new(self.sessions.clone())
    }

    /// 构造一个完整 ctx (Phase 1 占位, Week 5-6 加 plugin 装载)
    pub fn build_ctx(&self) -> Context {
        Context::new()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_builder_with_stub() {
        let log = EventLog::open_in_memory().unwrap();
        let builder = ServerBuilder::with_stub(log);
        let _agent = builder.build_agent_service();
        let _session = builder.build_session_service();
        let _ctx = builder.build_ctx();
    }

    #[test]
    fn server_builder_with_custom_adapter() {
        // 构造一个 echo adapter 测 with_adapter
        use async_trait::async_trait;
        use ma_harness_core::{ModelRequest, ModelResponse, FinishReason};

        struct EchoAdapter;
        #[async_trait]
        impl ModelAdapter for EchoAdapter {
            fn name(&self) -> &str {
                "echo"
            }
            async fn complete(
                &self,
                req: &ModelRequest,
            ) -> anyhow::Result<ModelResponse> {
                Ok(ModelResponse {
                    model: req.model.clone(),
                    content: "echo".to_string(),
                    finish_reason: FinishReason::Stop,
                    prompt_tokens: 1,
                    completion_tokens: 1,
                })
            }
        }

        let log = EventLog::open_in_memory().unwrap();
        let builder = ServerBuilder::with_stub(log).with_adapter(Arc::new(EchoAdapter));
        let agent = builder.build_agent_service();
        // Smoke test
        drop(agent);
    }
}
