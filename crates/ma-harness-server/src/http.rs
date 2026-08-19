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
use salvo_extra::sse::{self as sse, SseEvent};
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

/// 全局 EventLog 容器 (Phase 5.3 / Day 92).
///
/// 跟 GLOBAL_SESSION_STORE 同样模式: Mutex 允许 test 多次覆盖.
static GLOBAL_EVENT_LOG: parking_lot::Mutex<Option<Arc<EventLog>>> =
    parking_lot::Mutex::new(None);

/// 2026-08-19 (Day 101 / P7-2.5): 全局 approval registry 容器
///
/// 跟 GLOBAL_EVENT_LOG 同样模式: Mutex 允许 test 多次覆盖.
/// 业务方调 set_global_approval() 装 approval service + policy.
/// HTTP handlers (POST/GET /v1/approvals) 走这个全局.
static GLOBAL_APPROVAL: parking_lot::Mutex<
    Option<Arc<ma_harness_cordis::ApprovalRegistry>>,
> = parking_lot::Mutex::new(None);

/// 2026-08-19 (Day 101 / P7-3.6): 全局 ChannelApprovalService (v2 oneshot 桥接)
///
/// 业务方调 set_global_channel_approval() 装; HTTP handlers
/// (GET /v1/approvals + POST/DELETE /v1/approvals/{id}) 走这个全局
/// 拿 pending list + submit decision.
static GLOBAL_CHANNEL_APPROVAL: parking_lot::Mutex<
    Option<Arc<ma_harness_cordis::ChannelApprovalService>>,
> = parking_lot::Mutex::new(None);

/// 初始化全局 approval registry (跟 run_router 配对调用)
pub fn set_global_approval(registry: Arc<ma_harness_cordis::ApprovalRegistry>) {
    *GLOBAL_APPROVAL.lock() = Some(registry);
}

/// 初始化全局 ChannelApprovalService (P7-3.6)
pub fn set_global_channel_approval(svc: Arc<ma_harness_cordis::ChannelApprovalService>) {
    *GLOBAL_CHANNEL_APPROVAL.lock() = Some(svc);
}

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

/// 初始化全局 EventLog (跟 `run_router_with_log_and_store` 配对调用)
pub fn set_global_event_log(log: Arc<EventLog>) {
    *GLOBAL_EVENT_LOG.lock() = Some(log);
}

/// 清除全局 EventLog (测试用)
pub fn clear_global_event_log() {
    *GLOBAL_EVENT_LOG.lock() = None;
}

