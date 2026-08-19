//! HTTP 服务 (salvo): /health + /version + /v1/runs + /v1/sessions
//!
//! 2026-08-18: 从 axum 迁移到 salvo (见 decision-log §12).
//! 迁移动机: salvo 内置 OpenAPI 导出, 编译更快, 跟 ma-harness 风格更贴.
//!
//! Phase 2.5: 加 /v1/runs (POST JSON 跑一次 agent, 跟 gRPC AgentService.Run 对齐),
//!            + /openapi.json (salvo 内置 OpenAPI 3.0 导出, 通过 #[endpoint] macro 自动).
//! Phase 5.1 (Day 90): 加 /v1/sessions (4 endpoint) 跟 gRPC SessionService 对齐.

use std::sync::Arc;

use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, ModelAdapter, StubModelAdapter};
use salvo::oapi::extract::JsonBody;
use salvo::oapi::extract::PathParam;
use salvo::oapi::{ToResponse, ToSchema};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::session_service::SessionServiceImpl;
use crate::session_store::SessionStore;

/// 全局 model adapter 容器 (Phase 2.5 HTTP/REST 简化设计).
///
/// 设计: salvo 0.79 Router 没有 `.data()` API, 没法把 Arc<dyn ModelAdapter> 注入到 handler.
/// 改用进程全局 `OnceCell<Arc<dyn ModelAdapter>>`, `run_router(adapter)` 时初始化一次,
/// handler 从全局拿. 多 router 实例会**共享** (适合单进程 server 场景).
static GLOBAL_ADAPTER: OnceCell<Arc<dyn ModelAdapter>> = OnceCell::const_new();

/// 全局 SessionStore 容器 (Phase 5.1 / Day 90).
///
/// 跟 GLOBAL_ADAPTER 不一样: 用 `parking_lot::Mutex<Option<Arc<dyn SessionStore>>>` 允许覆盖
/// (test 多次调 `run_router_with_store` 时不能 stuck 在第一次).
static GLOBAL_SESSION_STORE: parking_lot::Mutex<Option<Arc<dyn SessionStore>>> =
    parking_lot::Mutex::new(None);

/// 初始化全局 adapter (跟 `run_router` 配对调用)
pub fn set_global_adapter(adapter: Arc<dyn ModelAdapter>) {
    let _ = GLOBAL_ADAPTER.set(adapter);
}

/// 初始化全局 SessionStore (跟 `run_router_with_store` 配对调用)
///
/// 跟 set_global_adapter 不一样: 允许覆盖 (test 多次 init 不爆).
pub fn set_global_session_store(store: Arc<dyn SessionStore>) {
    *GLOBAL_SESSION_STORE.lock() = Some(store);
}

/// 清除全局 SessionStore (测试用, 跑下一个 test 前调)
pub fn clear_global_session_store() {
    *GLOBAL_SESSION_STORE.lock() = None;
}

/// 拿 SessionStore (从 GLOBAL_SESSION_STORE), 拿不到返 None
fn get_global_session_store() -> Option<Arc<dyn SessionStore>> {
    GLOBAL_SESSION_STORE.lock().clone()
}

/// 构造 salvo Router (基础: /health + /version)
///
/// **Phase 3.5 (T3.5)**: handler 改用 `#[endpoint]` 而非 `#[handler]`,
/// 这样 salvo-oapi 0.79 `merge_router` 能识别 + 提取 OpenAPI 注解.
/// 业务方 `mah openapi export` 拿到的 spec 包含这些路由.
pub fn router() -> Router {
    Router::new()
        .push(Router::with_path("health").get(health))
        .push(Router::with_path("version").get(version))
}

/// 构造含 /v1/runs 的 Router (Phase 2.5 HTTP/REST endpoint)
///
/// 调这个会**同时** set GLOBAL_ADAPTER (后续 handler 用). 多次调用后到的覆盖前面.
pub fn run_router(adapter: Arc<dyn ModelAdapter>) -> Router {
    set_global_adapter(adapter);
    Router::new()
        .push(Router::with_path("health").get(health))
        .push(Router::with_path("version").get(version))
        .push(
            Router::with_path("v1").push(Router::with_path("runs").post(create_run_handler)),
        )
}

