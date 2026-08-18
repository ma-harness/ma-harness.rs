//! HTTP 服务 (salvo): /health + /version
//!
//! 2026-08-18: 从 axum 迁移到 salvo (见 decision-log §12).
//! 迁移动机: salvo 内置 OpenAPI 导出, 编译更快, 跟 ma-harness 风格更贴.
//!
//! Phase 2 加: /v1/sessions, /v1/runs (REST endpoint) + OpenAPI 导出 (#[endpoint] macro).

use salvo::prelude::*;
use serde_json::json;

/// 构造 salvo Router
///
/// 用 `Router::with_path(...).get(handler)` 替代 axum 的 `.route(path, get(handler))`.
/// salvo 推荐 push 多个 sub-router, 风格跟 axum 略有不同但等价.
pub fn router() -> Router {
    Router::new()
        .push(Router::with_path("health").get(health))
        .push(Router::with_path("version").get(version))
}

/// /health 处理器
///
/// `#[handler]` macro 让 async fn 变成 salvo Handler, 等价 axum 的 `async fn` + 自动 impl Handler.
#[handler]
async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "ma-harness",
    }))
}

/// /version 处理器
#[handler]
async fn version() -> Json<serde_json::Value> {
    Json(json!({
        "name": "ma-harness",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[cfg(test)]
mod tests {
    // 2026-08-18: 用 salvo::test::TestClient 标准 API
    // 之前 mental commit 写 `router().into_service()` + `salvo::hyper::Body::empty()` 都错.
    // 正确做法: `TestClient::get(url).send(service)` 一行.
    use super::*;
    use salvo::test::TestClient;

    #[tokio::test]
    async fn health_returns_ok() {
        let service = Service::new(router());
        let resp = TestClient::get("http://localhost/health")
            .send(&service)
            .await;
        assert_eq!(resp.status_code(), salvo::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn version_returns_ok() {
        let service = Service::new(router());
        let resp = TestClient::get("http://localhost/version")
            .send(&service)
            .await;
        assert_eq!(resp.status_code(), salvo::http::StatusCode::OK);
    }
}
