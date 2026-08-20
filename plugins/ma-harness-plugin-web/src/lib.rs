//! ma_harness_plugin_web ?first-party plugin: HTTP / HTTPS 请求
//!
//! **设计**: seam 公开 API 风格.
//!
//! **Week 5-6 实装**: 2 个核心方?(http_get / http_post) + URL 白名?//! (EGRESS_ALLOW_LIST) + 超时 (TIMEOUT_MS).
//!
//! **Phase 1 简?*:
//! - URL 白名单用字符串前缀 (Phase 2 加更严格匹配)
//! - 没有 DNS 防泄?(Phase 2)
//! - 没有 cookie 持久?(Phase 2)

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(missing_docs)] // 2026-08-18: 内部 crate, 暂不强制 doc (Phase 2 release 前补)

use std::time::Duration;

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// 公开 typed key
// ============================================================================

/// 出站 URL 白名?(前缀匹配, 业务?set)
pub static EGRESS_ALLOW_LIST: ma_harness_cordis::CtxKey<Vec<String>> =
    ctx_key!("egress_allow_list");

/// HTTP 请求超时 (ms)
pub static TIMEOUT_MS: ma_harness_cordis::CtxKey<u32> = ctx_key!("timeout_ms");

/// 默认超时 (ms)
pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;

// ============================================================================
// 错误
// ============================================================================

/// Web plugin 错误
#[derive(Debug, Error)]
pub enum WebError {
    /// URL 不在白名单
    #[error("url {url} not in egress allow list {list:?}")]
    NotInAllowList {
        /// URL
        url: String,
        /// 白名单
        list: Vec<String>,
    },

    /// URL 解析失败
    #[error("invalid url {0}: {1}")]
    InvalidUrl(String, String),

    /// HTTP 错误
    #[error("http error: {0}")]
    Http(String),

    /// reqwest 错误
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
}

// ============================================================================
// HttpResponse
// ============================================================================

/// HTTP 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    /// HTTP 状态码
    pub status: u16,
    /// 响应?(Content-Type ?
    pub content_type: String,
    /// 响应 body (字符? 业务方按需 parse)
    pub body: String,
}

impl HttpResponse {
    /// 是否 2xx 成功
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

// ============================================================================
// WebService
// ============================================================================

/// Web service ?sandbox 限制?HTTP client
pub struct WebService {
    client: reqwest::Client,
}

impl WebService {
    /// 构?(Phase 1: 默认 client, Phase 2 ?connection pool / proxy)
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("ma-harness/0.1.0")
            .build()
            .expect("reqwest client build");
        Self { client }
    }

    /// HTTP GET
    pub async fn http_get(&self, ctx: &Context, url: &str) -> Result<HttpResponse, WebError> {
        self.check_allow_list(ctx, url)?;
        let timeout = self.get_timeout(ctx);
        let resp = self.client.get(url).timeout(timeout).send().await?;
        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = resp.text().await?;
        Ok(HttpResponse {
            status,
            content_type,
            body,
        })
    }

    /// HTTP POST (body 是字符串)
    pub async fn http_post(
        &self,
        ctx: &Context,
        url: &str,
        body: &str,
        content_type: &str,
    ) -> Result<HttpResponse, WebError> {
        self.check_allow_list(ctx, url)?;
        let timeout = self.get_timeout(ctx);
        let resp = self
            .client
            .post(url)
            .header("content-type", content_type)
            .body(body.to_string())
            .timeout(timeout)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();
        let resp_body = resp.text().await?;
        Ok(HttpResponse {
            status,
            content_type: ct,
            body: resp_body,
        })
    }

    /// 检?URL 是否在白名单
    fn check_allow_list(&self, ctx: &Context, url: &str) -> Result<(), WebError> {
        let allows = ctx.get(EGRESS_ALLOW_LIST).unwrap_or_default();
        if allows.is_empty() {
            return Err(WebError::NotInAllowList {
                url: url.to_string(),
                list: allows,
            });
        }
        for prefix in &allows {
            if url.starts_with(prefix) {
                return Ok(());
            }
        }
        Err(WebError::NotInAllowList {
            url: url.to_string(),
            list: allows,
        })
    }