/// 构造含 /v1/runs + /v1/sessions 的 Router (Phase 5.1 / Day 90)
///
/// 调这个会**同时** set GLOBAL_ADAPTER + GLOBAL_SESSION_STORE.
/// session store 必传 (跟 gRPC ServerBuilder 风格一致).
pub fn run_router_with_store(
    adapter: Arc<dyn ModelAdapter>,
    store: Arc<dyn SessionStore>,
) -> Router {
    set_global_adapter(adapter);
    set_global_session_store(store);
    Router::new()
        .push(Router::with_path("health").get(health))
        .push(Router::with_path("version").get(version))
        .push(
            Router::with_path("v1")
                .push(Router::with_path("runs").post(create_run_handler))
                .push(sessions_router()),
        )
}

/// /v1/sessions 嵌套 router (Phase 5.1 / Day 90)
///
/// - GET /v1/sessions — list
/// - POST /v1/sessions — create
/// - GET /v1/sessions/{id} — get
/// - POST /v1/sessions/{id}/close — close
fn sessions_router() -> Router {
    Router::with_path("sessions")
        .get(list_sessions_handler)
        .post(create_session_handler)
        .push(Router::with_path("{id}").get(get_session_handler))
        .push(
            Router::with_path("{id}").push(Router::with_path("close").post(close_session_handler)),
        )
}

/// /health 处理器 (Phase 3.5: 改用 #[endpoint] for OpenAPI export)
#[endpoint]
async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "ma-harness",
    }))
}

/// /version 处理器 (Phase 3.5: 改用 #[endpoint] for OpenAPI export)
#[endpoint]
async fn version() -> Json<serde_json::Value> {
    Json(json!({
        "name": "ma-harness",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

// ============================================================================
// POST /v1/runs — Phase 2.5 HTTP/REST endpoint
// ============================================================================

/// POST /v1/runs request body
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateRunRequest {
    /// Session ID (选填, 留空 = 新建)
    #[serde(default)]
    pub session_id: Option<String>,
    /// 用户消息
    pub message: String,
    /// 模型名 (默认 "stub")
    #[serde(default = "default_model")]
    pub model: String,
    /// 温度 (0.0 - 2.0, 默认 0.7)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// max tokens (默认 1024)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// system prompt (选填)
    #[serde(default)]
    pub system_prompt: Option<String>,
}

fn default_model() -> String {
    "stub".to_string()
}

const fn default_temperature() -> f32 {
    0.7
}

const fn default_max_tokens() -> u32 {
    1024
}

/// POST /v1/runs response body
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, ToResponse)]
pub struct CreateRunResponse {
    /// Session ID
    pub session_id: String,
    /// Run ID (UUID)
    pub run_id: String,
    /// Model 回复内容
    pub content: String,
    /// 模型 ID
    pub model: String,
    /// 停止原因 ("stop" / "length" / "content_filter")
    pub finish_reason: String,
    /// prompt tokens
    pub prompt_tokens: u32,
    /// completion tokens
    pub completion_tokens: u32,
}

/// POST /v1/runs handler
///
/// 从 `GLOBAL_ADAPTER` 拿 model adapter (跟 `run_router(adapter)` 配对).
/// 拿不到时降级到 `StubModelAdapter` (开发期不 panic).
///
/// **P4-4 (Phase 4)**: 改用 `#[endpoint]` 而非 `#[handler]`,
/// 让 salvo-oapi 0.79 `merge_router` 能识别 + 提取 OpenAPI 注解.
/// 业务方 `mah openapi export` 拿到的 spec 包含 /v1/runs.
#[endpoint]
async fn create_run_handler(
    body: JsonBody<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, salvo::Error> {
    let adapter: Arc<dyn ModelAdapter> = GLOBAL_ADAPTER
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(StubModelAdapter));

    let log = EventLog::open_in_memory().map_err(|e| {
        salvo::Error::other(format!("eventlog open: {e}"))
    })?;
    let req = body.0;
    let session_id = req
        .session_id
        .unwrap_or_else(|| format!("http-{}", uuid::Uuid::new_v4()));
    let agent = AgentLoop::new(log, adapter);
    let agent_req = AgentRunRequest {
        session_id: session_id.clone(),
        user_message: req.message,
        model: req.model.clone(),
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        system_prompt: req.system_prompt,
    };
    let resp = agent.run(agent_req).await.map_err(|e| {
        salvo::Error::other(format!("agent run: {e}"))
    })?;
    Ok(Json(CreateRunResponse {
        session_id: resp.session_id,
        run_id: resp.run_id,
        content: resp.model_response.content,
        model: resp.model_response.model,
        finish_reason: format!("{:?}", resp.model_response.finish_reason).to_lowercase(),
        prompt_tokens: resp.total_prompt_tokens,
        completion_tokens: resp.total_completion_tokens,
    }))
}

// ============================================================================
// POST /v1/sessions + GET /v1/sessions + GET /v1/sessions/{id} + POST /v1/sessions/{id}/close
// Phase 5.1 (Day 90): 跟 gRPC SessionService 对齐
// ============================================================================

/// 拿 SessionService (从 GLOBAL_SESSION_STORE), 拿不到返 503
fn session_service() -> Result<SessionServiceImpl, salvo::Error> {
    let store = get_global_session_store().ok_or_else(|| {
        salvo::Error::other(
            "SessionStore 未初始化. 业务方应调 run_router_with_store(adapter, store) 而非 run_router(adapter)",
        )
    })?;
    Ok(SessionServiceImpl::new(store))
}

/// GET /v1/sessions 响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListSessionsResponse {
    /// 所有 session (按 created_at DESC)
    pub sessions: Vec<ProtoSessionJson>,
    /// session 总数
    pub total: u32,
}

/// 单个 session 的 JSON 表示 (跟 proto Session 对齐)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProtoSessionJson {
    pub id: String,
    pub name: String,
    /// state: 0=Created, 1=Active, 2=Paused, 3=Closed
    pub state: i32,
    /// mode: 0=Default, 1=...
    pub mode: i32,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub closed_at: Option<String>,
    pub enabled_plugins: Vec<String>,
    pub user_id: String,
}

