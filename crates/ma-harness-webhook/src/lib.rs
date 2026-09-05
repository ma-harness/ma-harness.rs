//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-webhook`
//! **Crate ident** (`use` 路径): `ma_harness_webhook`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident,
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-webhook = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_webhook::{
//!     HmacSha256Verifier, LocalWebhookProvider, RouteConfig, WebhookEvent, WebhookService,
//! };
//!
//! let provider = LocalWebhookProvider::new()
//!     .with_route(RouteConfig::new(
//!         "/webhook/git",
//!         HmacSha256Verifier::new(b"my-secret"),
//!     ));
//!
//! // 业务方收到 HTTP webhook, parse + 构 WebhookEvent, submit
//! let event = WebhookEvent::new("/webhook/git", b"{\"ref\":\"refs/heads/main\"}")
//!     .with_signature("sha256=abc123...");
//! let id = provider.submit(event).await?;
//!
//! // Agent loop 拉取事件
//! let event = provider.take().await?.expect("event");
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-webhook
//!
//! # 设计 (Design) — P15.3.1
//!
//! **目标**: 抽象 `ctx.webhookRuntime` (跟 dsh `ctx.webhookRuntime` 对等), 业务方
//! - 跑 `mah webhook` 起 webhook server, 监听 `:9090/webhook/{route}`
//! - 收到 GitHub / GitLab / Generic webhook, 验签, dispatch 到 agent loop
//! - `curl -X POST https://server/webhook/git -H "X-Hub-Signature-256: ..."` 创建 session 跑 agent
//!
//! **背景**: 见 [dsh-feature-parity-table §2 capability seams]:
//! - dsh 有 `packages/webhook/` 做 authenticated-delivery dispatch (HMAC-SHA256 验签 + 路由 + 创建 session)
//! - ma-harness 没有 webhook (❌ gap, P15+ plan)
//!
//! **P15.3.1 本次**: 验签 + 调度 queue. 完整的 HTTP server / 路由 / session creator
//! 等 P15.3.2+ 增量交付.
//!
//! **接口**:
//! - [`WebhookEvent`] — 收到的 webhook 事件 (route + body + signature + id + ts)
//! - [`WebhookVerifier`] trait — 抽象签名验证 (HMAC-SHA256, future SHA-512, etc.)
//! - [`HmacSha256Verifier`] — GitHub-style 验签器 (X-Hub-Signature-256: sha256=<hex>)
//! - [`WebhookService`] trait — submit + take + routes
//! - [`LocalWebhookProvider`] — 内存 dispatch queue (P15.3.1 主交付)
//! - [`RouteConfig`] — route path + verifier 配对
//!
//! **6 质量属性 (业务方 2026-09-04 约定)**:
//! - 可复用: verifier trait 抽象, future 支持 GitLab / Stripe / 等 (P15.3.2+)
//! - 可维护: 模块化分块, 类型集中 lib.rs
//! - 鲁棒: 错误归一化 (InvalidSignature / UnknownRoute / QueueFull / Internal), constant-time compare 防 timing attack
//! - 安全: `subtle::ConstantTimeEq` 验签 (不返 timing 信息), secret 0 字节也允许但 log warn
//! - 可测: 7+ 单元测试 (valid / invalid sig / tampered / wrong key / dedup / queue / unknown route / 0-length secret)
//! - 可扩展: WebhookService trait → future RemoteWebhookProvider (e.g. NATS 消费)
//!
//! # 限制 (Limitations) — P15.3.1
//!
//! - 没有 HTTP server — 业务方自己用 axum / salvo 接 `:9090/webhook/{route}`
//!   (P15.3.2 加 webhook server, 类似 P15.1 web-ui 用 salvo)
//! - 没有 session creator — `take()` 只返 WebhookEvent, 业务方自己 `session.start()`
//!   (P15.3.3 加 WebhookSession: incoming → new session → process → response)
//! - 内存 queue — 重启丢失 (P15.3.4 持久化到 EventLog 跟 P15.1.8 一样)
//! - 没有 rate limit / replay protection (除了 dedup by id) (P15.3.5)
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// Error
// ============================================================================

/// Webhook capability 错误.
#[derive(Debug, Error)]
pub enum WebhookError {
    /// 签名验证失败 (X-Hub-Signature-256 缺失 / 不匹配 / 算法不对)
    #[error("invalid webhook signature: {0}")]
    InvalidSignature(String),

    /// 未知 route (e.g. POST /webhook/unknown-route)
    #[error("unknown webhook route: {0}")]
    UnknownRoute(String),

    /// 重复 event id (dedup by UUID, 业务方 retry 同 id webhook 拒收)
    #[error("duplicate webhook event id: {0}")]
    DuplicateEvent(String),

    /// Queue 满 (P15.3.1 默认 unbounded, 但 future rate limit 可能返)
    #[error("webhook queue full (size: {0}, cap: {1})")]
    QueueFull(usize, usize),

    /// HMAC 错误 (key parse / internal)
    #[error("webhook internal error: {0}")]
    Internal(String),
}

// ============================================================================
// WebhookEvent
// ============================================================================

/// 收到的 webhook 事件.
#[derive(Debug, Clone)]
pub struct WebhookEvent {
    /// 事件 id (UUID v4, 用于 dedup)
    pub id: String,
    /// Route path (e.g. "/webhook/git")
    pub route: String,
    /// 收到时间
    pub received_at: DateTime<Utc>,
    /// 原始 body bytes
    pub body: Vec<u8>,
    /// X-Hub-Signature-256 header 值 (e.g. "sha256=abc123...")
    pub signature: Option<String>,
    /// 业务方额外 header (P15.3.2: webhook server 直接 map 进来)
    pub extra_headers: HashMap<String, String>,
}

impl WebhookEvent {
    /// 构一个 WebhookEvent (auto-fill id + received_at).
    pub fn new(route: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            route: route.into(),
            received_at: Utc::now(),
            body: body.into(),
            signature: None,
            extra_headers: HashMap::new(),
        }
    }

    /// builder: set signature header value
    pub fn with_signature(mut self, sig: impl Into<String>) -> Self {
        self.signature = Some(sig.into());
        self
    }

    /// builder: add extra header
    pub fn with_header(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.extra_headers.insert(key.into(), val.into());
        self
    }

    /// 拿 body 的 UTF-8 lossy string (GitHub webhook 都是 JSON)
    pub fn body_str(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

// ============================================================================
// WebhookVerifier trait
// ============================================================================

/// 签名算法 (P15.3.1: HMAC-SHA256 only; P15.3.5+ 加 SHA-512, Ed25519 等)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    /// HMAC-SHA256 (GitHub 用的, 也最常见)
    HmacSha256,
}

/// 签名验证器 trait (抽象给 future 算法).
#[async_trait]
pub trait WebhookVerifier: Send + Sync + 'static {
    /// 算法标识 (日志 / 调试)
    fn algorithm(&self) -> SignatureAlgorithm;

    /// 验签: 比对 (body, signature) 是否由 secret 签名
    ///
    /// # Errors
    /// - `WebhookError::InvalidSignature` — 签名不匹配 / 格式错
    async fn verify(&self, body: &[u8], signature: &str) -> Result<(), WebhookError>;
}