/// 拿 EventLog (从 GLOBAL_EVENT_LOG), 拿不到返 None
fn get_global_event_log() -> Option<Arc<EventLog>> {
    GLOBAL_EVENT_LOG.lock().clone()
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

/// 构造含 /v1/runs + /v1/sessions + /v1/sessions/{id}/events 的 Router (Phase 5.3 / Day 92)
///
/// 全 3 个 global 都设: adapter + session store + event log.
/// event log 必传才能用 /events endpoint (跟 SessionServiceImpl 风格一致).
pub fn run_router_with_log_and_store(
    adapter: Arc<dyn ModelAdapter>,
    log: Arc<EventLog>,
    store: Arc<dyn SessionStore>,
) -> Router {
    set_global_adapter(adapter);
    set_global_event_log(log);
    set_global_session_store(store);
    Router::new()
        .push(Router::with_path("health").get(health))
        .push(Router::with_path("version").get(version))
        .push(
            Router::with_path("v1")
                .push(
                    Router::with_path("runs")
                        .post(create_run_handler)
                        .push(Router::with_path("stream").post(create_run_stream_handler)),
                )
                .push(sessions_router_with_events())
                // P7-2.5: HTTP approval 端点
                .push(approvals_router()),
        )
}

/// /v1/sessions 嵌套 router (Phase 5.1 / Day 90, 不含 events endpoint)
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

/// /v1/sessions 嵌套 router (Phase 5.3 / Day 92, 含 events endpoint)
///
/// 同 sessions_router, 加 GET /v1/sessions/{id}/events + GET /v1/sessions/{id}/events/stream (P7-1.7)
fn sessions_router_with_events() -> Router {
    Router::with_path("sessions")
        .get(list_sessions_handler)
        .post(create_session_handler)
        .push(Router::with_path("{id}").get(get_session_handler))
        .push(
            Router::with_path("{id}").push(Router::with_path("close").post(close_session_handler)),
        )
        .push(
            Router::with_path("{id}").push(
                Router::with_path("events")
                    .get(get_session_events_handler)
                    .push(Router::with_path("stream").get(stream_session_events_handler)),
            ),
        )
}

/// /v1/approvals 嵌套 router (P7-2.5 / Day 101)
///
/// HTTP approval 端点, 业务方用 Web UI / CLI / 其它 client 提交审批决策:
/// - GET    /v1/approvals          — 列出所有 pending approval
/// - GET    /v1/approvals/{id}     — 单个 approval detail
/// - POST   /v1/approvals/{id}     — 提交 decision (Approved / Denied)
/// - DELETE /v1/approvals/{id}     — 取消 (等 tool 调超时)
///
/// v1 简化: 业务方 register approval, 业务方手动 call approval service.
/// v2 集成: tool 调时 push pending, HTTP POST 决策后 oneshot set.
fn approvals_router() -> Router {
    Router::with_path("approvals")
        .get(list_approvals_handler)
        .push(
            Router::with_path("{id}")
                .get(get_approval_handler)
                .post(submit_approval_handler)
                .delete(cancel_approval_handler),
        )
}

/// GET /v1/approvals — 列出所有 pending approval (P7-3.6: 接 v2 ChannelApprovalService)
#[endpoint]
async fn list_approvals_handler() -> Json<serde_json::Value> {
    let svc = GLOBAL_CHANNEL_APPROVAL.lock().clone();
    match svc {
        None => Json(json!({
            "approvals": [],
            "count": 0,
            "_note": "P7-3.6: 没装 ChannelApprovalService, v2 集成待业务方调 set_global_channel_approval()"
        })),
        Some(svc) => {
            let ids = svc.pending_ids();
            Json(json!({
                "approvals": ids.iter().map(|id| json!({
                    "id": id,
                    "status": "pending",
                })).collect::<Vec<_>>(),
                "count": ids.len(),
            }))
        }
    }
}

/// GET /v1/approvals/{id} — 单个 approval detail (P7-3.6)
#[endpoint]
async fn get_approval_handler(id: PathParam<String>) -> Json<serde_json::Value> {
    let id = id.into_inner();
    let svc = GLOBAL_CHANNEL_APPROVAL.lock().clone();
    match svc {
        None => Json(json!({
            "id": id,
            "status": "unknown",
            "_note": "P7-3.6: 没装 ChannelApprovalService"
        })),
        Some(svc) => {
            let pending = svc.pending_ids().contains(&id);
            Json(json!({
                "id": id,
                "status": if pending { "pending" } else { "unknown_or_resolved" },
            }))
        }
    }
}

/// POST /v1/approvals/{id} body — 提交 decision (P7-3.6)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubmitDecisionRequest {
    /// 决策: "approved" | "denied" | "auto_approve"
    pub decision: String,
    /// 拒绝理由 (denied 时填)
    #[serde(default)]
    pub reason: Option<String>,
}

/// POST /v1/approvals/{id} — 提交 decision (P7-3.6)
#[endpoint]
async fn submit_approval_handler(
    id: PathParam<String>,
    body: JsonBody<SubmitDecisionRequest>,
) -> Json<serde_json::Value> {
    let id = id.into_inner();
    let req = body.0;
    let svc = GLOBAL_CHANNEL_APPROVAL.lock().clone();
    let Some(svc) = svc else {
        return Json(json!({
            "id": id,
            "status": "error",
            "_note": "P7-3.6: 没装 ChannelApprovalService"
        }));
    };
    use ma_harness_cordis::ApprovalDecision;
    let decision = match req.decision.as_str() {
        "approved" | "approve" => ApprovalDecision::Approved,
        "denied" | "deny" => ApprovalDecision::Denied {
            reason: req.reason.clone().unwrap_or_else(|| "user denied".to_string()),
        },
        "auto_approve" | "auto" => ApprovalDecision::AutoApprove,
        other => {
            return Json(json!({
                "id": id,
                "status": "error",
                "error": format!("unknown decision: {other}"),
            }));
        }
    };
    let submitted = svc.submit_decision(&id, decision.clone());
    Json(json!({
        "id": id,
        "decision": req.decision,
        "submitted": submitted,
        "_note": if submitted { "decision pushed" } else { "no pending request with this id" },
    }))
}