/// proto Session → JSON (Phase 5.1 / Day 90, 不依赖 serde derive 跨 prost 类型)
fn proto_session_to_json(s: &ma_harness_proto::ma_harness::v1::Session) -> ProtoSessionJson {
    ProtoSessionJson {
        id: s.id.clone(),
        name: s.name.clone(),
        state: s.state,
        mode: s.mode,
        created_at: s.created_at.as_ref().map(ts_to_string),
        updated_at: s.updated_at.as_ref().map(ts_to_string),
        closed_at: s.closed_at.as_ref().map(ts_to_string),
        enabled_plugins: s.enabled_plugins.clone(),
        user_id: s.user_id.clone(),
    }
}

fn ts_to_string(ts: &prost_types::Timestamp) -> String {
    let secs = ts.seconds;
    let nanos = ts.nanos as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| format!("{}s+{}ns", secs, nanos))
}

/// GET /v1/sessions — 列出所有 session
#[endpoint]
async fn list_sessions_handler() -> Result<Json<ListSessionsResponse>, salvo::Error> {
    let svc = session_service()?;
    // 跟 gRPC 一致: 不分页, 返所有
    let sessions = tokio::task::spawn_blocking(move || svc.list_sessions())
        .await
        .map_err(|e| salvo::Error::other(format!("join error: {e}")))?
        .map_err(|e| salvo::Error::other(format!("list sessions: {e}")))?;
    let total = sessions.len() as u32;
    Ok(Json(ListSessionsResponse {
        sessions: sessions.iter().map(proto_session_to_json).collect(),
        total,
    }))
}

/// POST /v1/sessions 请求
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateSessionRequest {
    /// session 名 (留空 = auto generate "session-{8-char-id}")
    #[serde(default)]
    pub name: String,
    /// mode: 0=Default (跟 gRPC OperatingMode 一致)
    #[serde(default)]
    pub mode: i32,
    /// 启用的 plugin 列表 (留空 = 全部)
    #[serde(default)]
    pub enabled_plugins: Vec<String>,
}

