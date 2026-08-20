//! SessionServiceImpl — gRPC SessionService 实现 (Phase 2.6 接 SessionStore trait)
//!
//! Week 1 Day 18 实现, Phase 2.6 改成接 store trait (InMemory / Sqlite 两套都支持).
//! 用 `Arc<dyn SessionStore>` 拿 store, 业务方在 ServerBuilder 注入.

use std::sync::Arc;

use ma_harness_proto::ma_harness::v1::{
    session_service_server::SessionService, CloseSessionRequest, CloseSessionResponse,
    CreateSessionRequest, CreateSessionResponse, GetSessionRequest, ListSessionsRequest,
    ListSessionsResponse, Session as ProtoSession, SessionState as ProtoSessionState,
};
use tonic::{Request, Response, Status};

use crate::session_store::SessionStore;

/// SessionServiceImpl — store-backed session CRUD
pub struct SessionServiceImpl {
    store: Arc<dyn SessionStore>,
}

impl SessionServiceImpl {
    /// 构造 (接 store trait)
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self { store }
    }

    // ============================================================================
    // 公开的非 gRPC 方法 (Phase 5.1 / Day 90: HTTP /v1/sessions handler 用)
    // 跟 gRPC trait impl 共享 store, 逻辑等价但返 Result 不返 Status
    // ============================================================================

    /// 列出所有 session (等价 gRPC `list`)
    pub fn list_sessions(&self) -> Result<Vec<ProtoSession>, String> {
        self.store
            .list()
            .map_err(|e| format!("session store list: {e}"))
    }

    /// 拿单个 session (等价 gRPC `get`)
    pub fn get_session(&self, id: &str) -> Result<Option<ProtoSession>, String> {
        self.store
            .get(id)
            .map_err(|e| format!("session store get: {e}"))
    }

    /// 创建 session (等价 gRPC `create`)
    pub fn create_session(&self, session: ProtoSession) -> Result<(), String> {
        self.store
            .create(&session)
            .map_err(|e| format!("session store create: {e}"))
    }

    /// 关闭 session (等价 gRPC `close`, 默认 final_state=Closed)
    pub fn close_session(&self, id: &str) -> Result<Option<ProtoSession>, String> {
        let mut session = self
            .store
            .get(id)
            .map_err(|e| format!("session store get: {e}"))?
            .ok_or_else(|| format!("session not found: {id}"))?;
        session.state = ProtoSessionState::Closed as i32;
        session.closed_at = Some(prost_types::Timestamp::from(std::time::SystemTime::now()));
        session.updated_at = Some(prost_types::Timestamp::from(std::time::SystemTime::now()));
        self.store
            .update(&session)
            .map_err(|e| format!("session store update: {e}"))?;
        Ok(Some(session))
    }
}

#[tonic::async_trait]
impl SessionService for SessionServiceImpl {
    async fn create(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let req = request.into_inner();
        let id = uuid::Uuid::new_v4().to_string();
        let name = if req.name.is_empty() {
            format!("session-{}", &id[..8])
        } else {
            req.name.clone()
        };

        let session = ProtoSession {
            id: id.clone(),
            name,
            state: ProtoSessionState::Created as i32,
            mode: req.mode,
            created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            closed_at: None,
            metadata: req.metadata,
            stats: None,
            enabled_plugins: req.enabled_plugins,
            user_id: String::new(),
        };
        self.store
            .create(&session)
            .map_err(|e| Status::internal(format!("session store create: {e}")))?;
        Ok(Response::new(CreateSessionResponse {
            session: Some(session),
        }))
    }