    /// ?ctx.TIMEOUT_MS, fallback default
    fn get_timeout(&self, ctx: &Context) -> Duration {
        Duration::from_millis(ctx.get(TIMEOUT_MS).unwrap_or(DEFAULT_TIMEOUT_MS) as u64)
    }
}

impl Default for WebService {
    fn default() -> Self {
        Self::new()
    }
}

impl CordisService for WebService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(WebService::new())
    }
    fn name(&self) -> &str {
        "web"
    }
}

impl SeamService for WebService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(WebService::new())
    }
    fn name(&self) -> &str {
        "web"
    }
}

// ============================================================================
// Plugin: WebPlugin
// ============================================================================

pub struct WebPlugin;

impl CordisPlugin for WebPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = <WebService as ma_harness_cordis::Service>::install(ctx)?;
        ctx.inject(std::sync::Arc::new(svc));
        ctx.set(EGRESS_ALLOW_LIST, Vec::<String>::new());
        ctx.set(TIMEOUT_MS, DEFAULT_TIMEOUT_MS);
        Ok(())
    }
    fn name(&self) -> &str {
        "web"
    }
}

impl SeamPlugin for WebPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "web"
    }
}

// ============================================================================
// 单元测试 (?wiremock 模拟 server)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx_with_mock_url(mock_uri: String) -> Context {
        let ctx = Context::new();
        ctx.set(EGRESS_ALLOW_LIST, vec![mock_uri]);
        ctx.set(TIMEOUT_MS, 5000u32);
        ctx
    }

    #[tokio::test]
    async fn http_get_success() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello world"))
            .mount(&mock)
            .await;

        let url = format!("{}/hello", mock.uri());
        let ctx = ctx_with_mock_url(mock.uri());
        let svc = WebService::new();

        let resp = svc.http_get(&ctx, &url).await.unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.is_success());
        assert_eq!(resp.body, "hello world");
    }

    #[tokio::test]
    async fn http_post_success() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .respond_with(ResponseTemplate::new(201).set_body_string("accepted"))
            .mount(&mock)
            .await;

        let url = format!("{}/echo", mock.uri());
        let ctx = ctx_with_mock_url(mock.uri());
        let svc = WebService::new();

        let resp = svc
            .http_post(&ctx, &url, "payload", "text/plain")
            .await
            .unwrap();
        assert_eq!(resp.status, 201);
        assert!(resp.is_success());
    }

    #[tokio::test]
    async fn http_get_outside_allow_list_errors() {
        let ctx = Context::new();
        ctx.set(
            EGRESS_ALLOW_LIST,
            vec!["https://allowed.example.com".to_string()],
        );
        let svc = WebService::new();
        let result = svc.http_get(&ctx, "https://evil.example.com").await;
        assert!(matches!(result, Err(WebError::NotInAllowList { .. })));
    }

    #[tokio::test]
    async fn http_get_with_empty_allow_list_errors() {
        let ctx = Context::new();
        ctx.set(EGRESS_ALLOW_LIST, Vec::<String>::new());
        let svc = WebService::new();
        let result = svc.http_get(&ctx, "https://anywhere.example.com").await;
        assert!(matches!(result, Err(WebError::NotInAllowList { .. })));
    }

    #[tokio::test]
    async fn http_get_4xx_response() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&mock)
            .await;

        let url = format!("{}/missing", mock.uri());
        let ctx = ctx_with_mock_url(mock.uri());
        let svc = WebService::new();

        let resp = svc.http_get(&ctx, &url).await.unwrap();
        assert_eq!(resp.status, 404);
        assert!(!resp.is_success());
        assert_eq!(resp.body, "not found");
    }
}