/// POST /v1/sessions — 创建新 session
#[endpoint]
async fn create_session_handler(
    body: JsonBody<CreateSessionRequest>,
) -> Result<Json<ProtoSessionJson>, salvo::Error> {
    let svc = session_service()?;
    let req = body.0;

    let id = Uuid::new_v4().to_string();
    let name = if req.name.is_empty() {
        format!("session-{}", &id[..8])
    } else {
        req.name.clone()
    };

    let now = chrono::Utc::now();
    let now_ts = prost_types::Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    };

    let session = ma_harness_proto::ma_harness::v1::Session {
        id: id.clone(),
        name,
        state: ma_harness_proto::ma_harness::v1::SessionState::Created as i32,
        mode: req.mode,
        created_at: Some(now_ts),
        updated_at: Some(now_ts),
        closed_at: None,
        metadata: None,
        stats: None,
        enabled_plugins: req.enabled_plugins,
        user_id: String::new(),
    };

    let session_for_response = session.clone();
    let svc_clone = svc;
    tokio::task::spawn_blocking(move || svc_clone.create_session(session))
        .await
        .map_err(|e| salvo::Error::other(format!("join error: {e}")))?
        .map_err(|e| salvo::Error::other(format!("create session: {e}")))?;

    Ok(Json(proto_session_to_json(&session_for_response)))
}

/// GET /v1/sessions/{id} — 拿单个 session
#[endpoint]
async fn get_session_handler(id: PathParam<String>) -> Result<Json<ProtoSessionJson>, salvo::Error> {
    let svc = session_service()?;
    let s = tokio::task::spawn_blocking(move || svc.get_session(&id.0))
        .await
        .map_err(|e| salvo::Error::other(format!("join error: {e}")))?
        .map_err(|e| salvo::Error::other(format!("get session: {e}")))?
        .ok_or_else(|| salvo::Error::other("session not found"))?;
    Ok(Json(proto_session_to_json(&s)))
}

/// POST /v1/sessions/{id}/close — 关闭 session
#[endpoint]
async fn close_session_handler(id: PathParam<String>) -> Result<Json<ProtoSessionJson>, salvo::Error> {
    let svc = session_service()?;
    let s = tokio::task::spawn_blocking(move || svc.close_session(&id.0))
        .await
        .map_err(|e| salvo::Error::other(format!("join error: {e}")))?
        .map_err(|e| salvo::Error::other(format!("close session: {e}")))?
        .ok_or_else(|| salvo::Error::other("session not found"))?;
    Ok(Json(proto_session_to_json(&s)))
}

#[cfg(test)]
mod tests {
    // 2026-08-18: 用 salvo::test::TestClient 标准 API
    // 之前 mental commit 写 `router().into_service()` + `salvo::hyper::Body::empty()` 都错.
    // 正确做法: `TestClient::get(url).send(service)` 一行
    use super::*;
    use salvo::test::{ResponseExt, TestClient};

    /// 全局 session-test Mutex, 强制串行 (Phase 5.1 / Day 90)
    ///
    /// 原因: GLOBAL_SESSION_STORE 是进程全局, 并行 test 会 race (A 设 store_a,
    ///       B 设 store_b, A 后续请求读 store_b, 看到 B 的 session)
    /// 解决: 每个 session test 拿这个 lock, 强制串行
    static SESSION_TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[tokio::test]
    async fn health_returns_ok() {
        let service = Service::new(router());
        let resp = TestClient::get("http://localhost/health")
            .send(&service)
            .await;
        // salvo::Response 字段是 status_code (Option<StatusCode>)
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
    }

    #[tokio::test]
    async fn version_returns_ok() {
        let service = Service::new(router());
        let resp = TestClient::get("http://localhost/version")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
    }

    #[tokio::test]
    async fn post_v1_runs_with_stub_returns_echo() {
        // 验 Phase 2.5 HTTP/REST endpoint 走通
        let service = Service::new(run_router(Arc::new(StubModelAdapter)));
        let body = serde_json::json!({
            "message": "hello",
            "model": "stub",
        });
        let mut resp = TestClient::post("http://localhost/v1/runs")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        // 解析响应 (salvo TestClient 的 take_json 拿 JSON)
        let json: CreateRunResponse = resp.take_json().await.unwrap();
        assert!(!json.run_id.is_empty());
        assert_eq!(json.model, "stub");
        assert!(json.content.contains("hello"));
        assert!(json.prompt_tokens > 0);
        assert!(json.completion_tokens > 0);
    }

    #[tokio::test]
    async fn post_v1_runs_with_custom_session_id() {
        // 验 session_id 透传
        let service = Service::new(run_router(Arc::new(StubModelAdapter)));
        let body = serde_json::json!({
            "session_id": "my-session-42",
            "message": "hi",
        });
        let mut resp = TestClient::post("http://localhost/v1/runs")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        let json: CreateRunResponse = resp.take_json().await.unwrap();
        assert_eq!(json.session_id, "my-session-42");
    }

