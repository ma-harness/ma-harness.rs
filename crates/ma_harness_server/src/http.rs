//! HTTP 服务 (axum): /health + 路由
//!
//! Week 1 Day 18 实现: 最简 /health. Phase 2 加 /v1/sessions, /v1/runs 等 REST endpoint.

use axum::{routing::get, Json, Router};
use serde_json::json;

/// 构造 axum Router
pub fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "ma-harness",
    }))
}

async fn version() -> Json<serde_json::Value> {
    Json(json!({
        "name": "ma-harness",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_returns_ok() {
        let app = router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn version_returns_ok() {
        let app = router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
