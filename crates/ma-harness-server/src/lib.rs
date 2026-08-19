//! # 命名约定
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-server`
//! **Crate ident** (`use` 路径): `ma_harness_server`
//!
//! Rust 自动从 kebab-case package name 推 snake_case crate ident.
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! ma-harness-server = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_server::*;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-server
//!
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
pub mod metrics;
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

    #[tokio::test]
    async fn server_restart_persists_sessions_via_sqlite() {
        // 2026-08-18 (Day 64) Phase 2.10 端到端测试:
        // 1. 用 SqliteStore 写 2 个 session
        // 2. drop ServerBuilder
        // 3. 重新 open 同一 db, 拿 sessions 验证持久化
        use ma_harness_proto::ma_harness::v1::{
            session_service_server::SessionService, CreateSessionRequest, GetSessionRequest,
            ListSessionsRequest, SessionState as ProtoSessionState,
        };
        use tonic::Request;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("sessions.db");

        // 1. 写 2 个 session
        let log1 = EventLog::open_in_memory().unwrap();
        let store1: Arc<dyn SessionStore> = Arc::new(SqliteStore::open(&db_path).unwrap());
        let builder1 = ServerBuilder::with_stub(log1).with_session_store(store1.clone());
        let svc1 = builder1.build_session_service();

        let create_resp1 = svc1
            .create(Request::new(CreateSessionRequest {
                name: "session-A".to_string(),
                mode: 0,
                metadata: None,
                enabled_plugins: vec![],
            }))
            .await
            .unwrap();
        let id_a = create_resp1.into_inner().session.unwrap().id;

        let create_resp2 = svc1
            .create(Request::new(CreateSessionRequest {
                name: "session-B".to_string(),
                mode: 0,
                metadata: None,
                enabled_plugins: vec!["hello".to_string()],
            }))
            .await
            .unwrap();
        let id_b = create_resp2.into_inner().session.unwrap().id;

        // 关闭 svc1
        drop(svc1);
        drop(builder1);
        drop(store1);

        // 2. 重新 open (模拟 server 重启)
        let log2 = EventLog::open_in_memory().unwrap();
        let store2: Arc<dyn SessionStore> = Arc::new(SqliteStore::open(&db_path).unwrap());
        let builder2 = ServerBuilder::with_stub(log2).with_session_store(store2.clone());
        let svc2 = builder2.build_session_service();

        // 3. 验证 2 个 session 都还在
        let list_resp = svc2
            .list(Request::new(ListSessionsRequest {
                page: 1,
                page_size: 100,
                state_filter: 0,
            }))
            .await
            .unwrap();
        let sessions = list_resp.into_inner().sessions;
        assert_eq!(sessions.len(), 2, "重启后 2 个 session 都在");

        // 验证具体 id 跟 name
        let a = svc2
            .get(Request::new(GetSessionRequest { id: id_a.clone() }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(a.name, "session-A");
        assert_eq!(a.state, ProtoSessionState::Created as i32);

        let b = svc2
            .get(Request::new(GetSessionRequest { id: id_b.clone() }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(b.name, "session-B");
        assert_eq!(b.enabled_plugins, vec!["hello".to_string()]);
    }
}
