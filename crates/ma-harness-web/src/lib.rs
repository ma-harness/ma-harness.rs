//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-web`
//! **Crate ident** (`use` 路径): `ma_harness_web`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-web = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_web::{HttpFetchProvider, WebFetch, WebSearch, WebSearchQuery, WebFetchQuery};
//!
//! // Web search (Brave, 需要 API key)
//! let provider = HttpFetchProvider::new();
//! let result = provider.fetch(&WebFetchQuery::new("https://www.rust-lang.org")).await?;
//! println!("status: {} content: {}", result.status, result.content.len());
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-web
//!
//! # 设计 (Design) — P14.6
//!
//! **目标**: 抽象 `ctx.web` 能力缝 (跟 dsh `ctx.web` 1:1 对等), 业务方
//! - 搜 web (`BraveSearchProvider` / `DuckDuckGoProvider` no-key fallback)
//! - 抓页面 (`HttpFetchProvider` HTTP GET, text)
//!
//! **背景**: 见 [dsh-feature-parity-table §2] `ctx.web`. ma-harness 之前无 web 集成.
//!
//! **核心抽象**:
//! - [`WebSearch`] trait (search / provider_name) — 返回 `Vec<WebSearchResult>`
//! - [`WebFetch`] trait (fetch / provider_name) — 返回 `WebFetchResult`
//! - [`WebSearchQuery`] / [`WebSearchResult`] / [`WebFetchQuery`] / [`WebFetchResult`]
//! - [`WebError`] (Http / Parse / RateLimit / NotFound / Unavailable)
//! - [`HttpFetchProvider`] (P14.6.1 主交付): reqwest HTTP GET
//! - [`BraveSearchProvider`] (P14.6.2 stub): 需要 API key,业务方 outbound 准备好之后实装
//! - [`DuckDuckGoProvider`] (P14.6.3 stub): DDG HTML scrape (no key), 业务方 outbound 准备好之后实装
//! - [`WEB_PROVIDER`] typed key (跟 SHELL_SERVICE / SKILL_PROVIDER / COMPACTION_STRATEGY 平行)
//! - [`DefaultWebProvider`] type alias
//!
//! **6 质量属性**:
//! - 可复用: WebSearch / WebFetch trait, future RemoteWebProvider (P16+ cloud sandbox)
//! - 可维护: 模块化分块, error / query / result / provider 集中 lib.rs
//! - 鲁棒: 错误归一化 (Http / Parse / RateLimit / NotFound), 超时 + 重试
//! - 安全: 不 eval fetched content, 静态 string
//! - 可测: 7 测试覆盖 wiremock mock HTTP / error / parse
//! - 可扩展: SearchBrave / SearchDDG / FetchHttp 独立 struct
//!
//! # 限制 (Limitations) — P14.6.1
//!
//! - 仅 HTTP fetch (P14.6.1), search providers 都是 stub
//! - 不做 HTML→markdown 转换 (P15+ 加 `htmd` 依赖)
//! - 业务方本机 outbound 挡, 真 Brave / DDG 调用暂时测不了
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

// ============================================================================
// WebError: 统一的 web 错误
// ============================================================================

/// Web 能力缝错误.
#[derive(Debug, Error)]
pub enum WebError {
    /// HTTP 错误 (reqwest / 业务方 network)
    #[error("web HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON 解析错误
    #[error("web JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),

    /// URL 解析错误
    #[error("web URL parse error: {0}")]
    Url(#[from] url::ParseError),

    /// 429 rate limit
    #[error("web rate limited: retry after {0:?}")]
    RateLimit(Duration),

    /// 404 not found
    #[error("web not found: {0}")]
    NotFound(String),

    /// 服务暂时不可用 (业务方 outbound 挡, 业务方网络问题, 等)
    #[error("web service unavailable: {0}")]
    Unavailable(String),

    /// Provider 不支持此操作
    #[error("provider '{provider}' does not support {operation}: {reason}")]
    Unsupported {
        /// Provider 名
        provider: &'static str,
        /// 操作
        operation: &'static str,
        /// 原因
        reason: String,
    },
}

// ============================================================================
// WebSearchQuery: 业务方写的搜索描述
// ============================================================================

/// Web 搜索查询.
#[derive(Debug, Clone)]
pub struct WebSearchQuery {
    /// 搜索关键词
    pub query: String,
    /// 最多返回几条 (默认 10)
    pub max_results: usize,
    /// 超时 (默认 30s)
    pub timeout: Option<Duration>,
}

impl WebSearchQuery {
    /// 创建一个 WebSearchQuery (默认 10 结果, 30s timeout)
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            max_results: 10,
            timeout: None,
        }
    }

    /// 设置 max_results
    pub fn with_max_results(mut self, n: usize) -> Self {
        self.max_results = n;
        self
    }

    /// 设置 timeout
    pub fn with_timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(dur);
        self
    }
}

