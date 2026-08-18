//! SessionServiceImpl — gRPC SessionService 实现 (内存版 CRUD)
//!
//! Week 1 Day 18 实现. Phase 2 加持久化 (rusqlite).

use std::sync::Arc;

use dashmap::DashMap;

use ma_harness_proto::ma_harness::v1::{
    session_service_server::SessionService, CloseSessionRequest, CloseSessionResponse,
    CreateSessionRequest, CreateSessionResponse, GetSessionRequest, ListSessionsRequest,
    ListSessionsResponse, Session as ProtoSession, SessionState as ProtoSessionState,
};
use tonic::{Request, Response, Status};

/// SessionServiceImpl — 内存版 session CRUD
pub struct SessionServiceImpl {
    sessions: DashMap<String, ProtoSession>,
}

impl SessionServiceImpl {
    pub fn new(sessions: DashMap<String, ProtoSession>) -> Self {
        Self { sessions }
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
            state: ProtoSessionState::SessionStateCreated as i32,
            mode: req.mode,
            created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            closed_at: None,
            metadata: req.metadata,
            stats: None,
            enabled_plugins: req.enabled_plugins,
            user_id: String::new(),
        };
        self.sessions.insert(id.clone(), session.clone());
        Ok(Response::new(CreateSessionResponse { session: Some(session) }))
    }

    async fn get(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<ProtoSession>, Status> {
        let id = request.into_inner().id;
        self.sessions
            .get(&id)
            .map(|entry| Response::new(entry.value().clone()))
            .ok_or_else(|| Status::not_found(format!("session not found: {}", id)))
    }

    async fn list(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let _req = request.into_inner();
        let sessions: Vec<ProtoSession> = self
            .sessions
            .iter()
            .map(|e| e.value().clone())
            .collect();
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
            ProtoSessionState::SessionStateClosed as i32
        } else {
            req.final_state
        };
        let mut session = self
            .sessions
            .get_mut(&id)
            .ok_or_else(|| Status::not_found(format!("session not found: {}", id)))?;
        session.state = final_state;
        session.closed_at = Some(prost_types::Timestamp::from(std::time::SystemTime::now()));
        session.updated_at = Some(prost_types::Timestamp::from(std::time::SystemTime::now()));
        Ok(Response::new(CloseSessionResponse {
            session: Some(session.clone()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_proto::ma_harness::v1::OperatingMode;

    fn service() -> SessionServiceImpl {
        SessionServiceImpl::new(DashMap::new())
    }

    #[tokio::test]
    async fn create_session() {
        let svc = service();
        let resp = svc
            .create(Request::new(CreateSessionRequest {
                name: "test".to_string(),
                mode: OperatingMode::OperatingModeDefault as i32,
                metadata: None,
                enabled_plugins: vec![],
            }))
            .await
            .unwrap();
        let session = resp.into_inner().session.unwrap();
        assert_eq!(session.name, "test");
        assert_eq!(session.state, ProtoSessionState::SessionStateCreated as i32);
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
        assert_eq!(session.state, ProtoSessionState::SessionStateClosed as i32);
    }
}