    async fn get(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<ProtoSession>, Status> {
        let id = request.into_inner().id;
        self.store
            .get(&id)
            .map_err(|e| Status::internal(format!("session store get: {e}")))?
            .map(Response::new)
            .ok_or_else(|| Status::not_found(format!("session not found: {}", id)))
    }

    async fn list(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let _req = request.into_inner();
        let sessions = self
            .store
            .list()
            .map_err(|e| Status::internal(format!("session store list: {e}")))?;
        let total = sessions.len() as u32;
        Ok(Response::new(ListSessionsResponse {
            sessions,
            total,
            page: 1,
            page_size: total,
        }))
    }

    async fn close(
        &self,
        request: Request<CloseSessionRequest>,
    ) -> Result<Response<CloseSessionResponse>, Status> {
        let req = request.into_inner();
        let id = req.id;
        let final_state = if req.final_state == 0 {
            ProtoSessionState::Closed as i32
        } else {
            req.final_state
        };
        let mut session = self
            .store
            .get(&id)
            .map_err(|e| Status::internal(format!("session store get: {e}")))?
            .ok_or_else(|| Status::not_found(format!("session not found: {}", id)))?;
        session.state = final_state;
        session.closed_at = Some(prost_types::Timestamp::from(std::time::SystemTime::now()));
        session.updated_at = Some(prost_types::Timestamp::from(std::time::SystemTime::now()));
        self.store
            .update(&session)
            .map_err(|e| Status::internal(format!("session store update: {e}")))?;
        Ok(Response::new(CloseSessionResponse {
            session: Some(session),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_proto::ma_harness::v1::OperatingMode;

    fn service() -> SessionServiceImpl {
        SessionServiceImpl::new(Arc::new(crate::session_store::InMemoryStore::new()))
    }

    fn service_with_sqlite() -> SessionServiceImpl {
        SessionServiceImpl::new(Arc::new(
            crate::session_store::SqliteStore::open_in_memory().unwrap(),
        ))
    }

    #[tokio::test]
    async fn create_session() {
        let svc = service();
        let resp = svc
            .create(Request::new(CreateSessionRequest {
                name: "test".to_string(),
                mode: OperatingMode::Default as i32,
                metadata: None,
                enabled_plugins: vec![],
            }))
            .await
            .unwrap();
        let session = resp.into_inner().session.unwrap();
        assert_eq!(session.name, "test");
        assert_eq!(session.state, ProtoSessionState::Created as i32);
    }

    #[tokio::test]
    async fn get_session() {
        let svc = service();
        let create_resp = svc
            .create(Request::new(CreateSessionRequest {
                name: "s".to_string(),
                mode: 0,
                metadata: None,
                enabled_plugins: vec![],
            }))
            .await
            .unwrap();
        let id = create_resp.into_inner().session.unwrap().id;
        let get_resp = svc
            .get(Request::new(GetSessionRequest { id: id.clone() }))
            .await
            .unwrap();
        assert_eq!(get_resp.into_inner().id, id);
    }

    #[tokio::test]
    async fn get_session_not_found() {
        let svc = service();
        let result = svc
            .get(Request::new(GetSessionRequest {
                id: "nope".to_string(),
            }))
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn list_sessions() {
        let svc = service();
        for i in 0..3 {
            svc.create(Request::new(CreateSessionRequest {
                name: format!("s{}", i),
                mode: 0,
                metadata: None,
                enabled_plugins: vec![],
            }))
            .await
            .unwrap();
        }
        let list = svc
            .list(Request::new(ListSessionsRequest {
                page: 1,
                page_size: 10,
                state_filter: 0,
            }))
            .await
            .unwrap();
        let resp = list.into_inner();
        assert_eq!(resp.sessions.len(), 3);
        assert_eq!(resp.total, 3);
    }

    #[tokio::test]
    async fn close_session() {
        let svc = service();
        let create_resp = svc
            .create(Request::new(CreateSessionRequest {
                name: "s".to_string(),
                mode: 0,
                metadata: None,
                enabled_plugins: vec![],
            }))
            .await
            .unwrap();
        let id = create_resp.into_inner().session.unwrap().id;
        let close_resp = svc
            .close(Request::new(CloseSessionRequest {
                id: id.clone(),
                final_state: 0,
            }))
            .await
            .unwrap();
        let session = close_resp.into_inner().session.unwrap();
        assert_eq!(session.state, ProtoSessionState::Closed as i32);
    }

    // ---- SqliteStore 集成测试 ----

    #[tokio::test]
    async fn sqlite_create_and_get_through_service() {
        // 验 service 接 SqliteStore 真跑通
        let svc = service_with_sqlite();
        let resp = svc
            .create(Request::new(CreateSessionRequest {
                name: "sqlite-test".to_string(),
                mode: 0,
                metadata: None,
                enabled_plugins: vec![],
            }))
            .await
            .unwrap();
        let id = resp.into_inner().session.unwrap().id;
        let got = svc
            .get(Request::new(GetSessionRequest { id: id.clone() }))
            .await
            .unwrap();
        assert_eq!(got.into_inner().id, id);
    }
}