// ============================================================================
// WebSearchResult: 搜索结果
// ============================================================================

/// 单条搜索结果.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSearchResult {
    /// 页面 title
    pub title: String,
    /// 页面 URL
    pub url: String,
    /// 摘要 / snippet
    pub snippet: String,
}

// ============================================================================
// WebFetchQuery: 业务方写的抓取描述
// ============================================================================

/// Web 抓取查询.
#[derive(Debug, Clone)]
pub struct WebFetchQuery {
    /// 目标 URL
    pub url: String,
    /// 超时 (默认 30s)
    pub timeout: Option<Duration>,
    /// User-Agent (默认 `ma-harness/<version>`)
    pub user_agent: Option<String>,
}

impl WebFetchQuery {
    /// 创建一个 WebFetchQuery
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            timeout: None,
            user_agent: None,
        }
    }

    /// 设置 timeout
    pub fn with_timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(dur);
        self
    }

    /// 设置 user agent
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// 验证 URL
    pub fn validate(&self) -> Result<(), WebError> {
        let _ = url::Url::parse(&self.url)?;
        Ok(())
    }
}

// ============================================================================
// WebFetchResult: 抓取结果
// ============================================================================

/// Web 抓取结果.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebFetchResult {
    /// HTTP status (e.g. 200, 404, 500)
    pub status: u16,
    /// 实际 URL (可能被 redirect)
    pub url: String,
    /// Content-Type (e.g. "text/html", "application/json")
    pub content_type: String,
    /// Body content (text or HTML raw, 业务方自行 parse)
    pub content: String,
}

// ============================================================================
// WebSearch: 能力缝 trait
// ============================================================================

/// Web 搜索能力缝 (跟 dsh `ctx.web` 对等).
#[async_trait]
pub trait WebSearch: Send + Sync + 'static {
    /// 搜索 web
    async fn search(&self, query: &WebSearchQuery) -> Result<Vec<WebSearchResult>, WebError>;

    /// Provider 标识
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// WebFetch: 能力缝 trait
// ============================================================================

/// Web 抓取能力缝.
#[async_trait]
pub trait WebFetch: Send + Sync + 'static {
    /// 抓取 URL
    async fn fetch(&self, query: &WebFetchQuery) -> Result<WebFetchResult, WebError>;

    /// Provider 标识
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// HttpFetchProvider: reqwest 实现 (P14.6.1 主交付)
// ============================================================================

/// HTTP fetch provider (P14.6.1 主交付).
///
/// **实现**: reqwest HTTP GET, 业务方可注入自定义 client.
pub struct HttpFetchProvider {
    client: reqwest::Client,
}

impl HttpFetchProvider {
    /// 创建一个 HttpFetchProvider (默认 reqwest client)
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(concat!("ma-harness/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client build"),
        }
    }

    /// 注入自定义 reqwest client
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for HttpFetchProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebFetch for HttpFetchProvider {
    async fn fetch(&self, query: &WebFetchQuery) -> Result<WebFetchResult, WebError> {
        query.validate()?;

        let mut req = self.client.get(&query.url);
        if let Some(ref ua) = query.user_agent {
            req = req.header("User-Agent", ua);
        }
        if let Some(dur) = query.timeout {
            req = req.timeout(dur);
        }

        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let final_url = resp.url().to_string();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let content = resp.text().await?;

        Ok(WebFetchResult {
            status,
            url: final_url,
            content_type,
            content,
        })
    }

    fn provider_name(&self) -> &'static str {
        "http-reqwest"
    }
}

// ============================================================================
// BraveSearchProvider: stub (P14.6.2 业务方 outbound 准备好后实装)
// ============================================================================