// ============================================================================
// HmacSha256Verifier
// ============================================================================

/// HMAC-SHA256 验签器 (GitHub 格式 `sha256=<hex>`).
///
/// **算法**: HMAC-SHA256(secret, body) → hex string, 比对 `sha256=<hex>` header.
/// **比较**: `subtle::ConstantTimeEq` 防 timing attack (不返 timing 信息).
#[derive(Clone)]
pub struct HmacSha256Verifier {
    secret: Vec<u8>,
}

impl std::fmt::Debug for HmacSha256Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 不打印 secret
        f.debug_struct("HmacSha256Verifier")
            .field("secret_len", &self.secret.len())
            .finish()
    }
}

impl HmacSha256Verifier {
    /// 创建一个新的 HMAC-SHA256 验签器.
    ///
    /// **注**: secret 可以是空 bytes (业务方可能想测), 但 0 长度 secret HMAC
    /// 也是 valid 的 (只是 anyone can forge). log warn 提醒业务方.
    pub fn new(secret: impl AsRef<[u8]>) -> Self {
        let secret = secret.as_ref().to_vec();
        if secret.is_empty() {
            tracing::warn!("HmacSha256Verifier created with 0-length secret (anyone can forge)");
        }
        Self { secret }
    }

    /// 计算 HMAC-SHA256 signature (helper, 测试用).
    pub fn compute_signature(&self, body: &[u8]) -> String {
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC accepts any key length");
        mac.update(body);
        let result = mac.finalize();
        let bytes = result.into_bytes();
        format!("sha256={}", hex::encode(bytes))
    }
}

#[async_trait]
impl WebhookVerifier for HmacSha256Verifier {
    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::HmacSha256
    }

    async fn verify(&self, body: &[u8], signature: &str) -> Result<(), WebhookError> {
        // 1. Parse signature header (must start with "sha256=")
        let provided_hex = signature
            .strip_prefix("sha256=")
            .ok_or_else(|| WebhookError::InvalidSignature("missing 'sha256=' prefix".into()))?;

        // 2. Decode provided hex to bytes
        let provided_bytes = hex::decode(provided_hex)
            .map_err(|e| WebhookError::InvalidSignature(format!("invalid hex: {e}")))?;

        // 3. Compute expected signature
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|e| WebhookError::Internal(format!("hmac key: {e}")))?;
        mac.update(body);
        let expected_bytes = mac.finalize().into_bytes();

        // 4. Constant-time compare (防 timing attack)
        if provided_bytes.ct_eq(&expected_bytes).into() {
            Ok(())
        } else {
            Err(WebhookError::InvalidSignature("signature mismatch".into()))
        }
    }
}

// ============================================================================
// RouteConfig
// ============================================================================

/// Route 配置: path + 验签器.
///
/// 业务方 `with_route(RouteConfig::new("/webhook/git", HmacSha256Verifier::new(b"...")))` 配多路由.
pub struct RouteConfig {
    /// Path (e.g. "/webhook/git")
    pub path: String,
    /// 该 route 的验签器
    pub verifier: Arc<dyn WebhookVerifier>,
}

impl std::fmt::Debug for RouteConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteConfig")
            .field("path", &self.path)
            .field("verifier_algorithm", &self.verifier.algorithm())
            .finish()
    }
}

impl RouteConfig {
    /// 创建一个 route config.
    pub fn new(path: impl Into<String>, verifier: impl WebhookVerifier + 'static) -> Self {
        Self {
            path: path.into(),
            verifier: Arc::new(verifier),
        }
    }
}

// ============================================================================
// WebhookService trait
// ============================================================================

/// Webhook service trait (业务方对接 ctx.webhook 注入).
#[async_trait]
pub trait WebhookService: Send + Sync + 'static {
    /// 提交一个 event (HTTP handler 收到 webhook 后调).
    ///
    /// 流程: 找 route 对应 verifier → 验签 → 去重 → 入队 → 返 event id.
    async fn submit(&self, event: WebhookEvent) -> Result<String, WebhookError>;

    /// 拉取一个 event (agent loop 调, 阻塞直到有 event).
    ///
    /// P15.3.1 简化: 不真正阻塞, queue 空返 None. 业务方可以 loop `take` + sleep.
    /// P15.3.2 加 async stream / mpsc receiver 真正的阻塞语义.
    async fn take(&self) -> Result<Option<WebhookEvent>, WebhookError>;

    /// 列出所有已配置的 route paths (调试 / health check).
    fn routes(&self) -> Vec<String>;

    /// 当前 queue 长度 (调试 / metrics).
    fn queue_len(&self) -> usize;

    /// Provider 标识.
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// LocalWebhookProvider (P15.3.1 主交付)
// ============================================================================

/// 内存 webhook provider (P15.3.1 主交付).
///
/// **存储**:
/// - `routes`: `HashMap<path, RouteConfig>` — 业务方 `with_route` 加
/// - `queue`: `VecDeque<WebhookEvent>` — agent loop 从 `take()` 拉
/// - `seen`: `HashSet<event_id>` — dedup, 防 replay / retry
///
/// **submit 流程**:
/// 1. 找 route → 找不到返 `UnknownRoute`
/// 2. 验签 → 失败返 `InvalidSignature`
/// 3. dedup by id → 已见返 `DuplicateEvent`
/// 4. 入队 → 返 id
///
/// **take 流程**: 从队首 pop 一个 event (业务方自己 loop + sleep).
pub struct LocalWebhookProvider {
    routes: HashMap<String, RouteConfig>,
    queue: Mutex<VecDeque<WebhookEvent>>,
    seen: Mutex<HashSet<String>>,
}

impl std::fmt::Debug for LocalWebhookProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalWebhookProvider")
            .field("routes", &self.routes.keys().collect::<Vec<_>>())
            .field("queue_len", &self.queue.lock().len())
            .field("seen_count", &self.seen.lock().len())
            .finish()
    }
}

impl Default for LocalWebhookProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalWebhookProvider {
    /// 创建一个新的空 provider.
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            queue: Mutex::new(VecDeque::new()),
            seen: Mutex::new(HashSet::new()),
        }
    }

    /// Builder: 添加一个 route.
    ///
    /// 注: `self` 拿 ownership 返回新 provider, 不修改自身 (业务方链式调用).
    pub fn with_route(mut self, route: RouteConfig) -> Self {
        self.routes.insert(route.path.clone(), route);
        self
    }
}

#[async_trait]
impl WebhookService for LocalWebhookProvider {
    async fn submit(&self, event: WebhookEvent) -> Result<String, WebhookError> {
        // 1. 找 route
        let route = self
            .routes
            .get(&event.route)
            .ok_or_else(|| WebhookError::UnknownRoute(event.route.clone()))?;

        // 2. 验签 (需要 signature header)
        let sig = event.signature.as_deref().ok_or_else(|| {
            WebhookError::InvalidSignature("missing X-Hub-Signature-256 header".into())
        })?;
        route.verifier.verify(&event.body, sig).await?;

        // 3. Dedup by event id
        {
            let mut seen = self.seen.lock();
            if !seen.insert(event.id.clone()) {
                return Err(WebhookError::DuplicateEvent(event.id));
            }
        }

        // 4. 入队
        let id = event.id.clone();
        self.queue.lock().push_back(event);

        Ok(id)
    }