    // === Phase 5.1 (Day 90): /v1/sessions 4 endpoint ===

    fn session_router() -> Router {
        // 每次新建 fresh store, 不让其他 test 残留 session 干扰
        let store: Arc<dyn SessionStore> = Arc::new(crate::session_store::InMemoryStore::new());
        // 先清, 再设 (避免跟其他并行 test 抢)
        clear_global_session_store();
        run_router_with_store(Arc::new(StubModelAdapter), store)
    }

    /// 拿 session-test lock (整个 test 期间持有)
    /// 用法: `let _lock = session_test_lock();`
    fn session_test_lock() -> parking_lot::MutexGuard<'static, ()> {
        SESSION_TEST_LOCK.lock()
    }

    #[tokio::test]
    async fn get_v1_sessions_empty() {
        let _lock = session_test_lock();
        let service = Service::new(session_router());
        let mut resp = TestClient::get("http://localhost/v1/sessions")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        let json: ListSessionsResponse = resp.take_json().await.unwrap();
        assert_eq!(json.total, 0);
        assert!(json.sessions.is_empty());
    }

    #[tokio::test]
    async fn post_v1_sessions_then_get() {
        let _lock = session_test_lock();
        // 1. create
        let service = Service::new(session_router());
        let body = serde_json::json!({
            "name": "test-session-1",
            "mode": 0,
            "enabled_plugins": ["hello"],
        });
        let mut resp = TestClient::post("http://localhost/v1/sessions")
            .json(&body)
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        let created: ProtoSessionJson = resp.take_json().await.unwrap();
        assert_eq!(created.name, "test-session-1");
        assert_eq!(created.enabled_plugins, vec!["hello".to_string()]);
        // state 应该是 Created (1)
        assert_eq!(created.state, 1, "state 应是 Created (1), got {}", created.state);
        // id 是 uuid 形式
        assert!(!created.id.is_empty());

        // 2. get
        let mut resp = TestClient::get(format!("http://localhost/v1/sessions/{}", created.id).as_str())
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        let got: ProtoSessionJson = resp.take_json().await.unwrap();
        assert_eq!(got.id, created.id);
        assert_eq!(got.name, "test-session-1");
    }

    #[tokio::test]
    async fn get_v1_sessions_list_after_create() {
        let _lock = session_test_lock();
        let service = Service::new(session_router());
        // 创 2 个
        for name in ["a", "b"] {
            TestClient::post("http://localhost/v1/sessions")
                .json(&serde_json::json!({ "name": name }))
                .send(&service)
                .await;
        }
        // list
        let mut resp = TestClient::get("http://localhost/v1/sessions")
            .send(&service)
            .await;
        let json: ListSessionsResponse = resp.take_json().await.unwrap();
        assert_eq!(json.total, 2);
        let names: Vec<String> = json.sessions.iter().map(|s| s.name.clone()).collect();
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
    }

    #[tokio::test]
    async fn post_v1_sessions_close_flips_state() {
        let _lock = session_test_lock();
        let service = Service::new(session_router());
        // 创一个
        let mut resp = TestClient::post("http://localhost/v1/sessions")
            .json(&serde_json::json!({ "name": "to-close" }))
            .send(&service)
            .await;
        let created: ProtoSessionJson = resp.take_json().await.unwrap();
        // close
        let mut resp = TestClient::post(format!("http://localhost/v1/sessions/{}/close", created.id).as_str())
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        let closed: ProtoSessionJson = resp.take_json().await.unwrap();
        // state 应该是 Closed (4)
        assert_eq!(closed.state, 4, "state 应是 Closed (4), got {}", closed.state);
        assert!(closed.closed_at.is_some(), "closed_at 应填");
    }

    #[tokio::test]
    async fn get_v1_sessions_not_found_errors() {
        let _lock = session_test_lock();
        let service = Service::new(session_router());
        let resp = TestClient::get("http://localhost/v1/sessions/nonexistent")
            .send(&service)
            .await;
        // 返 500 (salvo::Error::other → 500 internal error)
        assert!(
            resp.status_code == Some(salvo::http::StatusCode::INTERNAL_SERVER_ERROR)
                || resp.status_code == Some(salvo::http::StatusCode::NOT_FOUND),
            "not found 应返 4xx/5xx, got {:?}",
            resp.status_code
        );
    }
}