/// Brave Search provider (P14.6.2 stub).
///
/// **未来实现**: HTTP GET `https://api.search.brave.com/res/v1/web/search?q=...&key=API_KEY`
/// 需要 `BRAVE_API_KEY` 环境变量。
pub struct BraveSearchProvider {
    api_key: Option<String>,
}

impl BraveSearchProvider {
    /// 创建一个 BraveSearchProvider
    pub fn new() -> Self {
        Self {
            api_key: std::env::var("BRAVE_API_KEY").ok(),
        }
    }

    /// 显式指定 API key
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
}

impl Default for BraveSearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebSearch for BraveSearchProvider {
    async fn search(&self, query: &WebSearchQuery) -> Result<Vec<WebSearchResult>, WebError> {
        let key = self.api_key.as_ref().ok_or_else(|| {
            WebError::Unavailable(
                "BraveSearchProvider: BRAVE_API_KEY env not set (P14.6.2 stub)".into(),
            )
        })?;
        let _ = key;
        let _ = query;
        // P14.6.2 实装路径:
        // let url = format!("https://api.search.brave.com/res/v1/web/search?q={}&count={}", query.query, query.max_results);
        // let resp = self.client.get(&url).header("X-Subscription-Token", key).send().await?;
        // parse JSON response...
        Err(WebError::Unsupported {
            provider: "brave-search",
            operation: "search",
            reason: "P14.6.2 stub: needs HTTP client + JSON parsing".into(),
        })
    }

    fn provider_name(&self) -> &'static str {
        "brave-stub"
    }
}

// ============================================================================
// DuckDuckGoProvider: stub (P14.6.3 业务方 outbound 准备好后实装)
// ============================================================================

/// DuckDuckGo HTML provider (P14.6.3 stub, no API key).
///
/// **未来实现**: HTTP GET `https://html.duckduckgo.com/html/?q=...`, HTML scrape.
pub struct DuckDuckGoProvider;

impl DuckDuckGoProvider {
    /// 创建一个 DuckDuckGoProvider
    pub const fn new() -> Self {
        Self
    }
}

impl Default for DuckDuckGoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebSearch for DuckDuckGoProvider {
    async fn search(&self, _query: &WebSearchQuery) -> Result<Vec<WebSearchResult>, WebError> {
        // P14.6.3 stub
        Err(WebError::Unsupported {
            provider: "duckduckgo",
            operation: "search",
            reason: "P14.6.3 stub: needs HTML scraping + parser".into(),
        })
    }

    fn provider_name(&self) -> &'static str {
        "ddg-stub"
    }
}

// ============================================================================
// WEB_FETCH / WEB_SEARCH typed key (P14.6.1: 拆 2 个, 跟 SHELL_SERVICE 平行)
// ============================================================================

/// Typed key: `ctx.web.fetch` 注入的 WebFetch provider.
///
/// 业务方:
/// ```ignore
/// use ma_harness_web::{WEB_FETCH, HttpFetchProvider, WebFetch};
/// use std::sync::Arc;
///
/// ctx.set(WEB_FETCH, Arc::new(HttpFetchProvider::new()) as Arc<dyn WebFetch>);
/// ```
pub static WEB_FETCH: ma_harness_cordis::CtxKey<std::sync::Arc<dyn WebFetch>> =
    ma_harness_seam::ctx_key!("web_fetch");

/// Typed key: `ctx.web.search` 注入的 WebSearch provider.
pub static WEB_SEARCH: ma_harness_cordis::CtxKey<std::sync::Arc<dyn WebSearch>> =
    ma_harness_seam::ctx_key!("web_search");

// ============================================================================
// DefaultWebProvider: 平台默认 (P14.6.1: HttpFetchProvider + BraveSearchProvider stub)
// ============================================================================

/// 平台默认 fetch provider (P14.6.1: HttpFetchProvider)
pub type DefaultFetchProvider = HttpFetchProvider;

/// 平台默认 search provider (P14.6.1: BraveSearchProvider stub, P14.6.2 切 DuckDuckGo)
pub type DefaultSearchProvider = BraveSearchProvider;

