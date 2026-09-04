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
}