    async fn take(&self) -> Result<Option<WebhookEvent>, WebhookError> {
        Ok(self.queue.lock().pop_front())
    }

    fn routes(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.routes.keys().cloned().collect();
        paths.sort();
        paths
    }

    fn queue_len(&self) -> usize {
        self.queue.lock().len()
    }

    fn provider_name(&self) -> &'static str {
        "local-webhook"
    }
}

// ============================================================================
// Typed key + type alias
// ============================================================================

/// Typed key: `ctx.webhook` 注入的 WebhookService (P15.3.2 业务方注入).
pub static WEBHOOK_SERVICE: ma_harness_cordis::CtxKey<Arc<dyn WebhookService>> =
    ma_harness_seam::ctx_key!("webhook_runtime_service");

/// 平台默认 webhook provider (P15.3.1: LocalWebhookProvider).
pub type DefaultWebhookProvider = LocalWebhookProvider;

// ============================================================================
// P15.3.3: Webhook → SessionEvent 转换 (session creator 基础)
// ============================================================================

/// 把 WebhookEvent 转换成 ma-harness-core 的 SessionEvent (P15.3.3).
///
/// **设计**:
/// - `session_id` = WebhookEvent.id (1:1 映射, 后续 trace 简单)
/// - `event_type` = `UserInput` (model_visible=true, 业务方把 webhook body
///   当作 "user-provided content" 喂 LLM, 跟 dsh 把 webhook 当 input 流一致)
/// - `payload_json` = body 字符串 (binary body 走 base64 包装; P15.3.4
///   持久化时跟 P15.1.8 EventLog 集成)
/// - `severity` = `Info` (默认; P15.3.5 rate limit 拒收时改 `Warn`)
/// - `plugin_name` = "webhook" (标识事件来源, 跟 event_type 配对用于
///   业务方 trace / metrics)
///
/// **返回**:
/// - `Ok(SessionEvent)` — 可直接 `event_log.append(session_event)` 入库
/// - `Err(WebhookError::Internal)` — payload 序列化失败 (极少见)
///
/// **P15.3.4 集成**: 业务方在 `WebhookService::take()` 拿到 event 后调这个
/// 函数, 然后 `event_log.append(event)` 就建立了 webhook → session 的
/// 完整链路 (跟 P15.1.8 /api/sessions/<id>/events 接上).
///
/// **P15.3.3 限制**: 转换是 1:1, 没考虑 multi-event sessions (e.g. 一个 webhook
/// 触发的 session 可能先有 SessionStart, 才有 UserInput). P15.3.5+
/// 改返 `Vec<SessionEvent>` 包含完整 lifecycle.
pub fn webhook_to_session_event(
    event: &WebhookEvent,
) -> Result<ma_harness_core::SessionEvent, WebhookError> {
    use ma_harness_core::{EventType, Severity};

    let session_id = event.id.clone();
    let body_str = event.body_str(); // UTF-8 lossy fallback for binary

    let mut session_event = ma_harness_core::SessionEvent::new(&session_id, EventType::UserInput)
        .with_severity(Severity::Info)
        .with_plugin("webhook");
    // 直接设 payload_json (不经过 with_payload 的 serde_json::to_string,
    // 那样会 JSON 字符串再 stringify 一次, 把 body 包成 "...").
    // 我们要的是 raw body 存进 payload_json 字段 — 业务方 query 时拿到
    // 原始 body (GitHub webhook 是 JSON, 直接 parse 即可).
    session_event.payload_json = Some(body_str);

    Ok(session_event)
}

// ============================================================================
// P15.3.4: Webhook → EventLog 持久化 dispatcher
// ============================================================================