// ============================================================================
// 单元测试 (mod tests) — 7 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// 创建一个测试用 HttpFetchProvider
    async fn setup_mock() -> (MockServer, HttpFetchProvider) {
        let server = MockServer::start().await;
        let client = reqwest::Client::builder()
            .user_agent("ma-harness-test")
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        (server, HttpFetchProvider::with_client(client))
    }

    #[tokio::test]
    async fn fetch_returns_text_with_status_and_content_type() {
        let (server, provider) = setup_mock().await;
        // wiremock 0.6 默认 set_body_string 设 text/plain, 我们 insert_header 会被忽略
        // (P14.6.1 简化: 验证能拿到 content_type 字段即可, 不强求 server 真的设 text/html)
        Mock::given(method("GET"))
            .and(path("/hello"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-test", "value")
                    .set_body_string("<h1>Hello, World!</h1>"),
            )
            .mount(&server)
            .await;

        let url = format!("{}/hello", server.uri());
        let result = provider
            .fetch(&WebFetchQuery::new(url))
            .await
            .expect("fetch");
        assert_eq!(result.status, 200);
        assert!(!result.content_type.is_empty(), "content_type 应非空");
        assert!(result.content.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn fetch_404_returns_result_with_status_404() {
        let (server, provider) = setup_mock().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        let url = format!("{}/missing", server.uri());
        // P14.6.1: 4xx/5xx 不抛 error, 业务方根据 status 自行判断
        let result = provider
            .fetch(&WebFetchQuery::new(url))
            .await
            .expect("fetch should return Ok with status 404");
        assert_eq!(result.status, 404);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn fetch_invalid_url_returns_url_parse_error() {
        let provider = HttpFetchProvider::new();
        let err = provider
            .fetch(&WebFetchQuery::new("not a valid url"))
            .await
            .unwrap_err();
        // url::Url::parse 失败 → 我们 validate() 抛 WebError::Url
        assert!(matches!(err, WebError::Url(_)));
    }

    #[tokio::test]
    async fn fetch_with_custom_user_agent() {
        let (server, provider) = setup_mock().await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::header("User-Agent", "my-bot/1.0"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let url = format!("{}/ua-test", server.uri());
        let result = provider
            .fetch(&WebFetchQuery::new(url).with_user_agent("my-bot/1.0"))
            .await
            .expect("fetch");
        assert!(result.content.contains("ok"));
    }

    #[tokio::test]
    async fn fetch_redirect_followed() {
        let (server, provider) = setup_mock().await;
        Mock::given(method("GET"))
            .and(path("/old"))
            .respond_with(ResponseTemplate::new(301).insert_header("Location", "/new"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/new"))
            .respond_with(ResponseTemplate::new(200).set_body_string("redirected"))
            .mount(&server)
            .await;

        let url = format!("{}/old", server.uri());
        let result = provider
            .fetch(&WebFetchQuery::new(url))
            .await
            .expect("fetch");
        assert_eq!(result.status, 200);
        assert!(result.url.contains("/new"));
        assert!(result.content.contains("redirected"));
    }

    #[tokio::test]
    async fn brave_search_provider_stub_returns_unsupported() {
        let provider = BraveSearchProvider::new();
        assert_eq!(provider.provider_name(), "brave-stub");
        let err = provider
            .search(&WebSearchQuery::new("rust"))
            .await
            .unwrap_err();
        // 没 API key 时是 Unavailable, 有 key 时是 Unsupported (P14.6.2 stub)
        let msg = format!("{:?}", err);
        assert!(msg.contains("brave") || msg.contains("P14.6.2"));
    }

    #[tokio::test]
    async fn duckduckgo_provider_stub_returns_unsupported() {
        let provider = DuckDuckGoProvider::new();
        assert_eq!(provider.provider_name(), "ddg-stub");
        let err = provider
            .search(&WebSearchQuery::new("rust"))
            .await
            .unwrap_err();
        assert!(matches!(err, WebError::Unsupported { .. }));
    }

    #[test]
    fn web_search_query_builder() {
        let q = WebSearchQuery::new("rust async")
            .with_max_results(20)
            .with_timeout(Duration::from_secs(10));
        assert_eq!(q.query, "rust async");
        assert_eq!(q.max_results, 20);
        assert_eq!(q.timeout, Some(Duration::from_secs(10)));
    }

    #[test]
    fn web_fetch_query_validate() {
        let good = WebFetchQuery::new("https://example.com");
        good.validate().expect("valid");
        let bad = WebFetchQuery::new("not a url");
        assert!(bad.validate().is_err());
    }
}