/// DELETE /v1/approvals/{id} — 取消 (P7-3.6: 走 ChannelApprovalService.cancel)
#[endpoint]
async fn cancel_approval_handler(id: PathParam<String>) -> Json<serde_json::Value> {
    let id = id.into_inner();
    let svc = GLOBAL_CHANNEL_APPROVAL.lock().clone();
    match svc {
        None => Json(json!({
            "id": id,
            "status": "error",
            "_note": "P7-3.6: 没装 ChannelApprovalService"
        })),
        Some(svc) => {
            let cancelled = svc.cancel(&id);
            Json(json!({
                "id": id,
                "status": if cancelled { "cancelled" } else { "not_found" },
            }))
        }
    }
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

/// POST /v1/runs/stream — P5-8 (Day 97): Server-Sent Events (SSE) 流式响应
///
/// 浏览器走 `EventSource("/v1/runs/stream")` 拿 streaming response.
#[handler]
async fn create_run_stream_handler(
    body: JsonBody<CreateRunRequest>,
    res: &mut Response,
) -> Result<(), salvo::Error> {
    use futures::StreamExt;
    let adapter: Arc<dyn ModelAdapter> = GLOBAL_ADAPTER
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(StubModelAdapter));

    let req = body.0;
    let session_id = req
        .session_id
        .unwrap_or_else(|| format!("http-stream-{}", uuid::Uuid::new_v4()));
    let run_id = uuid::Uuid::new_v4().to_string();

    // 整个 SSE 逻辑包到 async_stream::stream! 里 (避免 lifetime 撞 stack)
    // ModelRequest 在 stream 内部构造, outlive 自己
    let run_id_inner = run_id.clone();
    let session_id_inner = session_id.clone();
    let event_stream = async_stream::stream! {
        let model_req = ma_harness_core::ModelRequest {
            model: req.model.clone(),
            messages: vec![ma_harness_core::ModelMessage {
                role: "user".to_string(),
                content: req.message.clone(),
            }],
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            system_prompt: req.system_prompt.clone(),
        };

        // 调 adapter.complete_stream
        let token_stream = adapter.complete_stream(&model_req);
        futures::pin_mut!(token_stream);

        while let Some(token) = token_stream.next().await {
            let payload = serde_json::json!({
                "session_id": session_id_inner,
                "run_id": run_id_inner,
                "content": token,
            });
            yield Ok::<_, std::convert::Infallible>(
                SseEvent::default()
                    .name("token")
                    .id(run_id_inner.clone())
                    .text(payload.to_string()),
            );
        }
    };

    sse::stream(res, event_stream);
    Ok(())
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

// ============================================================================
// GET /v1/sessions/{id}/events — Phase 5.3 (Day 92)
// 跟 gRPC SessionService.GetSessionEvents 对齐
// ============================================================================

/// GET /v1/sessions/{id}/events 响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetSessionEventsResponse {
    /// session id
    pub session_id: String,
    /// 事件列表 (model_visible only, 按 seq 升序)
    pub events: Vec<SessionEventJson>,
    /// 事件总数 (拿前)
    pub total: u32,
}

/// 单个 event 的 JSON 表示
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionEventJson {
    /// 自增 seq
    pub seq: i64,
    /// 事件类型 (i32, 跟 EventType enum 对齐)
    pub event_type: i32,
    /// 严重度 (i32, 跟 Severity enum 对齐)
    pub severity: i32,
    /// 业务方 payload (JSON 字符串, 跟 proto SessionEvent.payload_json 对齐)
    pub payload_json: Option<String>,
    /// 业务方 session id (跟 URL 里的 {id} 一样, 冗余方便客户端)
    pub session_id: String,
    /// 事件时间 (RFC3339 字符串)
    pub timestamp: String,
}