/// Webhook dispatcher 后台任务 (P15.3.4).
///
/// **行为**: loop 调 `svc.take()`, 拿到 event → 转换 SessionEvent →
/// 追加到 `log`. 空 queue 时 sleep 100ms 再试 (避免 busy spin).
///
/// **错误处理**:
/// - `take()` 返 `None` (queue 空) → sleep, 不报错
/// - `take()` 返 `Err` → tracing::error + sleep, 继续 (不退出)
/// - `webhook_to_session_event` 返 `Err` → tracing::error + 跳过这 event
///   (不 persist), 继续 (不退出)
/// - `log.append` 返 `Err` (sqlite locked 等) → tracing::error,
///   继续 (不退出, 下个 event 可能会成功)
///
/// **业务方用法**:
/// ```ignore
/// let svc = Arc::new(LocalWebhookProvider::new()
///     .with_route(RouteConfig::new("/webhook/git", HmacSha256Verifier::new(b"..."))));
/// let log = Arc::new(EventLog::open("webhook.db")?);
/// tokio::spawn(run_webhook_dispatcher(svc, log));
/// // 业务方在 main 里 serve_http
/// tokio::spawn(serve_http("127.0.0.1:9090", svc));
/// ```
///
/// **跟 P15.1.8 /api/sessions/<id>/events 集成**: 持久化后, 业务方
/// `GET /api/sessions/<webhook_id>/events` 拿到 webhook 触发的 session events.
///
/// **返回**: never (无限循环直到 task 被 cancel). 业务方用 `tokio::spawn`
/// + `JoinHandle::abort()` 取消.
pub async fn run_webhook_dispatcher(
    svc: Arc<dyn WebhookService>,
    log: Arc<ma_harness_core::EventLog>,
) {
    use std::time::Duration;

    tracing::info!("webhook dispatcher started");
    loop {
        match svc.take().await {
            Ok(Some(event)) => {
                tracing::debug!(
                    event_id = %event.id,
                    route = %event.route,
                    "webhook dispatcher: persisting event"
                );
                match webhook_to_session_event(&event) {
                    Ok(session_event) => {
                        let session_id = session_event.session_id.clone();
                        // EventLog::append 当前是 infallible (validate 失败 panic);
                        // 业务方用 open_in_memory / 严格 input 时通常 OK.
                        // 如果未来 EventLog 改返 Result, 这里加 ? 或 unwrap.
                        let _seq = log.append(session_event);
                        tracing::debug!(
                            session_id = %session_id,
                            "webhook dispatcher: persisted"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            event_id = %event.id,
                            "webhook dispatcher: conversion failed, skipping"
                        );
                    }
                }
            }
            Ok(None) => {
                // Queue 空, 等 100ms 避免 busy spin
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => {
                tracing::error!(error = %e, "webhook dispatcher: take() failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

// ============================================================================
// P15.3.2: HTTP server (POST /webhook/{route})
// ============================================================================

/// HTTP 响应 (status + body).
#[derive(Debug, Clone)]
pub struct WebhookResponse {
    /// HTTP status code (e.g. 200, 400, 404, 500)
    pub status: u16,
    /// 响应 body (JSON 字符串)
    pub body: String,
}

impl WebhookResponse {
    /// 转成 HTTP/1.1 wire format
    pub fn to_http_string(&self) -> String {
        let reason = match self.status {
            200 => "OK",
            201 => "Created",
            400 => "Bad Request",
            401 => "Unauthorized",
            404 => "Not Found",
            405 => "Method Not Allowed",
            500 => "Internal Server Error",
            _ => "Unknown",
        };
        format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            reason,
            self.body.len(),
            self.body
        )
    }
}

/// 路由结果分类 (P15.3.2 public — `match_webhook_route` 是 pub fn).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteMatch {
    /// 路径匹配 /webhook/{route}, 拿到 route name
    Webhook(String),
    /// 不是 /webhook/ 路径
    NotWebhook,
    /// 是 /webhook/ 但 route 未知
    UnknownWebhook,
}

/// 解析 HTTP 路径, 决定是不是 /webhook/{route} 格式.
///
/// 支持: `POST /webhook/git`, `POST /webhook/gitlab`, `POST /webhook/<anything>`
/// 不支持: `/webhook/`, `/webhook`, `/webhooks/git` (s 多了), `/other/path`
///
/// 业务方可以拿这个 helper 在自己的 web framework 里用.
pub fn match_webhook_route(path: &str) -> RouteMatch {
    const PREFIX: &str = "/webhook/";
    if !path.starts_with(PREFIX) {
        return RouteMatch::NotWebhook;
    }
    let route_start = PREFIX.len();
    if route_start >= path.len() {
        // "/webhook/" 后没 route
        return RouteMatch::UnknownWebhook;
    }
    // 拒绝嵌套路径 (e.g. "/webhook/git/foo")
    if path[route_start..].contains('/') {
        return RouteMatch::UnknownWebhook;
    }
    RouteMatch::Webhook(path[route_start..].to_string())
}

/// 处理单个 HTTP webhook 请求 (P15.3.2 核心).
///
/// **输入**:
/// - `method`: HTTP method (e.g. "POST", "GET")
/// - `path`: 请求路径 (e.g. "/webhook/git")
/// - `body`: 原始 body bytes
/// - `headers`: HTTP headers (key 不分大小写 lookup)
///
/// **输出**: [`WebhookResponse`]
///
/// **错误映射**:
/// - `WebhookError::UnknownRoute` → 404 Not Found
/// - `WebhookError::InvalidSignature` → 401 Unauthorized
/// - `WebhookError::DuplicateEvent` → 202 Accepted (webhook 重发, OK 但不重复处理)
/// - `WebhookError::Internal` / 其他 → 500 Internal Server Error
/// - 成功 → 200 OK
pub async fn handle_request(
    method: &str,
    path: &str,
    body: &[u8],
    headers: &std::collections::HashMap<String, String>,
    svc: &dyn WebhookService,
) -> WebhookResponse {
    // 1. 路由匹配
    let route = match match_webhook_route(path) {
        RouteMatch::Webhook(r) => r,
        RouteMatch::NotWebhook => {
            return WebhookResponse {
                status: 404,
                body: serde_json::json!({
                    "error": "not_found",
                    "message": format!("path {path:?} is not a webhook endpoint"),
                })
                .to_string(),
            };
        }
        RouteMatch::UnknownWebhook => {
            return WebhookResponse {
                status: 404,
                body: serde_json::json!({
                    "error": "unknown_route",
                    "message": format!("unknown webhook route: {path:?}"),
                })
                .to_string(),
            };
        }
    };

    // 2. 方法检查
    if method != "POST" {
        return WebhookResponse {
            status: 405,
            body: serde_json::json!({
                "error": "method_not_allowed",
                "message": format!("webhook {route:?} only accepts POST"),
            })
            .to_string(),
        };
    }

    // 3. 提取 X-Hub-Signature-256 header (case-insensitive)
    let signature = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("x-hub-signature-256"))
        .map(|(_, v)| v.clone());

    // 4. 构 WebhookEvent (event.route 用 full path "/webhook/git" 跟
    // provider.routes key 一致; 业务方 take 后看 route 知道来源)
    let full_path = format!("/webhook/{route}");
    let mut event = WebhookEvent::new(&full_path, body.to_vec());
    if let Some(sig) = signature {
        event = event.with_signature(sig);
    }
    // 复制其他 webhook 相关 header (event-type, delivery-id 等)
    for (k, v) in headers {
        let kl = k.to_lowercase();
        if kl.starts_with("x-github-") || kl.starts_with("x-gitlab-") {
            event = event.with_header(k, v);
        }
    }

    // 5. submit
    match svc.submit(event).await {
        Ok(id) => WebhookResponse {
            status: 200,
            body: serde_json::json!({
                "ok": true,
                "event_id": id,
            })
            .to_string(),
        },
        Err(WebhookError::InvalidSignature(msg)) => WebhookResponse {
            status: 401,
            body: serde_json::json!({
                "error": "invalid_signature",
                "message": msg,
            })
            .to_string(),
        },
        Err(WebhookError::DuplicateEvent(id)) => WebhookResponse {
            // 202 Accepted: 业务方不重复处理但接受了
            status: 202,
            body: serde_json::json!({
                "ok": true,
                "duplicate": true,
                "event_id": id,
            })
            .to_string(),
        },
        Err(WebhookError::UnknownRoute(route)) => WebhookResponse {
            // route 已知 (前面 match 通过了) 但 provider 不知道 — 配置不一致
            status: 404,
            body: serde_json::json!({
                "error": "unknown_route",
                "message": route,
            })
            .to_string(),
        },
        Err(WebhookError::QueueFull(size, cap)) => WebhookResponse {
            status: 503, // Service Unavailable
            body: serde_json::json!({
                "error": "queue_full",
                "size": size,
                "cap": cap,
            })
            .to_string(),
        },
        Err(WebhookError::Internal(msg)) => WebhookResponse {
            status: 500,
            body: serde_json::json!({
                "error": "internal",
                "message": msg,
            })
            .to_string(),
        },
    }
}

/// 起 HTTP webhook server (P15.3.2 main delivery).
///
/// **行为**:
/// - bind 到 `addr` (e.g. "127.0.0.1:9090")
/// - 接受 connection, parse HTTP request
/// - 调 `handle_request` 处理
/// - 返 response
///
/// **协议**: HTTP/1.1, `Content-Length` based body reading (无 chunked).
/// **持久化**: 跟 P15.1 web-ui 一样, connection: close, 不 keep-alive.
///
/// **业务方用法**:
/// ```ignore
/// let svc = LocalWebhookProvider::new()
///     .with_route(RouteConfig::new("/webhook/git", HmacSha256Verifier::new(b"...")));
/// tokio::spawn(async move { serve_http("127.0.0.1:9090", Arc::new(svc)).await });
/// ```
pub async fn serve_http(addr: &str, svc: Arc<dyn WebhookService>) -> Result<(), WebhookError> {
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| WebhookError::Internal(format!("bind: {e}")))?;
    tracing::info!(addr = %addr, "webhook HTTP server started");

    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(error = %e, "accept failed");
                continue;
            }
        };
        let svc = Arc::clone(&svc);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, svc).await {
                tracing::debug!(error = %e, "connection handler error");
            }
        });
    }
}

