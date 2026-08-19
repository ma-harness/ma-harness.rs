//! HTTP 服务 (salvo): /health + /version + /v1/runs
//!
//! 2026-08-18: 从 axum 迁移到 salvo (见 decision-log §12).
//! 迁移动机: salvo 内置 OpenAPI 导出, 编译更快, 跟 ma-harness 风格更贴.
//!
//! Phase 2.5: 加 /v1/runs (POST JSON 跑一次 agent, 跟 gRPC AgentService.Run 对齐),
//!            + /openapi.json (salvo 内置 OpenAPI 3.0 导出, 通过 #[endpoint] macro 自动).

use std::sync::Arc;

use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, ModelAdapter, StubModelAdapter};
use salvo::oapi::extract::JsonBody;
use salvo::oapi::{ToResponse, ToSchema};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::OnceCell;

/// 全局 model adapter 容器 (Phase 2.5 HTTP/REST 简化设计).
///
/// 设计: salvo 0.79 Router 没有 `.data()` API, 没法把 Arc<dyn ModelAdapter> 注入到 handler.
/// 改用进程全局 `OnceCell<Arc<dyn ModelAdapter>>`, `run_router(adapter)` 时初始化一次,
/// handler 从全局拿. 多 router 实例会**共享** (适合单进程 server 场景).
static GLOBAL_ADAPTER: OnceCell<Arc<dyn ModelAdapter>> = OnceCell::const_new();

/// 初始化全局 adapter (跟 `run_router` 配对调用)
pub fn set_global_adapter(adapter: Arc<dyn ModelAdapter>) {
    let _ = GLOBAL_ADAPTER.set(adapter);
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

#[cfg(test)]
mod tests {
    // 2026-08-18: 用 salvo::test::TestClient 标准 API
    // 之前 mental commit 写 `router().into_service()` + `salvo::hyper::Body::empty()` 都错.
    // 正确做法: `TestClient::get(url).send(service)` 一行
    use super::*;
    use salvo::test::{ResponseExt, TestClient};

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
}