/// GET /v1/sessions/{id}/events — 拿 session 的 model_visible events
///
/// 可选 query param `limit` (默认 50, 最大 1000).
#[endpoint]
async fn get_session_events_handler(
    id: PathParam<String>,
) -> Result<Json<GetSessionEventsResponse>, salvo::Error> {
    let log = get_global_event_log().ok_or_else(|| {
        salvo::Error::other(
            "EventLog 未初始化. 业务方应调 run_router_with_log_and_store(adapter, log, store) 而非 run_router_with_store",
        )
    })?;
    let session_id = id.0.clone();
    let id_for_log = id.0;
    // 同步 EventLog::get_model_visible (走 rusqlite, 阻塞)
    let page = tokio::task::spawn_blocking(move || log.get_model_visible(&id_for_log))
        .await
        .map_err(|e| salvo::Error::other(format!("join error: {e}")))?
        .map_err(|e| salvo::Error::other(format!("get model visible: {e}")))?;

    let total = page.events.len() as u32;
    let events: Vec<SessionEventJson> = page
        .events
        .into_iter()
        .map(|stored| {
            // ts 是 chrono::DateTime<Utc> (不是 prost Timestamp), 直接 RFC3339
            SessionEventJson {
                seq: stored.seq,
                event_type: stored.event.event_type as i32,
                severity: stored.event.severity as i32,
                payload_json: stored.event.payload_json,
                session_id: stored.event.session_id,
                timestamp: stored.event.ts.to_rfc3339(),
            }
        })
        .collect();

    Ok(Json(GetSessionEventsResponse {
        session_id,
        events,
        total,
    }))
}