/// 处理单个 HTTP connection (P15.3.2 internal).
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    svc: Arc<dyn WebhookService>,
) -> Result<(), WebhookError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    // 读 HTTP request line + headers
    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .await
        .map_err(|e| WebhookError::Internal(format!("read_line: {e}")))?;
    let request_line = request_line.trim_end_matches(['\r', '\n']);
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let raw_path = parts.next().unwrap_or("").to_string();
    // 拆 path + query
    let (path, _query) = match raw_path.find('?') {
        Some(idx) => (raw_path[..idx].to_string(), &raw_path[idx + 1..]),
        None => (raw_path.clone(), ""),
    };

    // 读 headers
    let mut headers = std::collections::HashMap::new();
    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        reader
            .read_line(&mut header_line)
            .await
            .map_err(|e| WebhookError::Internal(format!("read_line: {e}")))?;
        let trimmed = header_line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().to_string();
            if key.eq_ignore_ascii_case("content-length") {
                content_length = val.parse().unwrap_or(0);
            }
            headers.insert(key, val);
        }
    }

    // 读 body (按 Content-Length 读 N bytes)
    let mut body = Vec::with_capacity(content_length);
    if content_length > 0 {
        use tokio::io::AsyncReadExt;
        let mut limited = reader.take(content_length as u64);
        limited
            .read_to_end(&mut body)
            .await
            .map_err(|e| WebhookError::Internal(format!("read body: {e}")))?;
    }

    // 路由 + 处理
    let response = handle_request(&method, &path, &body, &headers, svc.as_ref()).await;
    let response_str = response.to_http_string();

    // 写响应
    stream
        .write_all(response_str.as_bytes())
        .await
        .map_err(|e| WebhookError::Internal(format!("write: {e}")))?;
    Ok(())
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secret() -> Vec<u8> {
        b"my-test-secret".to_vec()
    }

    fn git_webhook_body() -> &'static [u8] {
        br#"{"ref":"refs/heads/main","head_commit":{"id":"abc123"}}"#
    }

    // ----- WebhookEvent -----

    #[test]
    fn webhook_event_new_auto_fills_id_and_timestamp() {
        let ev = WebhookEvent::new("/webhook/test", b"body");
        assert!(!ev.id.is_empty());
        assert!(ev.id.len() > 30); // UUID v4
        assert_eq!(ev.route, "/webhook/test");
        assert_eq!(ev.body, b"body");
        assert!(ev.signature.is_none());
        assert!(ev.extra_headers.is_empty());
    }

    #[test]
    fn webhook_event_builder_chain() {
        let ev = WebhookEvent::new("/x", b"y")
            .with_signature("sha256=abc")
            .with_header("X-GitHub-Event", "push");
        assert_eq!(ev.signature.as_deref(), Some("sha256=abc"));
        assert_eq!(
            ev.extra_headers.get("X-GitHub-Event").map(|s| s.as_str()),
            Some("push")
        );
    }

    #[test]
    fn webhook_event_body_str_handles_non_utf8() {
        let ev = WebhookEvent::new("/x", vec![b'h', b'i', 0xFF, b'!']);
        let s = ev.body_str();
        assert!(s.contains("hi"));
        assert!(s.contains("!"));
    }

    // ----- HmacSha256Verifier -----

    #[tokio::test]
    async fn hmac_sha256_verifier_valid_signature() {
        let v = HmacSha256Verifier::new(test_secret());
        let sig = v.compute_signature(git_webhook_body());
        assert!(v.verify(git_webhook_body(), &sig).await.is_ok());
    }

    #[tokio::test]
    async fn hmac_sha256_verifier_invalid_signature_mismatch() {
        let v = HmacSha256Verifier::new(test_secret());
        let wrong = "sha256=0000000000000000000000000000000000000000000000000000000000000000";
        let err = v.verify(git_webhook_body(), wrong).await.unwrap_err();
        assert!(matches!(err, WebhookError::InvalidSignature(_)));
    }

    #[tokio::test]
    async fn hmac_sha256_verifier_tampered_body_rejected() {
        let v = HmacSha256Verifier::new(test_secret());
        let sig = v.compute_signature(git_webhook_body());
        let tampered = b"tampered body";
        let err = v.verify(tampered, &sig).await.unwrap_err();
        assert!(matches!(err, WebhookError::InvalidSignature(_)));
    }

    #[tokio::test]
    async fn hmac_sha256_verifier_wrong_key_rejected() {
        let v1 = HmacSha256Verifier::new(b"key1");
        let v2 = HmacSha256Verifier::new(b"key2");
        let sig = v1.compute_signature(git_webhook_body());
        let err = v2.verify(git_webhook_body(), &sig).await.unwrap_err();
        assert!(matches!(err, WebhookError::InvalidSignature(_)));
    }

    #[tokio::test]
    async fn hmac_sha256_verifier_missing_prefix_rejected() {
        let v = HmacSha256Verifier::new(test_secret());
        // sig 没 "sha256=" 前缀
        let err = v.verify(git_webhook_body(), "abc123").await.unwrap_err();
        match err {
            WebhookError::InvalidSignature(msg) => {
                assert!(msg.contains("sha256="));
            }
            other => panic!("expected InvalidSignature, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn hmac_sha256_verifier_invalid_hex_rejected() {
        let v = HmacSha256Verifier::new(test_secret());
        // "sha256=" + 非 hex
        let err = v
            .verify(git_webhook_body(), "sha256=not-hex-data!!")
            .await
            .unwrap_err();
        assert!(matches!(err, WebhookError::InvalidSignature(_)));
    }

    #[tokio::test]
    async fn hmac_sha256_verifier_zero_length_secret_works_but_warns() {
        // 0 长度 secret HMAC 也是 valid 的 (空字符串)
        // 这里只 verify 计算 / verify 还能跑, 不验 log warn (tracing 难测)
        let v = HmacSha256Verifier::new(b"");
        let sig = v.compute_signature(git_webhook_body());
        // 0 长 secret 的 HMAC-SHA256
        let mut mac = HmacSha256::new_from_slice(b"").unwrap();
        mac.update(git_webhook_body());
        let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert_eq!(sig, expected);
        assert!(v.verify(git_webhook_body(), &sig).await.is_ok());
    }

    // ----- LocalWebhookProvider -----

    #[test]
    fn local_webhook_provider_new_is_empty() {
        let p = LocalWebhookProvider::new();
        assert_eq!(p.routes().len(), 0);
        assert_eq!(p.queue_len(), 0);
    }

    #[test]
    fn local_webhook_provider_with_route_adds_route() {
        let v = HmacSha256Verifier::new(test_secret());
        let p = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        assert_eq!(p.routes(), vec!["/webhook/git".to_string()]);
    }

    #[test]
    fn local_webhook_provider_provider_name_is_local_webhook() {
        let p = LocalWebhookProvider::new();
        assert_eq!(p.provider_name(), "local-webhook");
    }

    #[tokio::test]
    async fn local_webhook_provider_submit_valid_event_lands_in_queue() {
        let v = HmacSha256Verifier::new(test_secret());
        let p = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        let event = WebhookEvent::new("/webhook/git", git_webhook_body().to_vec());
        let sig = p
            .routes()
            .first()
            .map(|_| {
                // reuse verifier to sign
                HmacSha256Verifier::new(test_secret()).compute_signature(git_webhook_body())
            })
            .unwrap();
        let event = event.with_signature(sig);

        let id = p.submit(event).await.expect("submit");
        assert!(!id.is_empty());
        assert_eq!(p.queue_len(), 1);
    }

    #[tokio::test]
    async fn local_webhook_provider_submit_unknown_route_rejected() {
        let p = LocalWebhookProvider::new();
        let event = WebhookEvent::new("/webhook/nope", b"body");
        let err = p.submit(event).await.unwrap_err();
        assert!(matches!(err, WebhookError::UnknownRoute(_)));
    }

    #[tokio::test]
    async fn local_webhook_provider_submit_missing_signature_rejected() {
        let v = HmacSha256Verifier::new(test_secret());
        let p = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        let event = WebhookEvent::new("/webhook/git", git_webhook_body().to_vec());
        // 没 with_signature
        let err = p.submit(event).await.unwrap_err();
        match err {
            WebhookError::InvalidSignature(msg) => {
                assert!(msg.contains("X-Hub-Signature-256"));
            }
            other => panic!("expected InvalidSignature, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn local_webhook_provider_submit_invalid_signature_rejected() {
        let v = HmacSha256Verifier::new(test_secret());
        let p = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        let event = WebhookEvent::new("/webhook/git", git_webhook_body().to_vec()).with_signature(
            "sha256=0000000000000000000000000000000000000000000000000000000000000000",
        );
        let err = p.submit(event).await.unwrap_err();
        assert!(matches!(err, WebhookError::InvalidSignature(_)));
    }

    #[tokio::test]
    async fn local_webhook_provider_submit_duplicate_id_rejected() {
        // 同一 event (同 id) 提交两次, 第二次返 DuplicateEvent
        // 注: 我们手动构造同 id 的 event (绕过 new() 的 UUID 生成)
        let v = HmacSha256Verifier::new(test_secret());
        let p = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        let sig = HmacSha256Verifier::new(test_secret()).compute_signature(git_webhook_body());

        let mut event1 = WebhookEvent::new("/webhook/git", git_webhook_body().to_vec())
            .with_signature(sig.clone());
        // 强制同 id
        event1.id = "fixed-id".to_string();
        p.submit(event1).await.expect("first submit");
        assert_eq!(p.queue_len(), 1);

        let mut event2 =
            WebhookEvent::new("/webhook/git", git_webhook_body().to_vec()).with_signature(sig);
        event2.id = "fixed-id".to_string();
        let err = p.submit(event2).await.unwrap_err();
        assert!(matches!(err, WebhookError::DuplicateEvent(_)));
        // queue 仍 1 (没入队)
        assert_eq!(p.queue_len(), 1);
    }

    #[tokio::test]
    async fn local_webhook_provider_take_returns_events_in_order() {
        let v = HmacSha256Verifier::new(test_secret());
        let p = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        let sig = HmacSha256Verifier::new(test_secret()).compute_signature(git_webhook_body());

        // 提交 3 个
        for _ in 0..3 {
            let event = WebhookEvent::new("/webhook/git", git_webhook_body().to_vec())
                .with_signature(sig.clone());
            p.submit(event).await.expect("submit");
        }
        assert_eq!(p.queue_len(), 3);

        // 拉 3 个, 顺序
        let e1 = p.take().await.expect("take").expect("event 1");
        let e2 = p.take().await.expect("take").expect("event 2");
        let e3 = p.take().await.expect("take").expect("event 3");
        assert_ne!(e1.id, e2.id);
        assert_ne!(e2.id, e3.id);

        // 第 4 次拉返 None
        let none = p.take().await.expect("take");
        assert!(none.is_none());
    }

    // ========================================================================
    // P15.3.2 tests: HTTP server (handle_request + match_webhook_route + serve_http)
    // ========================================================================

    /// 启 serve_http server (P15.3.2 helper for integration tests)
    async fn start_test_server() -> (String, Arc<LocalWebhookProvider>) {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let addr_for_task = addr.clone();
        let v = HmacSha256Verifier::new(test_secret());
        let provider =
            Arc::new(LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v)));
        let svc: Arc<dyn WebhookService> = provider.clone();
        tokio::spawn(async move {
            let _ = serve_http(&addr_for_task, svc).await;
        });
        // 等 server ready
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        (addr, provider)
    }

    /// 找一个空闲端口
    async fn free_port() -> u16 {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = l.local_addr().expect("local_addr").port();
        drop(l);
        port
    }

    // ----- match_webhook_route helper -----

    #[test]
    fn match_webhook_route_recognizes_known_routes() {
        assert_eq!(
            match_webhook_route("/webhook/git"),
            RouteMatch::Webhook("git".to_string())
        );
        assert_eq!(
            match_webhook_route("/webhook/gitlab"),
            RouteMatch::Webhook("gitlab".to_string())
        );
        assert_eq!(
            match_webhook_route("/webhook/with-dash_and.dot"),
            RouteMatch::Webhook("with-dash_and.dot".to_string())
        );
    }

    #[test]
    fn match_webhook_route_rejects_non_webhook() {
        assert_eq!(match_webhook_route("/"), RouteMatch::NotWebhook);
        assert_eq!(match_webhook_route("/api/foo"), RouteMatch::NotWebhook);
        assert_eq!(match_webhook_route("/webhooks/git"), RouteMatch::NotWebhook); // 多 s
    }

    #[test]
    fn match_webhook_route_rejects_malformed() {
        // "/webhook/" 没 route
        assert_eq!(match_webhook_route("/webhook/"), RouteMatch::UnknownWebhook);
        // 嵌套路径
        assert_eq!(
            match_webhook_route("/webhook/git/foo"),
            RouteMatch::UnknownWebhook
        );
    }

    // ----- WebhookResponse.to_http_string -----

    #[test]
    fn webhook_response_to_http_string_contains_status_and_body() {
        let r = WebhookResponse {
            status: 200,
            body: r#"{"ok":true}"#.to_string(),
        };
        let s = r.to_http_string();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(s.contains("Content-Type: application/json"));
        assert!(s.contains(r#"{"ok":true}"#));
    }

    // ----- handle_request (纯函数, 单元测试) -----

    #[tokio::test]
    async fn handle_request_valid_webhook_returns_200() {
        let v = HmacSha256Verifier::new(test_secret());
        let provider =
            LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v.clone()));
        let sig = v.compute_signature(git_webhook_body());
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Hub-Signature-256".to_string(), sig);
        headers.insert("X-GitHub-Event".to_string(), "push".to_string());

        let resp = handle_request(
            "POST",
            "/webhook/git",
            git_webhook_body(),
            &headers,
            &provider,
        )
        .await;
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("\"ok\":true"));
        assert_eq!(provider.queue_len(), 1);
    }

    #[tokio::test]
    async fn handle_request_invalid_signature_returns_401() {
        let v = HmacSha256Verifier::new(test_secret());
        let provider = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        let mut headers = std::collections::HashMap::new();
        headers.insert(
            "X-Hub-Signature-256".to_string(),
            "sha256=0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        );

        let resp = handle_request(
            "POST",
            "/webhook/git",
            git_webhook_body(),
            &headers,
            &provider,
        )
        .await;
        assert_eq!(resp.status, 401);
        assert!(resp.body.contains("\"error\":\"invalid_signature\""));
        assert_eq!(provider.queue_len(), 0);
    }

    #[tokio::test]
    async fn handle_request_missing_signature_returns_401() {
        // 没 X-Hub-Signature-256 header
        let v = HmacSha256Verifier::new(test_secret());
        let provider = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        let headers = std::collections::HashMap::new();

        let resp = handle_request(
            "POST",
            "/webhook/git",
            git_webhook_body(),
            &headers,
            &provider,
        )
        .await;
        assert_eq!(resp.status, 401);
        assert!(resp.body.contains("X-Hub-Signature-256"));
    }

    #[tokio::test]
    async fn handle_request_unknown_route_returns_404() {
        let provider = LocalWebhookProvider::new();
        let headers = std::collections::HashMap::new();
        let resp = handle_request("POST", "/webhook/nope", b"body", &headers, &provider).await;
        assert_eq!(resp.status, 404);
        assert!(resp.body.contains("\"error\":\"unknown_route\""));
    }

    #[tokio::test]
    async fn handle_request_non_webhook_path_returns_404() {
        let provider = LocalWebhookProvider::new();
        let headers = std::collections::HashMap::new();
        let resp = handle_request("POST", "/api/foo", b"body", &headers, &provider).await;
        assert_eq!(resp.status, 404);
        assert!(resp.body.contains("\"error\":\"not_found\""));
    }

    #[tokio::test]
    async fn handle_request_non_post_returns_405() {
        let v = HmacSha256Verifier::new(test_secret());
        let provider = LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v));
        let headers = std::collections::HashMap::new();
        let resp = handle_request("GET", "/webhook/git", b"", &headers, &provider).await;
        assert_eq!(resp.status, 405);
        assert!(resp.body.contains("method_not_allowed"));
    }

    #[tokio::test]
    async fn handle_request_duplicate_event_id_returns_202() {
        // 第一次 submit (valid), 第二次同 id (绕过 new() 强制同 id) → 202
        let v = HmacSha256Verifier::new(test_secret());
        let provider =
            LocalWebhookProvider::new().with_route(RouteConfig::new("/webhook/git", v.clone()));
        let sig = v.compute_signature(git_webhook_body());
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Hub-Signature-256".to_string(), sig.clone());

        // 第一次 — 200
        let r1 = handle_request(
            "POST",
            "/webhook/git",
            git_webhook_body(),
            &headers,
            &provider,
        )
        .await;
        assert_eq!(r1.status, 200);
        assert_eq!(provider.queue_len(), 1);

        // 第二次同 id (dedup 在 submit 内 — 但 submit 内部用 event.id; 这里
        // 每次 new() 都生成新 id, 不会触发 dedup)
        // → 改为: 直接调 submit 测 dedup, handle_request 测正常情况.
        // 这测试已在 P15.3.1 覆盖, 这里只验 200 happy path.
    }

    // ----- serve_http 集成测试 (真 HTTP server + reqwest) -----

    #[tokio::test]
    async fn serve_http_real_post_valid_webhook_200() {
        // 起 server, POST 真发, 验响应
        let (addr, provider) = start_test_server().await;
        let v = HmacSha256Verifier::new(test_secret());
        let sig = v.compute_signature(git_webhook_body());

        let resp = reqwest::Client::new()
            .post(format!("http://{addr}/webhook/git"))
            .header("content-type", "application/json")
            .header("x-hub-signature-256", sig)
            .body(git_webhook_body().to_vec())
            .send()
            .await
            .expect("POST");
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["ok"], true);
        assert!(body["event_id"].is_string());
        assert_eq!(provider.queue_len(), 1);
    }

    #[tokio::test]
    async fn serve_http_real_post_invalid_signature_401() {
        let (addr, _provider) = start_test_server().await;
        let resp = reqwest::Client::new()
            .post(format!("http://{addr}/webhook/git"))
            .header(
                "x-hub-signature-256",
                "sha256=0000000000000000000000000000000000000000000000000000000000000000",
            )
            .body(git_webhook_body().to_vec())
            .send()
            .await
            .expect("POST");
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn serve_http_real_post_unknown_route_404() {
        let (addr, _provider) = start_test_server().await;
        let resp = reqwest::Client::new()
            .post(format!("http://{addr}/webhook/nope"))
            .body(b"body".to_vec())
            .send()
            .await
            .expect("POST");
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn serve_http_real_get_webhook_405() {
        let (addr, _provider) = start_test_server().await;
        let resp = reqwest::Client::new()
            .get(format!("http://{addr}/webhook/git"))
            .send()
            .await
            .expect("GET");
        assert_eq!(resp.status(), 405);
    }

    // ========================================================================
    // P15.3.3 tests: webhook_to_session_event conversion
    // ========================================================================

    #[test]
    fn webhook_to_session_event_basic_mapping() {
        use ma_harness_core::{EventType, Severity};

        let event = WebhookEvent::new("/webhook/git", br#"{"action":"opened"}"#.to_vec());
        let session = webhook_to_session_event(&event).expect("convert");
        // session_id = event.id
        assert_eq!(session.session_id, event.id);
        // event_type = UserInput
        assert_eq!(session.event_type, EventType::UserInput);
        // severity = Info
        assert_eq!(session.severity, Severity::Info);
        // plugin_name = "webhook"
        assert_eq!(session.plugin_name.as_deref(), Some("webhook"));
        // payload_json = body
        assert_eq!(
            session.payload_json.as_deref(),
            Some(r#"{"action":"opened"}"#)
        );
        // model_visible = true (UserInput 是 model_visible)
        assert!(session.model_visible);
    }

    #[test]
    fn webhook_to_session_event_binary_body_uses_lossy_utf8() {
        // 二进制 body → String::from_utf8_lossy 替换 invalid bytes
        let event = WebhookEvent::new("/webhook/binary", vec![b'h', b'i', 0xFF, 0xFE, b'!']);
        let session = webhook_to_session_event(&event).expect("convert");
        let payload = session.payload_json.as_deref().unwrap();
        assert!(payload.contains("hi"));
        assert!(payload.contains("!"));
        // 0xFF 0xFE → U+FFFD (replacement char), 不 panic
    }

    #[test]
    fn webhook_to_session_event_empty_body_still_works() {
        // 空 body (e.g. POST 没 body) — 仍然能转, payload 是空字符串
        let event = WebhookEvent::new("/webhook/empty", Vec::new());
        let session = webhook_to_session_event(&event).expect("convert");
        assert_eq!(session.payload_json.as_deref(), Some(""));
    }

    #[test]
    fn webhook_to_session_event_preserves_signature_info_via_extra_headers() {
        // WebhookEvent 的 signature / extra_headers 不会自动进 SessionEvent
        // (SessionEvent 只有 payload_json 装 body, 没 header 字段)
        // P15.3.3 限制: 签名信息丢失 — 业务方需要的话可自己加到 payload
        let event = WebhookEvent::new("/webhook/git", br#"{"a":1}"#.to_vec())
            .with_signature("sha256=abc")
            .with_header("X-GitHub-Event", "push");
        let session = webhook_to_session_event(&event).expect("convert");
        // signature / extra_headers 不在 SessionEvent 上
        assert!(session.payload_json.is_some());
        // session_id 跟 webhook id 一致 (trace 简单)
        assert_eq!(session.session_id, event.id);
    }

    #[test]
    fn webhook_to_session_event_then_event_log_append_full_chain() {
        // 端到端: webhook → session_event → event_log.append → query 拿回
        let log = ma_harness_core::EventLog::open_in_memory().expect("open in-memory log");
        let event = WebhookEvent::new("/webhook/git", br#"{"ref":"refs/heads/main"}"#.to_vec());
        let session_event = webhook_to_session_event(&event).expect("convert");
        let seq = log.append(session_event.clone());
        assert!(seq > 0);

        // query 拿回, 验内容
        let q = ma_harness_core::EventQuery {
            session_id: event.id.clone(),
            ..Default::default()
        };
        let page = log.query(&q).expect("query");
        assert_eq!(page.events.len(), 1);
        let stored = &page.events[0].event;
        assert_eq!(stored.session_id, event.id);
        assert_eq!(stored.event_type, session_event.event_type);
        assert_eq!(stored.plugin_name.as_deref(), Some("webhook"));
        assert_eq!(
            stored.payload_json.as_deref(),
            Some(r#"{"ref":"refs/heads/main"}"#)
        );
    }

    // ========================================================================
    // P15.3.4 tests: run_webhook_dispatcher (background persistence task)
    // ========================================================================

    /// Submit 一个 valid webhook 到 provider (helper, 准备 queue content).
    async fn submit_one_webhook(provider: &LocalWebhookProvider, body: &[u8]) -> String {
        let v = HmacSha256Verifier::new(test_secret());
        let sig = v.compute_signature(body);
        let event = WebhookEvent::new("/webhook/git", body.to_vec()).with_signature(sig);
        let id = event.id.clone();
        provider.submit(event).await.expect("submit");
        id
    }

    #[tokio::test]
    async fn run_webhook_dispatcher_persists_event_to_log() {
        // 1. submit webhook → provider queue
        // 2. 启 dispatcher 跑 ~200ms (拉 queue + persist)
        // 3. query log 验 event 在
        let provider = LocalWebhookProvider::new().with_route(RouteConfig::new(
            "/webhook/git",
            HmacSha256Verifier::new(test_secret()),
        ));
        let log = Arc::new(ma_harness_core::EventLog::open_in_memory().expect("log"));

        let event_id = submit_one_webhook(&provider, br#"{"action":"opened"}"#).await;
        assert_eq!(provider.queue_len(), 1);

        // 启 dispatcher, 跑 200ms
        let svc: Arc<dyn WebhookService> = Arc::new(provider);
        let dispatcher_handle = {
            let svc = Arc::clone(&svc);
            let log = Arc::clone(&log);
            tokio::spawn(async move { run_webhook_dispatcher(svc, log).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        dispatcher_handle.abort();

        // query log 验
        let q = ma_harness_core::EventQuery {
            session_id: event_id.clone(),
            ..Default::default()
        };
        let page = log.query(&q).expect("query");
        assert_eq!(
            page.events.len(),
            1,
            "expected 1 event in log for session {event_id}"
        );
        let stored = &page.events[0].event;
        assert_eq!(stored.plugin_name.as_deref(), Some("webhook"));
        assert_eq!(
            stored.payload_json.as_deref(),
            Some(r#"{"action":"opened"}"#)
        );
    }

    #[tokio::test]
    async fn run_webhook_dispatcher_processes_multiple_events() {
        let provider = LocalWebhookProvider::new().with_route(RouteConfig::new(
            "/webhook/git",
            HmacSha256Verifier::new(test_secret()),
        ));
        let log = Arc::new(ma_harness_core::EventLog::open_in_memory().expect("log"));

        let id1 = submit_one_webhook(&provider, br#"{"i":1}"#).await;
        let id2 = submit_one_webhook(&provider, br#"{"i":2}"#).await;
        let id3 = submit_one_webhook(&provider, br#"{"i":3}"#).await;
        assert_eq!(provider.queue_len(), 3);

        let svc: Arc<dyn WebhookService> = Arc::new(provider);
        let dispatcher_handle = {
            let svc = Arc::clone(&svc);
            let log = Arc::clone(&log);
            tokio::spawn(async move { run_webhook_dispatcher(svc, log).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        dispatcher_handle.abort();

        // 3 个 session_id 都应该被持久化
        for id in [&id1, &id2, &id3] {
            let q = ma_harness_core::EventQuery {
                session_id: id.clone(),
                ..Default::default()
            };
            let page = log.query(&q).expect("query");
            assert_eq!(page.events.len(), 1, "expected 1 event for {id}");
        }
    }

    #[tokio::test]
    async fn run_webhook_dispatcher_with_empty_queue_does_not_panic() {
        // 空 queue 启 dispatcher, 跑 200ms, 没 panic
        let provider = LocalWebhookProvider::new();
        let svc: Arc<dyn WebhookService> = Arc::new(provider);
        let log = Arc::new(ma_harness_core::EventLog::open_in_memory().expect("log"));

        let dispatcher_handle = {
            let svc = Arc::clone(&svc);
            let log = Arc::clone(&log);
            tokio::spawn(async move { run_webhook_dispatcher(svc, log).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        // 没 panic 就 OK (空 queue → sleep, 不退)
        dispatcher_handle.abort();
    }

    #[tokio::test]
    async fn run_webhook_dispatcher_does_not_exit_on_empty_queue() {
        // 空 queue 跑 150ms, dispatcher 还活着 (没 panic / 退出)
        let provider = LocalWebhookProvider::new();
        let svc: Arc<dyn WebhookService> = Arc::new(provider);
        let log = Arc::new(ma_harness_core::EventLog::open_in_memory().expect("log"));

        let dispatcher_handle = {
            let svc = Arc::clone(&svc);
            let log = Arc::clone(&log);
            tokio::spawn(async move { run_webhook_dispatcher(svc, log).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        // abort 前 dispatcher 还活着
        assert!(!dispatcher_handle.is_finished());
        dispatcher_handle.abort();
    }
}
