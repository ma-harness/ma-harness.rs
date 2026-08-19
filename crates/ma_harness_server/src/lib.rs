//! ma_harness_server — 服务层 (salvo + tonic 拼装, 内部 crate)
//!
//! **内部 crate** (2026-08-18 锁定). salvo + tonic 拼装, 频繁变.
//! Week 7-9 起, 把 `ma_harness_seam` 的 5 个 registry 暴露成 gRPC service + HTTP endpoint.
//!
//! 2026-08-18 (Day 52): 恢复 ma_harness_proto (用本地 vendor/protoc), 恢复 gRPC service
//! 模块 (agent_service / session_service), 恢复 start_server 真实起 gRPC + HTTP
//!
//! 2026-08-18 (Day 60): 加 session_store 模块 (InMemory + Sqlite), Phase 2.6 持久化

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use ma_harness_cordis::Context;
use ma_harness_core::{EventLog, ModelAdapter, StubModelAdapter};

pub mod agent_service;
pub mod http;
pub mod session_service;
pub mod session_store;

pub use agent_service::AgentServiceImpl;
pub use session_service::SessionServiceImpl;
pub use session_store::{
    default_store, DefaultSessionStore, InMemoryStore, SessionStore, SessionStoreError,
    SqliteStore,
};

/// ServerBuilder — 拼装 server 所需的全部资源
pub struct ServerBuilder {
    /// 事件日志 (Phase 2 持久化)
    #[allow(dead_code)]
    log: EventLog,
    /// Model adapter (Phase 1 stub, Phase 2 OpenAI)
    adapter: Arc<dyn ModelAdapter>,
    /// Session store (Phase 1: InMemory, Phase 2.6: Sqlite 默认)
    sessions: Arc<dyn SessionStore>,
}

impl ServerBuilder {
    /// 用 stub adapter + 内存 session store 构造 (Phase 1 默认)
    pub fn with_stub(log: EventLog) -> Self {
        Self {
            log,
            adapter: Arc::new(StubModelAdapter),
            sessions: default_store(),
        }
    }

    /// 注入自定义 ModelAdapter
    pub fn with_adapter(mut self, adapter: Arc<dyn ModelAdapter>) -> Self {
        self.adapter = adapter;
        self
    }

    /// 注入自定义 SessionStore (Phase 2.6: 业务方要持久化用 .with_session_store(Arc::new(SqliteStore::open("path.db")?)))
    pub fn with_session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.sessions = store;
        self
    }

    /// 构造 AgentServiceImpl
    pub fn build_agent_service(&self) -> AgentServiceImpl {
        AgentServiceImpl::new(self.log.clone(), self.adapter.clone())
    }

    /// 构造 SessionServiceImpl (Phase 2.6 接 store trait)
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
        use ma_harness_core::{FinishReason, ModelRequest, ModelResponse};

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
        drop(agent);
    }

    #[test]
    fn server_builder_with_sqlite_store() {
        // 验 ServerBuilder 接 SqliteStore 走通
        let log = EventLog::open_in_memory().unwrap();
        let store: Arc<dyn SessionStore> = Arc::new(SqliteStore::open_in_memory().unwrap());
        let builder = ServerBuilder::with_stub(log).with_session_store(store);
        let _session = builder.build_session_service();
    }
}