/// GET /v1/sessions/{id}/events/stream — SSE 实时事件流 (P7-1.7)
///
/// 简化版: 轮询 EventLog 每 1s, 推新 event 给 client.
/// 完整 v2 用 broadcast channel (P8-2 + 真 pub-sub 一起做).
/// 返 SseEvent 流, 每个 event 是 JSON.
#[handler]
async fn stream_session_events_handler(
    req: &mut salvo::Request,
    res: &mut salvo::Response,
    id: PathParam<String>,
) -> Result<(), salvo::Error> {
    use futures::stream::StreamExt;

    let session_id = id.0.clone();
    let log = match get_global_event_log() {
        Some(log) => log,
        None => {
            return Err(salvo::Error::other(
                "EventLog 未初始化. 业务方应调 run_router_with_log_and_store",
            ));
        }
    };

    // 业务方可以传 since_seq query param (default 0)
    let since_seq: i64 = req
        .query::<String>("since_seq")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let event_stream = async_stream::stream! {
        let mut current_seq = since_seq;
        loop {
            // 拉新 events
            let log_clone = log.clone();
            let sid = session_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                log_clone.query(&ma_harness_core::log::EventQuery {
                    session_id: sid,
                    seq_from: Some(current_seq + 1),
                    ..Default::default()
                })
            }).await;

            match result {
                Ok(Ok(page)) => {
                    for stored in page.events {
                        let payload = serde_json::json!({
                            "seq": stored.seq,
                            "event_type": stored.event.event_type as i32,
                            "severity": stored.event.severity as i32,
                            "session_id": stored.event.session_id,
                            "payload_json": stored.event.payload_json,
                            "error_message": stored.event.error_message,
                            "timestamp": stored.event.ts.to_rfc3339(),
                        });
                        current_seq = stored.seq.max(current_seq);
                        yield Ok::<_, std::convert::Infallible>(
                            SseEvent::default()
                                .name("event")
                                .id(stored.seq.to_string())
                                .text(payload.to_string()),
                        );
                    }
                }
                Ok(Err(e)) => {
                    // log error, 推 error event, 继续轮询
                    yield Ok(SseEvent::default()
                        .name("error")
                        .text(format!("event log error: {e}")));
                }
                Err(e) => {
                    yield Ok(SseEvent::default()
                        .name("error")
                        .text(format!("join error: {e}")));
                }
            }

            // heartbeat
            yield Ok(SseEvent::default()
                .name("heartbeat")
                .text("ping"));

            // 1s 后再轮询
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
    };

    let mut event_stream = Box::pin(event_stream);
    let event_stream = async_stream::stream! {
        while let Some(ev) = event_stream.next().await {
            yield ev;
        }
    };

    sse::stream(res, event_stream);
    Ok(())
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

    fn full_router() -> Router {
        // 拿 log + store 都设, 走 /v1/sessions/{id}/events
        let log = EventLog::open_in_memory().unwrap();
        let store: Arc<dyn SessionStore> = Arc::new(crate::session_store::InMemoryStore::new());
        let _ = SESSION_TEST_LOCK.lock();
        clear_global_session_store();
        clear_global_event_log();
        run_router_with_log_and_store(Arc::new(StubModelAdapter), Arc::new(log), store)
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

    // === P5-3 (Day 92): HTTP /v1/sessions/{id}/events 跟 gRPC GetSessionEvents 对齐 ===

    /// /events endpoint 拿指定 session 的 events
    #[tokio::test]
    async fn get_v1_session_events_returns_events() {
        let _lock = session_test_lock();
        // 1. 准备 log, append 2 个 event for session "ev-1"
        let log = EventLog::open_in_memory().unwrap();
        for evt_type in [
            ma_harness_core::EventType::SessionStart,
            ma_harness_core::EventType::ToolCall,
        ] {
            let mut ev = ma_harness_core::SessionEvent::new("ev-1", evt_type);
            ev.payload_json = Some(format!(r#"{{"tool":"echo","seq":{}}}"#, evt_type as i32));
            let _ = log.append(ev);
        }
        // 别的 session, 不应返回
        let mut other = ma_harness_core::SessionEvent::new("other", ma_harness_core::EventType::SessionStart);
        other.payload_json = Some(r#"{"session":"other"}"#.to_string());
        let _ = log.append(other);

        // 2. 构造 router
        let store: Arc<dyn SessionStore> = Arc::new(crate::session_store::InMemoryStore::new());
        clear_global_session_store();
        clear_global_event_log();
        let service = Service::new(run_router_with_log_and_store(
            Arc::new(StubModelAdapter),
            Arc::new(log),
            store,
        ));

        // 3. 拿 ev-1 events
        let mut resp = TestClient::get("http://localhost/v1/sessions/ev-1/events")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        let json: GetSessionEventsResponse = resp.take_json().await.unwrap();
        assert_eq!(json.session_id, "ev-1");
        // 至少 2 个 (SessionStart + ToolCall)
        assert!(json.total >= 2, "ev-1 应有 >= 2 events, got {}", json.total);
        assert!(json.events.len() >= 2);
        // payload_json 应有
        for e in &json.events {
            assert!(e.payload_json.is_some(), "payload 应有, got {:?}", e);
        }
    }

    /// /events endpoint 没设 log 时 404
    #[tokio::test]
    async fn get_v1_session_events_no_log_404() {
        let _lock = session_test_lock();
        // 用 session_router() (无 log)
        let service = Service::new(session_router());
        let resp = TestClient::get("http://localhost/v1/sessions/some-id/events")
            .send(&service)
            .await;
        // 没 log 时该 endpoint 不存在 → 404
        assert_eq!(
            resp.status_code,
            Some(salvo::http::StatusCode::NOT_FOUND),
            "/events endpoint 不应存在 (session_router 不含), got {:?}",
            resp.status_code
        );
    }

    // === P5-8 (Day 97): HTTP /v1/runs/stream SSE endpoint ===

    /// SSE endpoint 走真 StubModelAdapter, 验返回 SSE events
    #[tokio::test]
    async fn post_v1_runs_stream_returns_sse_events() {
        // SSE 走 run_router_with_log_and_store (3-arg 版本)
        let log = EventLog::open_in_memory().unwrap();
        let store: Arc<dyn SessionStore> = Arc::new(crate::session_store::InMemoryStore::new());
        clear_global_session_store();
        clear_global_event_log();
        let service = Service::new(run_router_with_log_and_store(
            Arc::new(StubModelAdapter),
            Arc::new(log),
            store,
        ));
        let body = serde_json::json!({
            "message": "alpha beta gamma",
            "model": "stub",
        });
        let mut resp = TestClient::post("http://localhost/v1/runs/stream")
            .json(&body)
            .send(&service)
            .await;
        // SSE 返 200
        assert_eq!(
            resp.status_code,
            Some(salvo::http::StatusCode::OK),
            "SSE endpoint 应返 200"
        );
        // 拿 body 字节
        let body_bytes = resp.take_bytes(None).await.unwrap();
        let body_str = String::from_utf8_lossy(&body_bytes);
        // 应含 3 个 event 字段
        let event_count = body_str.matches("event:").count();
        assert!(event_count >= 1, "SSE body 应有 event: 字段, got: {}", body_str);
        // 也应含 token "alpha"
        assert!(body_str.contains("alpha"), "SSE 应含 token 'alpha'");
    }

    // ============================================================================
    // P7-3.6: Approval v2 HTTP 端点 (ChannelApprovalService 集成) 测试
    // ============================================================================
    use ma_harness_cordis::{
        ApprovalDecision, ApprovalRequest, ApprovalService, ChannelApprovalService, Context,
        RiskLevel,
    };
    use std::time::Duration;

    fn install_channel_approval() -> Arc<ChannelApprovalService> {
        let svc = Arc::new(ChannelApprovalService::new());
        set_global_channel_approval(svc.clone());
        svc
    }

    /// 跑 approval v2 测试用的 router (走 run_router_with_log_and_store, 含 /v1/approvals)
    fn approval_v2_router() -> Service {
        let log = Arc::new(ma_harness_core::EventLog::open_in_memory().unwrap());
        let store: Arc<dyn crate::session_store::SessionStore> =
            Arc::new(crate::session_store::SqliteStore::open_in_memory().unwrap());
        Service::new(run_router_with_log_and_store(
            Arc::new(ma_harness_core::StubModelAdapter),
            log,
            store,
        ))
    }

    #[tokio::test]
    async fn http_v2_approvals_list_empty() {
        let _lock = SESSION_TEST_LOCK.lock();
        let _svc = install_channel_approval();
        let service = approval_v2_router();
        let mut resp = TestClient::get("http://localhost/v1/approvals")
            .send(&service)
            .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        let v: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(v["count"], 0);
    }

    #[tokio::test]
    async fn http_v2_approvals_get_unknown_id() {
        let _lock = SESSION_TEST_LOCK.lock();
        let _svc = install_channel_approval();
        let service = approval_v2_router();
        let mut resp = TestClient::get("http://localhost/v1/approvals/ghost-id")
            .send(&service)
            .await;
        let v: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(v["status"], "unknown_or_resolved");
    }

    #[tokio::test]
    async fn http_v2_approvals_submit_decision_wakes_future() {
        // 完整 e2e: 起一个 spawn 调 request_approval, HTTP POST submit decision 唤醒
        let _lock = SESSION_TEST_LOCK.lock();
        let svc = install_channel_approval();
        let ctx = Context::new();

        // 业务方 spawn 调 request_approval
        let svc2 = svc.clone();
        let task: tokio::task::JoinHandle<Result<ApprovalDecision, _>> = tokio::spawn(async move {
            let req = ApprovalRequest {
                tool_name: "fs.delete".into(),
                arguments: serde_json::json!({}),
                risk_level: RiskLevel::High,
                context: "delete /tmp/x".into(),
                tool_call_id: "e2e-req-1".into(),
            };
            ApprovalService::request_approval(svc2.as_ref(), &ctx, &req).await
        });

        // 等 50ms 让 request 进 pending
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(svc.pending_count(), 1);

        // HTTP GET list 应看到 1 个 pending
        let service = approval_v2_router();
        let mut resp = TestClient::get("http://localhost/v1/approvals")
            .send(&service)
            .await;
        let v: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(v["count"], 1);
        assert_eq!(v["approvals"][0]["id"], "e2e-req-1");

        // HTTP POST submit decision
        let body = serde_json::json!({
            "decision": "approved",
        });
        let mut resp2 = TestClient::post("http://localhost/v1/approvals/e2e-req-1")
            .json(&body)
            .send(&service)
            .await;
        let v2: serde_json::Value = resp2.take_json().await.unwrap();
        assert_eq!(v2["submitted"], true);

        // 业务方 spawn 应该拿 Approved
        let result = task.await.unwrap().unwrap();
        assert_eq!(result, ApprovalDecision::Approved);
        assert_eq!(svc.pending_count(), 0);
    }

    #[tokio::test]
    async fn http_v2_approvals_cancel_via_delete() {
        let _lock = SESSION_TEST_LOCK.lock();
        let svc = install_channel_approval();
        let svc2 = svc.clone();
        let task: tokio::task::JoinHandle<Result<ApprovalDecision, _>> = tokio::spawn(async move {
            let req = ApprovalRequest {
                tool_name: "fs.delete".into(),
                arguments: serde_json::json!({}),
                risk_level: RiskLevel::High,
                context: "delete".into(),
                tool_call_id: "e2e-cancel-1".into(),
            };
            ApprovalService::request_approval(svc2.as_ref(), &Context::new(), &req).await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let service = approval_v2_router();
        let mut resp = TestClient::delete("http://localhost/v1/approvals/e2e-cancel-1")
            .send(&service)
            .await;
        let v: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(v["status"], "cancelled");

        let result = task.await.unwrap().unwrap();
        assert!(matches!(result, ApprovalDecision::Denied { .. }));
    }

    #[tokio::test]
    async fn http_v2_approvals_submit_denied_with_reason() {
        let _lock = SESSION_TEST_LOCK.lock();
        let svc = install_channel_approval();
        let svc2 = svc.clone();
        let task: tokio::task::JoinHandle<Result<ApprovalDecision, _>> = tokio::spawn(async move {
            let req = ApprovalRequest {
                tool_name: "fs.delete".into(),
                arguments: serde_json::json!({}),
                risk_level: RiskLevel::High,
                context: "delete".into(),
                tool_call_id: "e2e-deny-1".into(),
            };
            ApprovalService::request_approval(svc2.as_ref(), &Context::new(), &req).await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let service = approval_v2_router();
        let body = serde_json::json!({
            "decision": "denied",
            "reason": "test reason",
        });
        let mut resp = TestClient::post("http://localhost/v1/approvals/e2e-deny-1")
            .json(&body)
            .send(&service)
            .await;
        let v: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(v["submitted"], true);

        let result = task.await.unwrap().unwrap();
        match result {
            ApprovalDecision::Denied { reason } => {
                assert_eq!(reason, "test reason");
            }
            _ => panic!("expected Denied"),
        }
    }

    #[tokio::test]
    async fn http_v2_approvals_submit_unknown_id_returns_false() {
        let _lock = SESSION_TEST_LOCK.lock();
        let _svc = install_channel_approval();
        let service = approval_v2_router();
        let body = serde_json::json!({ "decision": "approved" });
        let mut resp = TestClient::post("http://localhost/v1/approvals/ghost")
            .json(&body)
            .send(&service)
            .await;
        let v: serde_json::Value = resp.take_json().await.unwrap();
        assert_eq!(v["submitted"], false);
    }

    // P7-1.7: SSE events/stream
    #[tokio::test]
    async fn http_sse_events_stream_endpoint_exists() {
        let _lock = SESSION_TEST_LOCK.lock();

        let log = Arc::new(ma_harness_core::EventLog::open_in_memory().unwrap());
        let store: Arc<dyn crate::session_store::SessionStore> =
            Arc::new(crate::session_store::SqliteStore::open_in_memory().unwrap());
        let service = Service::new(run_router_with_log_and_store(
            Arc::new(ma_harness_core::StubModelAdapter),
            log,
            store,
        ));

        // 验 SSE 端点存在 + 返 200 + Content-Type: text/event-stream
        // (业务方实际拿 body 用 EventSource 长连, server 测试不阻塞读 body)
        let resp = TestClient::get(
            "http://localhost/v1/sessions/empty-session/events/stream?since_seq=0",
        )
        .send(&service)
        .await;
        assert_eq!(resp.status_code, Some(salvo::http::StatusCode::OK));
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("event-stream"), "content-type 应含 event-stream, got: {ct}");
    }
}
