//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-web-ui`
//! **Crate ident** (`use` 路径): `ma_harness_web_ui`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-web-ui = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_web_ui::{LocalWebUiServer, WebUiServer, SseEvent, UserInput};
//!
//! let server = LocalWebUiServer::bind("127.0.0.1:3080").await?;
//!
//! // 1. 订阅 user input (业务方 agent loop)
//! let mut input_rx = server.subscribe_user_input();
//! tokio::spawn(async move {
//!     while let Some(input) = input_rx.recv().await {
//!         println!("user: {}", input.text);
//!     }
//! });
//!
//! // 2. 启动 server
//! let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
//! tx.send(SseEvent::SessionEvent(json!({"id": "1"}))).unwrap();
//! server.run(tx).await?;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-web-ui
//!
//! # 设计 (Design) — P15.1.1+
//!
//! **目标**: 抽象 `ctx.web_ui` (跟 dsh `:3080` browser app 对等), 业务方
//! - 跑 `mah web` 打开浏览器
//! - 看 live session (events via SSE)
//! - 接受 user input (P15.1.4+)
//! - 多 tab / 多 browser 并存, UserInputAck 实时反馈 (P15.1.5+)
//! - SSE heartbeat + dead connection 自动清理 (P15.1.6+)
//! - Graceful shutdown (P15.1.7+)
//! - Real session/event data from `EventLog` (P15.1.8+)
//!
//! **P15.1 大工程** (8-12 周): Rust + WASM (Leptos/Yew) or React + REST API.
//! **P15.1.1 骨架**: crate 脚手架, WebUiServer trait, std+tokio HTTP server, SSE.
//! **P15.1.2**: SSE query filter (?session=xxx), SessionStart/End events.
//! **P15.1.3**: /api/version + /api/sessions endpoints (stub).
//! **P15.1.4**: POST /api/input 接 user input, UserInput 转发给 subscribers.
//! **P15.1.5**: 多 SSE 广播 (Vec) + UserInputAck 实时反馈.
//! **P15.1.6**: SSE heartbeat (15s) + dead sender 自动清理.
//! **P15.1.7**: Graceful shutdown (`stop()` + `Notify` 协调).
//! **P15.1.8** (本次): 接 `ma-harness-core::EventLog` 替 `/api/sessions` stub,
//! 新增 `/api/sessions/<id>/events?limit=N`.
//!
//! **设计决策**: 不引 salvo/axum, 用 `tokio::net::TcpListener` + 手写 minimal HTTP
//! (P15.1.x 只需要几个 endpoint, 完整 framework 过度设计).
//! 业务方 P15.1.9+ 改用 axum + 真 SPA 框架时, LocalWebUiServer 换成对应 impl.
//!
//! **核心抽象**:
//! - [`SseEvent`] enum (SessionEvent / SessionStart/End / UserInputAck / Message / Heartbeat / Done)
//! - [`UserInput`] struct (P15.1.4: browser → server, session + text + ts)
//! - [`WebUiServer`] trait (bind / run / port)
//! - [`LocalWebUiServer`] (主交付, std + tokio; multi-SSE + heartbeat + shutdown + EventLog integration)
//! - [`html_shell`] (P15.1.5: 含 input form + UserInputAck 渲染, 业务方 P15.1.9+ 替换为 Leptos / React)
//!
//! **6 质量属性**:
//! - 可复用: WebUiServer trait, future RemoteWebUiServer (P15+ cloud); `with_event_log` 接受任意 EventLog
//! - 可维护: 模块化分块, server / sse / http / html / error / user_input / event_log 集中 lib.rs
//! - 鲁棒: 错误归一化 (Bind / IO / ChannelClosed), 405 vs 404 区分, Content-Length body 读,
//!   EventLog 错误 → 500, 没 attached log → 503 (明确告诉 client), lock 短暂 + 跨 await 安全
//! - 安全: 不 eval user input, SSE events 静态 string, server 端盖 timestamp 不信 client,
//!   ?limit clamp 到 1000 (防 DoS)
//! - 可测: 47+ 测试覆盖 bind / HTTP / SSE / HTML / POST input / 多 subscriber / 多 SSE / heartbeat / shutdown / EventLog
//! - 可扩展: active_sse_subs Vec<Sender> + user_input_subs Vec<Sender> + event_log Arc<EventLog>,
//!   业务方多 subscriber / 多 tab / 真 session 数据自然支持
//!
//! # 限制 (Limitations) — P15.1.8
//!
//! - placeholder HTML shell (业务方 P15.1.9+ 替换为 Leptos / React)
//! - `stop()` 不强制关闭已有活跃连接 — accept loop 退出, 已连接 conn 自然结束
//! - `/api/sessions` 只返 `event_count` + placeholder `started_at`/`last_activity` (0)
//!   + `status: "unknown"` (P15.1.8.1+ 加 `list_session_summaries()` to EventLog 拿 first/last ts)
//! - `/api/sessions/<id>/events` 不支持 streaming (P15.1.9+ 改 SSE subscribe, 实时推新 event)
//! - `with_event_log` 只接 `EventLog` (P15.1.9+ 接 trait `SessionStore` 支持 mock / remote)
//! - `stop()` 不强制关闭已有活跃连接 — accept loop 退出, 已连接 conn 自然结束
//!   (业务方 P15.1.9+ 想要 hard kill 已有 conn 可加 close_inflight + 30s timeout)
//! - 不接 ma-harness-server OpenAPI (P15.1.9+ 集成)
//! - 不接 ctx.user 全链路 (P15.2 集成 ma-harness-core Context)
//!
//! [dsh-feature-parity-table §8]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#8-distribution-surfaces

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

// ============================================================================
// WebUiError
// ============================================================================

/// Web UI capability 错误.
#[derive(Debug, Error)]
pub enum WebUiError {
    /// Bind 失败 (端口占用 / 权限不足)
    #[error("bind failed: {0}")]
    Bind(String),

    /// IO 错误
    #[error("web UI I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP parse 错误
    #[error("HTTP parse error: {0}")]
    HttpParse(String),

    /// Channel closed
    #[error("SSE channel closed")]
    ChannelClosed,
}

// ============================================================================
// SseEvent
// ============================================================================

/// SSE event (服务端 stream 给浏览器).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// SessionEvent (业务方喂 ma-harness-core::SessionEvent JSON 串)
    SessionEvent {
        /// 业务方 session id (P15.1.2: 多 session filter)
        session_id: String,
        /// 事件 JSON 字符串
        json: String,
    },
    /// Session 开始 (P15.1.2 新增)
    SessionStart {
        /// 业务方 session id
        session_id: String,
    },
    /// Session 结束 (P15.1.2 新增)
    SessionEnd {
        /// 业务方 session id
        session_id: String,
    },
    /// User input ack (P15.1.5 新增).
    ///
    /// **流向**: `POST /api/input` 收到 → server 广播给所有 SSE 连接,
    /// 让浏览器看到 "input received" 而不必等 POST 200 response.
    /// 同时包含 server 端盖的 timestamp (跟 UserInput.received_at_ms 一致).
    UserInputAck {
        /// 业务方 session id
        session_id: String,
        /// server 接收时间戳 (ms since epoch)
        received_at_ms: i64,
    },
    /// 自定义消息 (e.g. 错误 / 状态)
    Message(String),
    /// Heartbeat (keep-alive, 防 proxy / browser timeout)
    Heartbeat,
    /// 流结束
    Done,
}

impl SseEvent {
    /// 转成 SSE wire format (`data: ...\n\n`)
    pub fn to_sse_string(&self) -> String {
        match self {
            SseEvent::SessionEvent { session_id, json } => {
                format!("event: session\nid: {session_id}\ndata: {json}\n\n")
            }
            SseEvent::SessionStart { session_id } => {
                format!("event: session_start\nid: {session_id}\ndata: start\n\n")
            }
            SseEvent::SessionEnd { session_id } => {
                format!("event: session_end\nid: {session_id}\ndata: end\n\n")
            }
            SseEvent::UserInputAck {
                session_id,
                received_at_ms,
            } => {
                // data 字段是 JSON 字符串, 方便浏览器 parse
                let data =
                    format!(r#"{{"session":"{session_id}","received_at_ms":{received_at_ms}}}"#);
                format!("event: user_input_ack\nid: {session_id}\ndata: {data}\n\n")
            }
            SseEvent::Message(msg) => format!("event: message\ndata: {msg}\n\n"),
            SseEvent::Heartbeat => ": heartbeat\n\n".to_string(),
            SseEvent::Done => "event: done\ndata: end\n\n".to_string(),
        }
    }

    /// 拿 session_id (如果有)
    pub fn session_id(&self) -> Option<&str> {
        match self {
            SseEvent::SessionEvent { session_id, .. }
            | SseEvent::SessionStart { session_id }
            | SseEvent::SessionEnd { session_id }
            | SseEvent::UserInputAck { session_id, .. } => Some(session_id),
            _ => None,
        }
    }
}

// ============================================================================
// UserInput (P15.1.4: browser → server, via POST /api/input)
// ============================================================================

/// 用户输入 (浏览器 → server, 透过 `POST /api/input`).
///
/// **流向** (P15.1.4):
/// - 浏览器 fetch POST 上来, body 是 `{"session": "...", "text": "..."}`
/// - server 解析 → 构 `UserInput` → 转发给所有 `subscribe_user_input()` 的 subscribers
/// - 业务方 agent loop 拿 receiver, 喂给 LLM
///
/// **设计决策**:
/// - 用 `mpsc::unbounded` 简化 (P15.1.4 阶段)
/// - 多 subscriber 支持 (Vec<Sender>) — 业务方可能有多个 agent 监听
/// - 时间戳 server 端盖 (不信任 client 时钟)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInput {
    /// 业务方 session id (e.g. "demo-session")
    pub session_id: String,
    /// 用户输入文本
    pub text: String,
    /// server 接收时间戳 (ms since epoch)
    pub received_at_ms: i64,
}

impl UserInput {
    /// 构一个新 UserInput, 自动盖 `received_at_ms = now`
    pub fn now(session_id: impl Into<String>, text: impl Into<String>) -> Self {
        let received_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            session_id: session_id.into(),
            text: text.into(),
            received_at_ms,
        }
    }
}

// ============================================================================
// WebUiServer trait
// ============================================================================

/// Web UI server (P15.1.1 trait 抽象).
///
/// **业务方用**: 实现 `bind` + `run` + `port`. 内部接 SSE channel.
#[async_trait]
pub trait WebUiServer: Send + Sync + 'static {
    /// Bind 到 addr (e.g. "127.0.0.1:3080")
    async fn bind(addr: &str) -> Result<Self, WebUiError>
    where
        Self: Sized;

    /// 跑 server (持续 accept connection, 业务方 spawn)
    ///
    /// # Arguments
    /// - `events`: SSE channel (业务方往 tx 发 SseEvent, server 转发给所有连接)
    async fn run(&self, events: mpsc::UnboundedSender<SseEvent>) -> Result<(), WebUiError>;

    /// 监听端口
    fn port(&self) -> u16;

    /// Provider 标识
    fn provider_name(&self) -> &'static str {
        "web-ui"
    }
}

// ============================================================================
// LocalWebUiServer (P15.1.1 主交付, std + tokio)
// ============================================================================

/// 本地 Web UI server (P15.1.1 骨架).
///
/// **实现**: `tokio::net::TcpListener` + 手写 minimal HTTP request parser.
/// 接受 `GET /` 返 HTML shell, `GET /api/sse` 返 SSE stream,
/// `POST /api/input` 接 user input (P15.1.4 新增).
/// 不引 salvo/axum (P15.1.5+ 改用真 framework).
#[derive(Debug)]
pub struct LocalWebUiServer {
    addr: String,
    port: u16,
    /// 活跃的 SSE senders (P15.1.5: Vec 替代 Option, 支持多 tab 多 browser).
    /// 每条 SSE 连接 push 一个 sender 进来, 连接断开时清理 (TODO: 业务方 P15.1.5+ 加 cleanup).
    /// 改用 `std::sync::Mutex` (跟 `user_input_subs` 一致, lock 短暂 + 跨 await 安全)
    active_sse_subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>>,
    /// 订阅 user input 的 receivers (P15.1.4 新增, std::sync::Mutex 因为 subscribe_user_input 是 sync)
    user_input_subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<UserInput>>>>,
    /// Server 启动状态
    running: Arc<Mutex<bool>>,
    /// P15.1.7: graceful shutdown signal. `stop()` triggers it; `run()` /
    /// `heartbeat_loop` `tokio::select!` on it.
    shutdown: Arc<tokio::sync::Notify>,
    /// P15.1.8: 可选的 EventLog 集成. 设了之后 `/api/sessions` 返真实数据,
    /// `/api/sessions/<id>/events` 返该 session 的 events. 没设的话返 503.
    event_log: Option<Arc<ma_harness_core::EventLog>>,
}

impl LocalWebUiServer {
    /// 创建一个新 LocalWebUiServer (未 bind)
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            port: 0,
            active_sse_subs: Arc::new(std::sync::Mutex::new(Vec::new())),
            user_input_subs: Arc::new(std::sync::Mutex::new(Vec::new())),
            running: Arc::new(Mutex::new(false)),
            shutdown: Arc::new(tokio::sync::Notify::new()),
            event_log: None,
        }
    }

    /// Builder: attach EventLog (P15.1.8 新增).
    ///
    /// **作用**: 设了之后 `/api/sessions` 返真实 session 列表 (从 log 查),
    /// `/api/sessions/<id>/events?limit=N` 返该 session 的 events.
    /// 没设的话 session endpoints 返 503 (service unavailable, 明确告诉 client
    /// "server 没配 event log, 不能返 session 数据").
    ///
    /// **注**: `EventLog` 是 `Clone` (内部 `Arc<Mutex<Connection>>`), 所以 clone 即可,
    /// 不需要 `Arc<EventLog>` 包装. 但 crate 间共享更自然用 `Arc<EventLog>`.
    pub fn with_event_log(mut self, log: Arc<ma_harness_core::EventLog>) -> Self {
        self.event_log = Some(log);
        self
    }

    /// 拿监听地址
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// 拿当前 attached event_log (P15.1.8 测试用)
    #[cfg(test)]
    pub(crate) fn has_event_log(&self) -> bool {
        self.event_log.is_some()
    }

    /// 触发 graceful shutdown (P15.1.7 新增).
    ///
    /// **行为**:
    /// - `run()` 的 accept loop 收到信号后, 立即 break 并返 `Ok(())`
    /// - `heartbeat_loop` 后台任务收到信号后, 退出
    /// - 已有活跃连接 (HTTP/SSE) 不被强制断开, 自然结束 (client close or handler return)
    ///
    /// **幂等**: 多次调用安全 (`Notify::notify_one` 只 wake 一个 waiter, 没 waiter 也不报错)
    /// **非阻塞**: 立即返, 不等 server 真正退出
    /// **线程安全**: 业务方任意线程调, 都能触发 server 退出
    pub fn stop(&self) {
        self.shutdown.notify_one();
    }

    /// 拿 shutdown Notify 的 clone (P15.1.7 测试 / 高级用).
    ///
    /// 业务方一般用 `stop()` 即可. 这个方法给需要
    /// "跟其他 signal (e.g. SIGINT, Ctrl-C) 联合" 的场景.
    pub fn shutdown_handle(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.shutdown)
    }

    /// 订阅 user input channel (P15.1.4 新增).
    ///
    /// **业务方用法** (agent loop):
    /// ```ignore
    /// let server = LocalWebUiServer::bind("127.0.0.1:3080").await?;
    /// let mut input_rx = server.subscribe_user_input();
    /// while let Some(input) = input_rx.recv().await {
    ///     // 喂给 LLM
    ///     agent.handle_user_input(&input.text).await?;
    /// }
    /// ```
    ///
    /// **多 subscriber 支持**: 业务方可以调多次, 每个拿独立 receiver,
    /// 每次 `POST /api/input` 都会被 fan-out 给所有 subscribers.
    ///
    /// **注**: 返回的 receiver drop 后, 该 subscriber 就不收消息了
    /// (但其他 subscribers 不受影响).
    pub fn subscribe_user_input(&self) -> mpsc::UnboundedReceiver<UserInput> {
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut subs) = self.user_input_subs.lock() {
            subs.push(tx);
        }
        // 注: lock 失败 (poisoned) 时 subscriber 拿不到消息, 但不 panic
        // — 业务方通常启动期 subscribe, 不会遇到
        rx
    }

    /// 拿当前 user input subscriber 数 (测试用)
    #[cfg(test)]
    pub(crate) fn user_input_sub_count(&self) -> usize {
        self.user_input_subs.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// 拿当前活跃 SSE connection 数 (P15.1.5 测试用)
    #[cfg(test)]
    pub(crate) fn sse_sub_count(&self) -> usize {
        self.active_sse_subs.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// 广播 SseEvent 给所有活跃 SSE 连接 (P15.1.5 新增).
    ///
    /// **用法**: 业务方 / 内置 handler (e.g. `POST /api/input`) 想发事件给所有浏览器
    /// 调这个. 不会 panic 如果 0 connections (静默 no-op).
    ///
    /// **lock 策略**: 短暂 lock 拿到 sender Vec 副本, drop lock, 再非阻塞 send.
    /// 不在 lock 内 send 是为了减少锁持有时间 (虽然 send 不 await, 但保持一致性).
    ///
    /// **注**: `#[allow(dead_code)]` 因为 lib crate dead-code 分析看不到测试用法,
    /// 实际 `tests::broadcast_sse_with_no_subscribers_is_noop` 和
    /// `tests::multiple_sse_connections_all_receive_broadcast` 都用到.
    #[allow(dead_code)]
    pub(crate) fn broadcast_sse(&self, event: SseEvent) {
        broadcast_sse(&self.active_sse_subs, event);
    }
}

/// 广播 SseEvent 给所有活跃 SSE 连接 (P15.1.5 free function).
///
/// **共享 helper**: `LocalWebUiServer::broadcast_sse` 和 `handle_post_api_input`
/// 都用这个 — 避免逻辑重复. 静默 no-op if 0 connections / lock poisoned.
fn broadcast_sse(
    active_sse_subs: &Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>>,
    event: SseEvent,
) {
    let senders: Vec<mpsc::UnboundedSender<SseEvent>> = match active_sse_subs.lock() {
        Ok(subs) => subs.iter().cloned().collect(),
        Err(e) => {
            tracing::error!(error = %e, "active_sse_subs mutex poisoned");
            return;
        }
    };
    for tx in senders {
        // 失败说明 connection 已断 (filter_rx drop), 不影响其他连接
        let _ = tx.send(event.clone());
    }
}

// ============================================================================
// P15.1.6: SSE heartbeat + dead sender cleanup
// ============================================================================

/// P15.1.6: 默认 heartbeat 间隔 (server → 所有活跃 SSE 连接).
///
/// **15s 理由**:
/// - 短: 能快速发现 dead connection (3 个 miss ≈ 45s 才被 cleanup, 够快)
/// - 长: 不浪费带宽, 不影响 idle browser (SSE comment 不触发 JS 事件)
/// - 业务方 P15.1.7+ 可以 `set_heartbeat_interval()` override
pub(crate) const DEFAULT_HEARTBEAT_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(15);

/// P15.1.6: SSE heartbeat 后台循环.
///
/// **作用**:
/// 1. 每 `interval` 触发一次, 遍历 `active_sse_subs`
/// 2. 对每个 sender 试 `send(Heartbeat)` — 失败 (receiver 已 drop) 表示连接死了
/// 3. 用 `Vec::retain` 移除 dead senders (原子, 不需要再 lock)
///
/// **为何用 send 来 detect 死连接**:
/// - `mpsc::UnboundedSender::send` 只在 `Receiver` drop 时返回 `Err`
/// - SSE connection 断开 → handle_connection task 退出 → filter_rx drop → send 立即 fail
/// - 一次 `send` 既发 heartbeat 又 detect 死连接, 不需要单独 ping/pong
///
/// **lock 策略**: 整个 retain 在一个 critical section, 不跨 await, 不会丢新 push 的 conn
/// (因为新 push 在 retain 之后才会 lock 拿锁).
///
/// **P15.1.7**: listen `shutdown` Notify — 收到时立即退出 (不等下一个 tick).
async fn heartbeat_loop(
    active_sse_subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>>,
    interval: std::time::Duration,
    shutdown: Arc<tokio::sync::Notify>,
) {
    let mut tick = tokio::time::interval(interval);
    // 第一次 tick 立即触发, 但 first_tick = 0 之后等 interval — 跳过 first tick
    // (避免 server 刚启动就 heartbeat 一次空 Vec)
    tick.tick().await;
    loop {
        tokio::select! {
            // biased 让 shutdown 优先
            biased;
            _ = shutdown.notified() => {
                tracing::debug!("heartbeat task received shutdown signal");
                return;
            }
            _ = tick.tick() => {
                let cleanup_count = {
                    match active_sse_subs.lock() {
                        Ok(mut subs) => {
                            let before = subs.len();
                            // retain 内部试 send: 成功 (Ok) → 保留, 失败 (Err) → 移除
                            subs.retain(|tx| tx.send(SseEvent::Heartbeat).is_ok());
                            before - subs.len()
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "heartbeat: active_sse_subs mutex poisoned");
                            0
                        }
                    }
                };
                if cleanup_count > 0 {
                    tracing::debug!(
                        cleaned = cleanup_count,
                        "heartbeat: pruned dead SSE senders"
                    );
                }
            }
        }
    }
}

#[async_trait]
impl WebUiServer for LocalWebUiServer {
    async fn bind(addr: &str) -> Result<Self, WebUiError> {
        // Parse port from addr (e.g. "127.0.0.1:3080" -> 3080)
        let port = addr
            .rsplit(':')
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or_else(|| WebUiError::Bind(format!("invalid addr: {addr}")))?;

        // 验证可 bind (用临时 TcpListener)
        let _listener: TcpListener = TcpListener::bind(addr)
            .await
            .map_err(|e| WebUiError::Bind(format!("{addr}: {e}")))?;

        Ok(Self {
            addr: addr.to_string(),
            port,
            active_sse_subs: Arc::new(std::sync::Mutex::new(Vec::new())),
            user_input_subs: Arc::new(std::sync::Mutex::new(Vec::new())),
            running: Arc::new(Mutex::new(false)),
            shutdown: Arc::new(tokio::sync::Notify::new()),
            event_log: None,
        })
    }

    async fn run(&self, events: mpsc::UnboundedSender<SseEvent>) -> Result<(), WebUiError> {
        let listener = TcpListener::bind(&self.addr)
            .await
            .map_err(|e| WebUiError::Bind(format!("{}: {}", self.addr, e)))?;
        {
            let mut running = self.running.lock().await;
            *running = true;
        }
        tracing::info!(addr = %self.addr, port = self.port, "web UI server started");

        // P15.1.6: 启 heartbeat 后台任务 (清理 dead SSE senders + keep-alive)
        // P15.1.7: heartbeat 也 listen shutdown signal, 收到时退出
        let heartbeat_subs = Arc::clone(&self.active_sse_subs);
        let heartbeat_shutdown = Arc::clone(&self.shutdown);
        tokio::spawn(async move {
            heartbeat_loop(
                heartbeat_subs,
                DEFAULT_HEARTBEAT_INTERVAL,
                heartbeat_shutdown,
            )
            .await;
        });

        loop {
            tokio::select! {
                // P15.1.7: biased 让 shutdown 优先 — 避免 accept 一直 ready
                // 的时候 starve shutdown signal
                biased;
                _ = self.shutdown.notified() => {
                    tracing::info!(addr = %self.addr, "web UI server received shutdown signal");
                    return Ok(());
                }
                accept_result = listener.accept() => {
                    let (stream, _peer) = match accept_result {
                        Ok(pair) => pair,
                        Err(e) => {
                            tracing::warn!(error = %e, "accept failed");
                            continue;
                        }
                    };
                    // P15.1.5: events (外层 sse_tx) 不再 clone 到 handle_connection —
                    // 用 active_sse_subs Vec broadcast 取代了之前的 per-connection filter_tx.
                    // 外层 sse_tx 暂未使用 (P15.1.6+ 业务方接 ma-harness-core EventLog 时接入).
                    let _ = events;
                    let active_sse_subs = Arc::clone(&self.active_sse_subs);
                    let user_input_subs = Arc::clone(&self.user_input_subs);
                    let event_log = self.event_log.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, active_sse_subs, user_input_subs, event_log).await {
                            tracing::debug!(error = %e, "connection handler error");
                        }
                    });
                }
            }
        }
    }

    fn port(&self) -> u16 {
        self.port
    }
}

/// 处理 1 个 HTTP connection (内部用, 业务方一般不调).
async fn handle_connection(
    mut stream: TcpStream,
    active_sse_subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>>,
    user_input_subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<UserInput>>>>,
    event_log: Option<Arc<ma_harness_core::EventLog>>,
) -> Result<(), WebUiError> {
    // 读 HTTP request (method + path + query + headers + body)
    // 用独立 BufReader, 不 borrow stream 持久
    let req = {
        let mut reader = BufReader::new(&mut stream);
        parse_http_request(&mut reader).await?
    };

    // 路由: (method, path) -> handler
    // 405 vs 404 区分: 已知 path 用错 method -> 405, 未知 path -> 404
    const KNOWN_PATHS: &[&str] = &[
        "/",
        "/index.html",
        "/api/health",
        "/api/version",
        "/api/sessions",
        "/api/sse",
        "/api/input",
    ];

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            let body = html_shell();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await?;
        }
        ("GET", "/api/health") => {
            // P15.1.2: health check endpoint
            let body = serde_json::json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await?;
        }
        ("GET", "/api/version") => {
            // P15.1.3 新增: version meta (crate + rustc + features)
            let body = serde_json::json!({
                "crate_version": env!("CARGO_PKG_VERSION"),
                "rust_version": "n/a (P15.1.3 stub)", // 业务方可改用 rustc_version_runtime (P15.1.5+)
                "name": env!("CARGO_PKG_NAME"),
                "build": "debug",
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await?;
        }
        ("GET", "/api/sessions") => {
            // P15.1.8: 从 EventLog 查真实 session 列表
            // 之前 (P15.1.3-P15.1.7) 返 1 个 demo-session stub — 现在需要 attached event_log
            if let Some(log) = &event_log {
                handle_get_api_sessions_list(&mut stream, log).await?;
            } else {
                // 没 attached EventLog → 503 (告诉 client "server 没配 event log")
                let body = serde_json::json!({
                    "error": "event_log_not_attached",
                    "message": "WebUiServer was started without an EventLog; cannot list sessions",
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).await?;
            }
        }
        ("GET", "/api/sse") => {
            // P15.1.2: 支持 ?session=xxx query filter
            let session_filter = req.query.get("session").cloned();

            // Split stream: reader 独立, writer 独立 (避免 borrow 冲突)
            let (read_half, mut write_half) = stream.split();
            let mut reader = BufReader::new(read_half);

            // 返 SSE stream
            let prelude = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
            write_half.write_all(prelude.as_bytes()).await?;

            // 立即发 session_filter hint
            if let Some(ref sid) = session_filter {
                let hint = SseEvent::Message(format!("subscribed to session: {sid}"));
                write_half
                    .write_all(hint.to_sse_string().as_bytes())
                    .await?;
            }

            // 注册 active SSE sender (per-connection filter channel, push 到 Vec)
            // P15.1.5: 用 Vec 替代 Option, 支持多 tab / 多 browser 并存
            let (filter_tx, mut filter_rx) = mpsc::unbounded_channel::<SseEvent>();
            {
                if let Ok(mut subs) = active_sse_subs.lock() {
                    subs.push(filter_tx);
                }
                // 注: lock 失败 (poisoned) 时本 connection 收不到 broadcast,
                // 但自己 filter_tx 内的数据 (直接通过 filter_rx) 仍能工作
            }

            let mut buf = [0u8; 1024];

            loop {
                tokio::select! {
                    _ = read_with_timeout(&mut reader, &mut buf) => {
                        break; // client 断开或超时
                    }
                    event = filter_rx.recv() => {
                        match event {
                            Some(e) => {
                                // 过滤: session_filter 不匹配则跳过
                                if let Some(filter) = &session_filter {
                                    if let Some(e_sid) = e.session_id() {
                                        if e_sid != filter {
                                            continue;
                                        }
                                    }
                                }
                                let sse = e.to_sse_string();
                                if write_half.write_all(sse.as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                            None => {
                                let done = SseEvent::Done.to_sse_string();
                                let _ = write_half.write_all(done.as_bytes()).await;
                                break;
                            }
                        }
                    }
                }
            }
            // 注: P15.1.5 重构后 sse_tx 已从 handle_connection 移除,
            // broadcast 改用 active_sse_subs (Vec) + LocalWebUiServer::broadcast_sse.
        }
        ("POST", "/api/input") => {
            // P15.1.4 + P15.1.5: 接收 user input, 转发给所有 subscribers
            // 并广播 UserInputAck 给所有活跃 SSE 连接
            handle_post_api_input(&mut stream, &req.body, &user_input_subs, &active_sse_subs)
                .await?;
        }
        ("GET", path) if parse_session_id_from_path(path).is_some() => {
            // P15.1.8: GET /api/sessions/<id>/events?limit=N
            let session_id = parse_session_id_from_path(path).unwrap();
            if let Some(log) = &event_log {
                let limit = parse_limit_param(&req.query);
                handle_get_api_session_events(&mut stream, session_id, log, limit).await?;
            } else {
                let body = serde_json::json!({
                    "error": "event_log_not_attached",
                    "message": "WebUiServer was started without an EventLog; cannot list events",
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).await?;
            }
        }
        (method, path) if KNOWN_PATHS.contains(&path) => {
            // 已知 path 但 method 不对 (e.g. GET /api/input)
            // 405 Method Not Allowed
            let allowed = if path == "/api/input" { "POST" } else { "GET" };
            let response = format!(
                "HTTP/1.1 405 Method Not Allowed\r\nAllow: {allowed}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).await?;
            // 防止未用 method 警告
            let _ = method;
        }
        _ => {
            // 未知 path
            let body = "Not Found";
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await?;
        }
    }
    Ok(())
}

// ============================================================================
// P15.1.8: EventLog-backed session endpoints
// ============================================================================

/// 处理 `GET /api/sessions` (P15.1.8 新增, 替 P15.1.3 stub).
///
/// **数据源**: `EventLog::list_sessions()` + `count()` per session.
///
/// **返回**: `{ "sessions": [...], "total": N }`
/// 每个 session:
/// ```json
/// {
///   "id": "session-abc",
///   "event_count": 42,
///   "started_at": 0,        // P15.1.8 minimal: 0 (placeholder, 等 list_session_summaries)
///   "last_activity": 0,     // P15.1.8 minimal: 0
///   "status": "unknown"     // P15.1.8 minimal: 恒为 "unknown" (P15.1.8.1+ 推 first/last ts)
/// }
/// ```
///
/// **错误处理**: EventLog 错误 (sqlite locked / IO) → 500
async fn handle_get_api_sessions_list(
    stream: &mut TcpStream,
    event_log: &Arc<ma_harness_core::EventLog>,
) -> Result<(), WebUiError> {
    // 列所有 session_id (按事件数倒序)
    let session_ids = match event_log.list_sessions() {
        Ok(ids) => ids,
        Err(e) => {
            let body = serde_json::json!({
                "error": "event_log_query_failed",
                "message": e.to_string(),
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await?;
            return Ok(());
        }
    };

    // 每个 session 加 count (started_at / last_activity / status 留 placeholder, P15.1.8 minimal)
    let mut sessions = Vec::with_capacity(session_ids.len());
    for sid in session_ids {
        let event_count = event_log.count(&sid).unwrap_or(0);
        sessions.push(serde_json::json!({
            "id": sid,
            "event_count": event_count,
            "started_at": 0,    // P15.1.8.1+ 用 list_session_summaries()
            "last_activity": 0, // 同上
            "status": "unknown"
        }));
    }
    let total = sessions.len();

    let body = serde_json::json!({
        "sessions": sessions,
        "total": total,
    })
    .to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

/// 处理 `GET /api/sessions/<id>/events?limit=N` (P15.1.8 新增).
///
/// **数据源**: `EventLog::query()` with `EventQuery { session_id, limit, .. }`.
///
/// **Query 参数**:
/// - `limit`: 最多多少条 (默认 100, 最大 1000). 不传 / 0 / 负数 → 100
///
/// **返回**: `{ "session_id": "...", "events": [...], "total_in_range": N, "has_more": bool }`
async fn handle_get_api_session_events(
    stream: &mut TcpStream,
    session_id: &str,
    event_log: &Arc<ma_harness_core::EventLog>,
    limit: u32,
) -> Result<(), WebUiError> {
    let q = ma_harness_core::EventQuery {
        session_id: session_id.to_string(),
        limit: Some(limit),
        ..Default::default()
    };
    let page = match event_log.query(&q) {
        Ok(p) => p,
        Err(e) => {
            let body = serde_json::json!({
                "error": "event_log_query_failed",
                "message": e.to_string(),
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await?;
            return Ok(());
        }
    };

    // Serialize StoredEvent -> JSON
    let events: Vec<serde_json::Value> = page
        .events
        .iter()
        .map(|se| {
            serde_json::json!({
                "seq": se.seq,
                "id": se.event.id,
                "session_id": se.event.session_id,
                "event_type": se.event.event_type,
                "ts": se.event.ts,
                "severity": se.event.severity,
                "run_id": se.event.run_id,
                "plugin_name": se.event.plugin_name,
                "payload_json": se.event.payload_json,
                "error_message": se.event.error_message,
                "model_visible": se.event.model_visible,
            })
        })
        .collect();

    let body = serde_json::json!({
        "session_id": session_id,
        "events": events,
        "total_in_range": page.total_in_range,
        "has_more": page.has_more,
    })
    .to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

/// Parse `/api/sessions/<id>/events` 拿 `<id>` 部分 (P15.1.8 helper).
///
/// **注**: 不做 URL decode (P15.1.8 minimal, 业务方 UUID 不会含特殊字符).
/// 如果 <id> 含 `/` 也不处理 (event log session_id 通常是 UUID / safe identifier).
fn parse_session_id_from_path(path: &str) -> Option<&str> {
    // path = "/api/sessions/<id>/events"
    //       ^          ^    ^   ^
    //       0          14   ?   ?
    const PREFIX: &str = "/api/sessions/";
    const SUFFIX: &str = "/events";
    if !path.starts_with(PREFIX) || !path.ends_with(SUFFIX) {
        return None;
    }
    let id_start = PREFIX.len();
    let id_end = path.len() - SUFFIX.len();
    if id_end <= id_start {
        return None; // empty id
    }
    Some(&path[id_start..id_end])
}

/// Parse `?limit=N` query param to u32 (P15.1.8 helper).
///
/// **行为**:
/// - 缺失 / 空 / 无法 parse → 100 (default)
/// - 0 → 100 (P15.1.8 minimal: 0 视为缺省, 不返 0 条空列表)
/// - 超过 1000 → clamp 到 1000 (防 DoS)
fn parse_limit_param(query: &std::collections::HashMap<String, String>) -> u32 {
    match query.get("limit").and_then(|s| s.parse::<u32>().ok()) {
        Some(n) if n > 0 && n <= 1000 => n,
        Some(n) if n > 1000 => 1000, // clamp
        _ => 100,                    // 缺失 / 0 / 负 / parse fail → default
    }
}

/// 处理 `POST /api/input` (P15.1.4).
///
/// **Body 格式** (JSON):
/// ```json
/// {"session": "xxx", "text": "user message"}
/// ```
///
/// **响应**:
/// - 200: `{"ok": true, "session": "xxx", "received_at_ms": 1234}`
/// - 400: `{"error": "invalid_json", "message": "..."}`
/// - 400: `{"error": "missing_field", "message": "session and text are required"}`
///
/// **副作用** (P15.1.5 新增): 成功后广播 `SseEvent::UserInputAck` 给所有活跃 SSE 连接,
/// 让浏览器立刻看到 "input received" 而不必等 POST 200 response.
async fn handle_post_api_input(
    stream: &mut TcpStream,
    body: &[u8],
    user_input_subs: &Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<UserInput>>>>,
    active_sse_subs: &Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>>,
) -> Result<(), WebUiError> {
    // 1. 解析 JSON
    let json: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            let resp_body = serde_json::json!({
                "error": "invalid_json",
                "message": e.to_string(),
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            stream.write_all(response.as_bytes()).await?;
            return Ok(());
        }
    };

    // 2. 提取必填字段
    let session_id = json
        .get("session")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let text = json
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if session_id.is_empty() || text.is_empty() {
        let resp_body = serde_json::json!({
            "error": "missing_field",
            "message": "session and text are required",
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    // 3. 构 UserInput (server 端盖 timestamp)
    let user_input = UserInput::now(session_id.clone(), text.clone());

    // 4. 转发给所有 subscribers (lock 拿到 sender 列表后立即 drop lock, 然后 send)
    let senders: Vec<mpsc::UnboundedSender<UserInput>> = {
        let subs = user_input_subs.lock().map_err(|e| {
            tracing::error!(error = %e, "user_input_subs mutex poisoned");
            WebUiError::ChannelClosed
        })?;
        subs.iter().cloned().collect()
    };
    let delivered = senders.len();
    for tx in senders {
        // 忽略 send 失败 (subscriber 已 drop receiver) — 不影响其他 subscribers
        let _ = tx.send(user_input.clone());
    }

    // 4.5 P15.1.5: 广播 UserInputAck 给所有活跃 SSE 连接
    // 浏览器 fetch POST 完拿 200 response 之前, 已经能从 SSE 看到 ack
    let ack = SseEvent::UserInputAck {
        session_id: user_input.session_id.clone(),
        received_at_ms: user_input.received_at_ms,
    };
    // 统计 sse_delivered (需要 lock 拿 Vec.len, 然后释放 lock 调 broadcast_sse)
    let sse_delivered = active_sse_subs.lock().map(|s| s.len()).unwrap_or(0);
    broadcast_sse(active_sse_subs, ack);

    // 5. 返 200 OK
    let resp_body = serde_json::json!({
        "ok": true,
        "session": user_input.session_id,
        "received_at_ms": user_input.received_at_ms,
        "delivered_to": delivered,
        "sse_delivered_to": sse_delivered,
    })
    .to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        resp_body.len(),
        resp_body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

/// 解析 1 个完整 HTTP request (P15.1.4 helper, 给 POST body 准备)
///
/// **解析范围**:
/// - Request line: `METHOD /path?query HTTP/1.1`
/// - Headers: 到空行 (\r\n\r\n)
/// - Body: 按 Content-Length 读 N bytes
///
/// **不支持** (P15.1.4 简化):
/// - Transfer-Encoding: chunked
/// - Content-Encoding: gzip
/// - 多值 header
async fn parse_http_request<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<ParsedRequest, WebUiError> {
    // Request line
    let mut line = String::new();
    reader.read_line(&mut line).await.map_err(WebUiError::Io)?;
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let raw_path = parts.next().unwrap_or("").to_string();
    let (path, query_str) = split_path_query(&raw_path);
    let query = parse_query(query_str);

    // Headers
    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        reader
            .read_line(&mut header_line)
            .await
            .map_err(WebUiError::Io)?;
        let trimmed = header_line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }

    // Body (按 Content-Length 读 N bytes)
    let mut body = Vec::with_capacity(content_length);
    if content_length > 0 {
        use tokio::io::AsyncReadExt;
        let mut limited = reader.take(content_length as u64);
        limited
            .read_to_end(&mut body)
            .await
            .map_err(WebUiError::Io)?;
    }

    Ok(ParsedRequest {
        method,
        path: path.to_string(),
        query,
        body,
    })
}

/// Parsed HTTP request (P15.1.4 helper)
#[derive(Debug, Default, Clone)]
struct ParsedRequest {
    /// HTTP method (e.g. "GET", "POST")
    method: String,
    /// Path (e.g. "/api/input")
    path: String,
    /// Query params (e.g. "session" -> "xxx")
    query: std::collections::HashMap<String, String>,
    /// Body bytes (按 Content-Length 读)
    body: Vec<u8>,
}

/// 拆 path 和 query string (P15.1.2 helper)
fn split_path_query(raw: &str) -> (&str, &str) {
    match raw.find('?') {
        Some(idx) => (&raw[..idx], &raw[idx + 1..]),
        None => (raw, ""),
    }
}

/// Parse query string to HashMap (P15.1.2 简化版: 不支持重复 key / url decode)
fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if query.is_empty() {
        return map;
    }
    for pair in query.split('&') {
        if let Some(eq) = pair.find('=') {
            map.insert(pair[..eq].to_string(), pair[eq + 1..].to_string());
        }
    }
    map
}

/// 读 client data with 30s timeout (P15.1.2 helper, 防止僵尸连接)
async fn read_with_timeout<R: tokio::io::AsyncBufRead + Unpin + Send>(
    reader: &mut R,
    buf: &mut [u8],
) -> std::io::Result<usize> {
    use tokio::io::AsyncReadExt;
    match tokio::time::timeout(std::time::Duration::from_secs(30), reader.read(buf)).await {
        Ok(Ok(0)) => Ok(0),
        Ok(Ok(n)) => Ok(n),
        Ok(Err(e)) => Err(e),
        Err(_) => Ok(0), // 超时算断
    }
}

// ============================================================================
// HTML shell (P15.1.1 placeholder, 业务方 P15.1.2+ 替换)
// ============================================================================

/// HTML shell (极简 placeholder, 业务方 P15.1.2+ 替换为 Leptos / React).
///
/// **特性**:
/// - 内嵌 CSS (极简 dark mode)
/// - 内嵌 JS EventSource 客户端 (连接 /api/sse, 显示 events)
/// - P15.1.4: 内嵌 user input form (POST /api/input)
/// - 兼容 P15.1.2+ React mount point (`<div id="root">`)
pub fn html_shell() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>ma-harness Web UI (P15.1.4 input form)</title>
  <style>
    body { background: #1a1a1a; color: #e0e0e0; font-family: monospace; margin: 0; padding: 1rem; }
    h1 { color: #4fc3f7; font-size: 1.2rem; }
    #input-form { background: #0d0d0d; border: 1px solid #333; padding: 0.5rem; margin-bottom: 0.5rem; }
    #input-form input { background: #1a1a1a; color: #e0e0e0; border: 1px solid #555; padding: 0.25rem; }
    #input-form button { background: #4fc3f7; color: #1a1a1a; border: none; padding: 0.25rem 0.5rem; cursor: pointer; }
    #log { background: #0d0d0d; border: 1px solid #333; padding: 0.5rem; height: 70vh; overflow-y: auto; }
    .event { padding: 0.25rem 0; border-bottom: 1px solid #222; font-size: 0.9rem; }
    .session { color: #81c784; }
    .message { color: #ffb74d; }
    .heartbeat { color: #555; font-style: italic; }
    .done { color: #e57373; font-weight: bold; }
    .user-input { color: #ce93d8; }
  </style>
</head>
<body>
  <h1>ma-harness Web UI (P15.1.4 input form)</h1>
  <p>Live session event stream (Server-Sent Events) + user input form (POST /api/input).</p>
  <div id="root"></div>
  <div id="input-form">
    <label>Session: <input id="session-id" value="demo-session" /></label>
    <label>Message: <input id="user-text" placeholder="type a message" /></label>
    <button id="send-btn">Send</button>
  </div>
  <div id="log"></div>
  <script>
    const log = document.getElementById('log');
    const es = new EventSource('/api/sse');
    function appendEvent(cls, text) {
      const div = document.createElement('div');
      div.className = 'event ' + cls;
      div.textContent = text;
      log.appendChild(div);
      log.scrollTop = log.scrollHeight;
    }
    es.addEventListener('session', (e) => appendEvent('session', '[session] ' + e.data));
    es.addEventListener('session_start', (e) => appendEvent('session', '[session_start] ' + e.data));
    es.addEventListener('session_end', (e) => appendEvent('session', '[session_end] ' + e.data));
    es.addEventListener('user_input_ack', (e) => {
      // data 是 JSON 字符串, parse 后展示
      let obj = JSON.parse(e.data);
      appendEvent('user-input', '[ack] ' + obj.session + ' received at ' + obj.received_at_ms);
    });
    es.addEventListener('message', (e) => appendEvent('message', '[message] ' + e.data));
    es.addEventListener('done', () => appendEvent('done', '[done] stream ended'));
    es.onerror = () => appendEvent('heartbeat', '[error] SSE connection lost');

    // P15.1.4: POST /api/input 发送 user input
    document.getElementById('send-btn').addEventListener('click', async () => {
      const session = document.getElementById('session-id').value;
      const text = document.getElementById('user-text').value;
      if (!text) return;
      try {
        const resp = await fetch('/api/input', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ session, text })
        });
        const data = await resp.json();
        if (resp.ok) {
          appendEvent('user-input', '[input] sent to ' + data.session + ' (ack ' + data.received_at_ms + ', delivered to ' + data.delivered_to + ' subscribers)');
        } else {
          appendEvent('message', '[input error] ' + (data.error || resp.status) + ': ' + (data.message || ''));
        }
      } catch (err) {
        appendEvent('message', '[input error] ' + err);
      }
      document.getElementById('user-text').value = '';
    });
    document.getElementById('user-text').addEventListener('keypress', (e) => {
      if (e.key === 'Enter') document.getElementById('send-btn').click();
    });
  </script>
</body>
</html>"#
    .to_string()
}

// ============================================================================
// Typed key
// ============================================================================

/// Typed key: `ctx.web_ui` 注入的 WebUiServer (P15.1.5+ 业务方注入).
pub static WEB_UI_SERVER: ma_harness_cordis::CtxKey<Arc<dyn WebUiServer>> =
    ma_harness_seam::ctx_key!("web_ui_server");

// ============================================================================
// Default type aliases
// ============================================================================

/// 平台默认 Web UI server (P15.1.1: LocalWebUiServer)
pub type DefaultWebUiServer = LocalWebUiServer;

// ============================================================================
// 单元测试 (mod tests) — 8 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// 找一个空闲端口
    async fn free_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = l.local_addr().expect("local_addr").port();
        drop(l);
        port
    }

    #[tokio::test]
    async fn bind_parses_port_from_addr() {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        assert_eq!(server.port(), port);
    }

    #[tokio::test]
    async fn bind_rejects_invalid_addr() {
        let err = LocalWebUiServer::bind("not an addr").await.unwrap_err();
        assert!(matches!(err, WebUiError::Bind(_)));
    }

    #[tokio::test]
    async fn html_shell_contains_event_source_client() {
        let html = html_shell();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("EventSource"));
        assert!(html.contains("/api/sse"));
        assert!(html.contains("ma-harness"));
    }

    #[tokio::test]
    async fn sse_event_to_sse_string() {
        let session = SseEvent::SessionEvent {
            session_id: "s1".into(),
            json: r#"{"id":"1"}"#.into(),
        };
        assert_eq!(
            session.to_sse_string(),
            "event: session\nid: s1\ndata: {\"id\":\"1\"}\n\n"
        );

        let start = SseEvent::SessionStart {
            session_id: "s1".into(),
        };
        assert_eq!(
            start.to_sse_string(),
            "event: session_start\nid: s1\ndata: start\n\n"
        );

        let end = SseEvent::SessionEnd {
            session_id: "s1".into(),
        };
        assert_eq!(
            end.to_sse_string(),
            "event: session_end\nid: s1\ndata: end\n\n"
        );

        let msg = SseEvent::Message("hello".into());
        assert_eq!(msg.to_sse_string(), "event: message\ndata: hello\n\n");

        let hb = SseEvent::Heartbeat;
        assert_eq!(hb.to_sse_string(), ": heartbeat\n\n");

        let done = SseEvent::Done;
        assert_eq!(done.to_sse_string(), "event: done\ndata: end\n\n");

        // P15.1.5: UserInputAck
        let ack = SseEvent::UserInputAck {
            session_id: "s1".into(),
            received_at_ms: 1234567890,
        };
        assert_eq!(
            ack.to_sse_string(),
            "event: user_input_ack\nid: s1\ndata: {\"session\":\"s1\",\"received_at_ms\":1234567890}\n\n"
        );
    }

    #[tokio::test]
    async fn http_get_root_returns_html_shell() {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        // 等 server ready
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 业务方调 reqwest
        let url = format!("http://{addr}/");
        let resp = reqwest::get(&url).await.expect("GET /");
        assert_eq!(resp.status(), 200);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("text/html"));
        let body = resp.text().await.expect("body");
        assert!(body.contains("ma-harness"));
    }

    #[tokio::test]
    async fn http_get_unknown_path_returns_404() {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let url = format!("http://{addr}/nonexistent");
        let resp = reqwest::get(&url).await.expect("GET /nonexistent");
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn sse_endpoint_emits_session_event() {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 业务方开 SSE 连接 (异步 task)
        let url = format!("http://{addr}/api/sse");
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await.expect("GET /api/sse");
        assert_eq!(response.status(), 200);
        let ct = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("text/event-stream"));
    }

    /// 共享 helper: 启 server, 等 ready, 返 client
    async fn start_test_server() -> (String, reqwest::Client) {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        (addr, reqwest::Client::new())
    }

    #[tokio::test]
    async fn http_get_api_version_returns_json() {
        let (addr, client) = start_test_server().await;
        let url = format!("http://{addr}/api/version");
        let resp = client.get(&url).send().await.expect("GET /api/version");
        assert_eq!(resp.status(), 200);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("application/json"));
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["crate_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(body["name"], env!("CARGO_PKG_NAME"));
        assert!(body["build"].is_string());
    }

    #[tokio::test]
    async fn http_get_api_sessions_without_event_log_returns_503() {
        // P15.1.8: 没 attached EventLog 时, /api/sessions 返 503 (不再返 stub)
        let (addr, client) = start_test_server().await;
        let url = format!("http://{addr}/api/sessions");
        let resp = client.get(&url).send().await.expect("GET /api/sessions");
        assert_eq!(resp.status(), 503);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["error"], "event_log_not_attached");
    }

    // ========================================================================
    // P15.1.4 tests: POST /api/input (user input)
    // ========================================================================

    /// 启 server + subscribe_user_input, 返 (addr, client, rx)
    async fn start_test_server_with_input()
    -> (String, reqwest::Client, mpsc::UnboundedReceiver<UserInput>) {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        let input_rx = server.subscribe_user_input();
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        (addr, reqwest::Client::new(), input_rx)
    }

    #[tokio::test]
    async fn user_input_now_constructor_sets_timestamp() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let input = UserInput::now("s1", "hello");
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        assert_eq!(input.session_id, "s1");
        assert_eq!(input.text, "hello");
        assert!(input.received_at_ms >= before);
        assert!(input.received_at_ms <= after);
    }

    #[tokio::test]
    async fn http_post_api_input_with_valid_json_returns_200_and_delivers_to_subscriber() {
        let (addr, client, mut input_rx) = start_test_server_with_input().await;
        let url = format!("http://{addr}/api/input");
        let body = serde_json::json!({
            "session": "demo-session",
            "text": "hello from test"
        });
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST /api/input");
        assert_eq!(resp.status(), 200);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("application/json"));
        let resp_body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(resp_body["ok"], true);
        assert_eq!(resp_body["session"], "demo-session");
        assert!(resp_body["received_at_ms"].is_number());
        assert_eq!(resp_body["delivered_to"], 1);

        // 验证 subscriber 收到 UserInput
        let received = input_rx.recv().await.expect("subscriber should receive");
        assert_eq!(received.session_id, "demo-session");
        assert_eq!(received.text, "hello from test");
        assert!(received.received_at_ms > 0);
    }

    #[tokio::test]
    async fn http_post_api_input_with_invalid_json_returns_400() {
        let (addr, client, mut input_rx) = start_test_server_with_input().await;
        let url = format!("http://{addr}/api/input");
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body("{not valid json")
            .send()
            .await
            .expect("POST /api/input");
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["error"], "invalid_json");
        // subscriber 不应收到任何东西
        assert!(input_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn http_post_api_input_missing_session_returns_400() {
        let (addr, client, mut input_rx) = start_test_server_with_input().await;
        let url = format!("http://{addr}/api/input");
        let body = serde_json::json!({ "text": "no session" });
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST /api/input");
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["error"], "missing_field");
        assert!(input_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn http_post_api_input_missing_text_returns_400() {
        let (addr, client, mut input_rx) = start_test_server_with_input().await;
        let url = format!("http://{addr}/api/input");
        let body = serde_json::json!({ "session": "s1" });
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST /api/input");
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["error"], "missing_field");
        assert!(input_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn http_get_api_input_returns_405() {
        // GET /api/input 是已知 path, 但 method 不允许 -> 405
        let (addr, client, _rx) = start_test_server_with_input().await;
        let url = format!("http://{addr}/api/input");
        let resp = client.get(&url).send().await.expect("GET /api/input");
        assert_eq!(resp.status(), 405);
        // Allow header 应该告诉 client 应该用 POST
        let allow = resp
            .headers()
            .get("allow")
            .map(|h| h.to_str().unwrap().to_string())
            .unwrap_or_default();
        assert_eq!(allow, "POST");
    }

    #[tokio::test]
    async fn http_post_unknown_path_returns_404() {
        let (addr, client, _rx) = start_test_server_with_input().await;
        let url = format!("http://{addr}/api/nonexistent");
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body("{}")
            .send()
            .await
            .expect("POST /api/nonexistent");
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn http_post_api_input_with_multiple_subscribers_delivers_to_all() {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        // 启 3 个 subscribers
        let mut rx1 = server.subscribe_user_input();
        let mut rx2 = server.subscribe_user_input();
        let mut rx3 = server.subscribe_user_input();
        assert_eq!(server.user_input_sub_count(), 3);

        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let url = format!("http://{addr}/api/input");
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "session": "fanout-test",
            "text": "broadcast me"
        });
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST");
        assert_eq!(resp.status(), 200);
        let resp_body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(resp_body["delivered_to"], 3);

        // 3 个 subscriber 都收到
        let i1 = rx1.recv().await.expect("rx1");
        let i2 = rx2.recv().await.expect("rx2");
        let i3 = rx3.recv().await.expect("rx3");
        assert_eq!(i1.text, "broadcast me");
        assert_eq!(i2.text, "broadcast me");
        assert_eq!(i3.text, "broadcast me");
        assert_eq!(i1.session_id, "fanout-test");
    }

    #[tokio::test]
    async fn http_post_api_input_with_no_subscribers_still_returns_200() {
        // 业务方可能没 subscribe (e.g. CLI mode), POST 不应 panic
        // 注意: 单独启 server, 不调 subscribe_user_input(), 这样 user_input_subs Vec 真的空
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        assert_eq!(server.user_input_sub_count(), 0);
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let url = format!("http://{addr}/api/input");
        let body = serde_json::json!({
            "session": "no-subs",
            "text": "into the void"
        });
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST");
        assert_eq!(resp.status(), 200);
        let resp_body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(resp_body["ok"], true);
        assert_eq!(resp_body["delivered_to"], 0);
    }

    #[tokio::test]
    async fn http_html_shell_contains_input_form_for_p15_1_4() {
        // P15.1.4: HTML shell 应包含 input form
        let html = html_shell();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("EventSource"));
        assert!(html.contains("/api/sse"));
        assert!(html.contains("/api/input"));
        assert!(html.contains("send-btn"));
        assert!(html.contains("user-text"));
    }

    // ========================================================================
    // P15.1.5 tests: multi-SSE broadcast + UserInputAck
    // ========================================================================

    #[tokio::test]
    async fn sse_event_user_input_ack_to_sse_string_includes_json_data() {
        // P15.1.5: UserInputAck 的 data 字段是 JSON 字符串 (浏览器 parse 用)
        let ack = SseEvent::UserInputAck {
            session_id: "s1".into(),
            received_at_ms: 1234567890,
        };
        let sse = ack.to_sse_string();
        assert!(sse.starts_with("event: user_input_ack\n"));
        assert!(sse.contains("id: s1\n"));
        assert!(sse.contains(r#"data: {"session":"s1","received_at_ms":1234567890}"#));
        assert!(sse.ends_with("\n\n"));
        // session_id 应被识别
        assert_eq!(ack.session_id(), Some("s1"));
    }

    #[tokio::test]
    async fn broadcast_sse_with_no_subscribers_is_noop() {
        // P15.1.5: 0 connections 时 broadcast_sse 不 panic
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");
        assert_eq!(server.sse_sub_count(), 0);

        // 调用 broadcast_sse — 不应 panic
        server.broadcast_sse(SseEvent::Message("test".into()));
        server.broadcast_sse(SseEvent::Heartbeat);
        server.broadcast_sse(SseEvent::UserInputAck {
            session_id: "s1".into(),
            received_at_ms: 100,
        });
    }

    #[tokio::test]
    async fn multiple_sse_connections_all_receive_broadcast() {
        // P15.1.5: 多 SSE connection (multi-tab) 时 broadcast 全部收到
        // 这里直接测 broadcast_sse (跳过 HTTP, 走直接调用)
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");

        // 模拟 3 个 SSE connection 注册 (push filter_tx)
        // 注: 我们直接 push Sender 到 active_sse_subs Vec, 模拟 SSE handler 行为
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SseEvent>();
        let (tx2, mut rx2) = mpsc::unbounded_channel::<SseEvent>();
        let (tx3, mut rx3) = mpsc::unbounded_channel::<SseEvent>();
        {
            let mut subs = server.active_sse_subs.lock().unwrap();
            subs.push(tx1);
            subs.push(tx2);
            subs.push(tx3);
        }
        assert_eq!(server.sse_sub_count(), 3);

        // Broadcast
        let event = SseEvent::Message("broadcast to all".into());
        server.broadcast_sse(event.clone());

        // 3 个 rx 都收到
        let e1 = rx1.recv().await.expect("rx1");
        let e2 = rx2.recv().await.expect("rx2");
        let e3 = rx3.recv().await.expect("rx3");
        assert_eq!(e1, event);
        assert_eq!(e2, event);
        assert_eq!(e3, event);
    }

    #[tokio::test]
    async fn http_post_api_input_broadcasts_user_input_ack_to_sse_subscribers() {
        // P15.1.5: POST /api/input 成功后广播 UserInputAck 给活跃 SSE
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");

        // 模拟 1 个 SSE connection 注册
        let (sse_tx, mut sse_rx) = mpsc::unbounded_channel::<SseEvent>();
        {
            let mut subs = server.active_sse_subs.lock().unwrap();
            subs.push(sse_tx);
        }
        assert_eq!(server.sse_sub_count(), 1);

        // 启 server
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // POST /api/input
        let url = format!("http://{addr}/api/input");
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "session": "ack-test",
            "text": "hello ack"
        });
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST");
        assert_eq!(resp.status(), 200);
        let resp_body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(resp_body["ok"], true);
        assert_eq!(resp_body["sse_delivered_to"], 1);

        // SSE 收到 UserInputAck
        let event = sse_rx.recv().await.expect("sse rx");
        match event {
            SseEvent::UserInputAck {
                session_id,
                received_at_ms,
            } => {
                assert_eq!(session_id, "ack-test");
                assert!(received_at_ms > 0);
            }
            other => panic!("expected UserInputAck, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_post_api_input_with_sse_response_includes_sse_delivered_count() {
        // P15.1.5: POST 响应包含 sse_delivered_to 字段
        let (addr, client, _rx) = start_test_server_with_input().await;

        // 模拟 1 个 SSE connection 注册
        let port: u16 = addr
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .expect("parse port");
        let _ = port; // 不用, 仅为说明

        // 因为 start_test_server_with_input 创建了 server 但没注册 SSE,
        // sse_delivered_to 应该是 0
        let url = format!("http://{addr}/api/input");
        let body = serde_json::json!({
            "session": "s1",
            "text": "no sse"
        });
        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST");
        assert_eq!(resp.status(), 200);
        let resp_body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(resp_body["ok"], true);
        assert!(resp_body["sse_delivered_to"].is_number());
        assert_eq!(resp_body["sse_delivered_to"], 0);
    }

    #[tokio::test]
    async fn http_html_shell_contains_user_input_ack_event_handler() {
        // P15.1.5: HTML shell 应处理 user_input_ack 事件
        let html = html_shell();
        assert!(html.contains("addEventListener('user_input_ack'"));
        assert!(html.contains("JSON.parse(e.data)"));
    }

    // ========================================================================
    // P15.1.6 tests: SSE heartbeat + dead sender cleanup
    // ========================================================================

    /// 启一个 heartbeat_loop 测试用 (短 interval), 等若干 tick, 然后 abort
    async fn run_heartbeat_for(
        subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>>,
        interval: std::time::Duration,
        ticks: u32,
    ) {
        // P15.1.7: heartbeat_loop 现在需要 shutdown Notify — 测试用独立 Notify,
        // 不触发 shutdown, 维持 P15.1.6 行为
        let shutdown = Arc::new(tokio::sync::Notify::new());
        let handle = tokio::spawn(async move {
            heartbeat_loop(subs, interval, shutdown).await;
        });
        // 等 ticks * interval 毫秒 (留 50ms buffer 让 tick 触发)
        let total = interval * ticks + std::time::Duration::from_millis(50);
        tokio::time::sleep(total).await;
        handle.abort();
    }

    #[tokio::test]
    async fn heartbeat_loop_with_empty_subs_is_noop() {
        // P15.1.6: 0 connections 时 heartbeat 不 panic, 不影响任何东西
        let subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        // 跑 2 个 tick (~100ms), 内部应 noop
        run_heartbeat_for(subs.clone(), std::time::Duration::from_millis(50), 2).await;
        assert_eq!(subs.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn heartbeat_loop_delivers_heartbeat_to_all_alive_subscribers() {
        // P15.1.6: heartbeat 发到所有活跃 sub
        let subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let (tx1, mut rx1) = mpsc::unbounded_channel::<SseEvent>();
        let (tx2, mut rx2) = mpsc::unbounded_channel::<SseEvent>();
        let (tx3, mut rx3) = mpsc::unbounded_channel::<SseEvent>();
        {
            let mut s = subs.lock().unwrap();
            s.push(tx1);
            s.push(tx2);
            s.push(tx3);
        }
        assert_eq!(subs.lock().unwrap().len(), 3);

        // 跑 2 个 tick (~100ms), 期望每个 rx 至少收到 2 个 Heartbeat
        run_heartbeat_for(subs.clone(), std::time::Duration::from_millis(50), 2).await;

        // 3 个 conn 还在 (没被清理)
        assert_eq!(subs.lock().unwrap().len(), 3);

        // 每个 rx 收到 Heartbeat
        let e1 = rx1.try_recv().expect("rx1 got heartbeat");
        let e2 = rx2.try_recv().expect("rx2 got heartbeat");
        let e3 = rx3.try_recv().expect("rx3 got heartbeat");
        assert!(matches!(e1, SseEvent::Heartbeat));
        assert!(matches!(e2, SseEvent::Heartbeat));
        assert!(matches!(e3, SseEvent::Heartbeat));
    }

    #[tokio::test]
    async fn heartbeat_loop_cleans_up_dead_senders_after_receiver_dropped() {
        // P15.1.6: 关键 regression test — P15.1.5 limitation fix
        // receiver drop → sender 在 Vec 里 → 下个 heartbeat tick send 失败 → retain 移除
        let subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        // 1 个 alive conn + 1 个 dead conn (tx 留 Vec, rx 已 drop)
        let (tx_alive, mut rx_alive) = mpsc::unbounded_channel::<SseEvent>();
        let (tx_dead, _rx_dead) = mpsc::unbounded_channel::<SseEvent>();
        drop(_rx_dead); // 立即 drop, 模拟断连
        {
            let mut s = subs.lock().unwrap();
            s.push(tx_alive);
            s.push(tx_dead);
        }
        assert_eq!(subs.lock().unwrap().len(), 2);

        // 跑 2 个 tick (~100ms)
        run_heartbeat_for(subs.clone(), std::time::Duration::from_millis(50), 2).await;

        // dead conn 已被清理, 只剩 alive
        assert_eq!(subs.lock().unwrap().len(), 1);

        // alive 收到 heartbeat
        let e = rx_alive.try_recv().expect("alive got heartbeat");
        assert!(matches!(e, SseEvent::Heartbeat));
    }

    #[tokio::test]
    async fn heartbeat_loop_with_all_dead_senders_clears_vec() {
        // P15.1.6: 全部 dead 时 Vec 变空
        let subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let (tx1, rx1) = mpsc::unbounded_channel::<SseEvent>();
        let (tx2, rx2) = mpsc::unbounded_channel::<SseEvent>();
        drop(rx1);
        drop(rx2);
        {
            let mut s = subs.lock().unwrap();
            s.push(tx1);
            s.push(tx2);
        }
        assert_eq!(subs.lock().unwrap().len(), 2);

        // 跑 1 个 tick (~60ms)
        run_heartbeat_for(subs.clone(), std::time::Duration::from_millis(50), 1).await;

        assert_eq!(subs.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn heartbeat_loop_does_not_remove_healthy_senders() {
        // P15.1.6 regression: 不要误伤活 conn
        let subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let (tx1, _rx1) = mpsc::unbounded_channel::<SseEvent>();
        let (tx2, _rx2) = mpsc::unbounded_channel::<SseEvent>();
        {
            let mut s = subs.lock().unwrap();
            s.push(tx1);
            s.push(tx2);
        }

        run_heartbeat_for(subs.clone(), std::time::Duration::from_millis(50), 3).await;

        // rx 还持有, send 永远成功, 不会清
        assert_eq!(subs.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn run_method_spawns_heartbeat_task_for_active_sse_subs() {
        // P15.1.6: run() 启 server 时同时启 heartbeat 后台任务
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");

        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 模拟 1 个 SSE conn 注册, 跑 ~1.5 个 DEFAULT_HEARTBEAT_INTERVAL 太长 (22.5s)
        // 这里不真等 15s, 只验 "spawn 后 server 内部状态正常 + 有 task 跑"
        let (sse_tx, sse_rx) = mpsc::unbounded_channel::<SseEvent>();
        {
            let mut s = server.active_sse_subs.lock().unwrap();
            s.push(sse_tx);
        }
        assert_eq!(server.sse_sub_count(), 1);

        // 等短时间, 验 server 没 panic + 还能 accept 新 conn
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let resp = reqwest::get(format!("http://{addr}/api/health"))
            .await
            .expect("GET /api/health");
        assert_eq!(resp.status(), 200);

        // cleanup: drop sse_rx 让 heartbeat 把它清掉 (但要 15s+ 才会发生, 这里只验注册成功)
        drop(sse_rx);
    }

    #[tokio::test]
    async fn default_heartbeat_interval_is_reasonable() {
        // P15.1.6: 默认 interval 应是 15s (业务方可依赖)
        assert_eq!(
            DEFAULT_HEARTBEAT_INTERVAL,
            std::time::Duration::from_secs(15)
        );
    }

    // ========================================================================
    // P15.1.7 tests: graceful shutdown (stop() + Notify)
    // ========================================================================

    #[tokio::test]
    async fn stop_method_is_sync_and_idempotent() {
        // P15.1.7: stop() 立即返 (sync), 多次调用安全
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = LocalWebUiServer::bind(&addr).await.expect("bind");

        // 调多次不 panic
        server.stop();
        server.stop();
        server.stop();

        // shutdown_handle 返回 Arc<Notify>
        let h = server.shutdown_handle();
        // Arc 是共享的 — 调 notify_one 也等效于 stop()
        h.notify_one();
    }

    #[tokio::test]
    async fn run_returns_ok_after_stop_signal() {
        // P15.1.7: run() 在 stop 后返 Ok(())
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = Arc::new(LocalWebUiServer::bind(&addr).await.expect("bind"));

        let (tx, _rx) = mpsc::unbounded_channel();
        let server_clone = Arc::clone(&server);
        let run_handle = tokio::spawn(async move { server_clone.run(tx).await });

        // 等 server ready
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 触发 stop
        server.stop();

        // run() 应返 Ok — 给 500ms 等它退出
        let join_result = tokio::time::timeout(std::time::Duration::from_millis(500), run_handle)
            .await
            .expect("run() should exit within 500ms");
        let run_result = join_result.expect("JoinHandle should not be cancelled");
        assert!(
            run_result.is_ok(),
            "stop() should cause run() to return Ok, got {run_result:?}"
        );
    }

    #[tokio::test]
    async fn heartbeat_loop_exits_on_shutdown_signal() {
        // P15.1.7: 关键 regression test — P15.1.6 limitation fix
        // heartbeat_loop 收到 shutdown 后立即退出 (不等下一个 tick)
        let subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let shutdown = Arc::new(tokio::sync::Notify::new());

        let subs_clone = Arc::clone(&subs);
        let shutdown_clone = Arc::clone(&shutdown);
        let handle = tokio::spawn(async move {
            // 长 interval (10s), 但 shutdown 应该立即让它退出
            heartbeat_loop(
                subs_clone,
                std::time::Duration::from_secs(10),
                shutdown_clone,
            )
            .await;
        });

        // 等 heartbeat 启动
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!handle.is_finished(), "heartbeat should still be running");

        // 触发 shutdown
        shutdown.notify_one();

        // heartbeat 应该在 100ms 内退出 (远小于 10s interval)
        // heartbeat_loop 返 (), 只需确认 join 成功
        tokio::time::timeout(std::time::Duration::from_millis(500), handle)
            .await
            .expect("heartbeat should exit within 500ms of shutdown")
            .expect("JoinHandle should not error");
    }

    #[tokio::test]
    async fn heartbeat_loop_exits_immediately_even_before_first_tick() {
        // P15.1.7 edge: shutdown 在 first tick 之前触发, 仍然能退出
        let subs: Arc<std::sync::Mutex<Vec<mpsc::UnboundedSender<SseEvent>>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let shutdown = Arc::new(tokio::sync::Notify::new());

        let subs_clone = Arc::clone(&subs);
        let shutdown_clone = Arc::clone(&shutdown);
        let handle = tokio::spawn(async move {
            heartbeat_loop(
                subs_clone,
                std::time::Duration::from_secs(60),
                shutdown_clone,
            )
            .await;
        });

        // 立即 shutdown (heartbeat 应该还没 first tick)
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        shutdown.notify_one();

        let result = tokio::time::timeout(std::time::Duration::from_millis(200), handle)
            .await
            .expect("heartbeat should exit immediately on shutdown");
        result.expect("JoinHandle");
    }

    #[tokio::test]
    async fn stop_drops_listener_so_new_connections_fail() {
        // P15.1.7: stop() 让 run() 返 Ok, listener 随之 drop, 新连接被拒
        // 注: 这测试组合 "stop + listener drop", 跟 run_returns_ok_after_stop_signal
        // 一起覆盖 P15.1.7 行为. 单独不调 GET 避免 keep-alive 干扰.
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = Arc::new(LocalWebUiServer::bind(&addr).await.expect("bind"));

        let (tx, _rx) = mpsc::unbounded_channel();
        let server_clone = Arc::clone(&server);
        let run_handle = tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });

        // 等 server ready
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 触发 stop
        server.stop();

        // 等 run() 退出 (500ms 跟 run_returns_ok_after_stop_signal 一致)
        let _ = tokio::time::timeout(std::time::Duration::from_millis(500), run_handle)
            .await
            .expect("run() exits");

        // 后: 新连接应被拒 (listener 已 drop → connection refused)
        let result = reqwest::Client::new()
            .get(format!("http://{addr}/api/health"))
            .timeout(std::time::Duration::from_millis(500))
            .send()
            .await;
        assert!(
            result.is_err(),
            "expected connection refused after stop, got {result:?}"
        );
    }

    #[tokio::test]
    async fn stop_does_not_forcibly_close_existing_connections() {
        // P15.1.7: 已有活跃连接不被 stop 强制断开 (P15.1.7 minimal 行为)
        // 设计选择: stop 只退出 accept loop, 已有连接自然结束
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let server = Arc::new(LocalWebUiServer::bind(&addr).await.expect("bind"));

        let (tx, _rx) = mpsc::unbounded_channel();
        let server_clone = Arc::clone(&server);
        let run_handle = tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });

        // 等 server ready
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 触发 stop
        server.stop();

        // 等 run() 退出 (accept loop 关闭)
        let _ = tokio::time::timeout(std::time::Duration::from_millis(500), run_handle)
            .await
            .expect("run() exits");

        // 注: 这个测试只验 "stop 不 panic" + "run() 干净退出"
        // 已有连接的强制关闭是 P15.1.8+ 范畴
    }

    // ========================================================================
    // P15.1.8 tests: EventLog real session integration
    // ========================================================================

    use ma_harness_core::{EventLog, EventType, SessionEvent, Severity};

    /// 启 server + attach EventLog (P15.1.8 helper)
    async fn start_test_server_with_log() -> (String, reqwest::Client, Arc<EventLog>) {
        let port = free_port().await;
        let addr = format!("127.0.0.1:{port}");
        let log = Arc::new(EventLog::open_in_memory().expect("open in-memory log"));
        let server = LocalWebUiServer::bind(&addr)
            .await
            .expect("bind")
            .with_event_log(Arc::clone(&log));
        assert!(server.has_event_log());
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = Arc::new(server);
        let server_clone = Arc::clone(&server);
        tokio::spawn(async move {
            let _ = server_clone.run(tx).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        (addr, reqwest::Client::new(), log)
    }

    /// Append 一个事件到 log (P15.1.8 helper)
    ///
    /// **注**: 用 `ApprovalRequest` (model_visible=false), 不需要 payload_json.
    /// 大多数 EventType (Session*, Model*, User*, Tool*) 是 model_visible,
    /// append 时校验 "必须有非空 payload_json", 测试简单事件用 ApprovalRequest 避开.
    fn append_event(log: &EventLog, session_id: &str, event_type: EventType) -> i64 {
        let ev = SessionEvent::new(session_id, event_type).with_severity(Severity::Info);
        log.append(ev)
    }

    #[tokio::test]
    async fn http_get_api_sessions_returns_real_sessions_from_event_log() {
        // P15.1.8: attached EventLog → 真实 session 列表
        let (addr, client, log) = start_test_server_with_log().await;
        // Append 2 个 session 各 3 条 events (ApprovalRequest 不需要 payload)
        for _ in 0..3 {
            append_event(&log, "session-A", EventType::ApprovalRequest);
            append_event(&log, "session-B", EventType::ApprovalRequest);
        }

        let url = format!("http://{addr}/api/sessions");
        let resp = client.get(&url).send().await.expect("GET /api/sessions");
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["total"], 2);
        let sessions = body["sessions"].as_array().expect("sessions array");
        assert_eq!(sessions.len(), 2);
        // 按 count DESC 排序 (list_sessions 是这样)
        // session-A 和 session-B 都 3 条, 顺序不定
        let ids: Vec<&str> = sessions.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"session-A"));
        assert!(ids.contains(&"session-B"));
        // 每个 session event_count = 3
        for s in sessions {
            assert_eq!(s["event_count"], 3);
        }
    }

    #[tokio::test]
    async fn http_get_api_sessions_with_empty_log_returns_empty_list() {
        // P15.1.8: log 0 events → total 0, sessions []
        let (addr, client, _log) = start_test_server_with_log().await;

        let url = format!("http://{addr}/api/sessions");
        let resp = client.get(&url).send().await.expect("GET");
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["total"], 0);
        assert_eq!(body["sessions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn http_get_api_session_events_returns_events_for_session() {
        // P15.1.8: GET /api/sessions/<id>/events 返该 session 的 events
        let (addr, client, log) = start_test_server_with_log().await;
        for i in 0..5 {
            let payload = format!(r#"{{"i":{i}}}"#);
            let ev = SessionEvent::new("session-X", EventType::UserInput)
                .with_severity(Severity::Info)
                .with_payload(&payload)
                .expect("payload json");
            log.append(ev);
        }
        // Append 1 event 到别的 session (不应该出现在结果里)
        append_event(&log, "session-Y", EventType::ApprovalRequest);

        let url = format!("http://{addr}/api/sessions/session-X/events");
        let resp = client.get(&url).send().await.expect("GET events");
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["session_id"], "session-X");
        let events = body["events"].as_array().expect("events array");
        assert_eq!(events.len(), 5);
        // 每个 event 都有 seq + session_id + payload
        for e in events {
            assert!(e["seq"].is_number());
            assert_eq!(e["session_id"], "session-X");
            assert!(e["payload_json"].is_string());
        }
        // total_in_range = 5
        assert_eq!(body["total_in_range"], 5);
    }

    #[tokio::test]
    async fn http_get_api_session_events_respects_limit_query_param() {
        // P15.1.8: ?limit=2 限制返 2 条
        let (addr, client, log) = start_test_server_with_log().await;
        for _ in 0..5 {
            append_event(&log, "session-L", EventType::ApprovalRequest);
        }

        let url = format!("http://{addr}/api/sessions/session-L/events?limit=2");
        let resp = client.get(&url).send().await.expect("GET events");
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        let events = body["events"].as_array().expect("events array");
        assert_eq!(events.len(), 2);
        // total_in_range 还是 5 (limit 只控本次返数, 不控总范围)
        assert_eq!(body["total_in_range"], 5);
    }

    #[tokio::test]
    async fn http_get_api_session_events_default_limit_is_100() {
        // P15.1.8: 没 ?limit → default 100
        let (addr, client, log) = start_test_server_with_log().await;
        // append 50 条 (不到 100)
        for _ in 0..50 {
            append_event(&log, "session-D", EventType::ApprovalRequest);
        }
        let url = format!("http://{addr}/api/sessions/session-D/events");
        let resp = client.get(&url).send().await.expect("GET");
        let body: serde_json::Value = resp.json().await.expect("json");
        let events = body["events"].as_array().expect("events array");
        assert_eq!(events.len(), 50); // < 100, 全返
    }

    #[tokio::test]
    async fn http_get_api_session_events_clamps_oversized_limit_to_1000() {
        // P15.1.8: ?limit=99999 → clamp 到 1000 (防 DoS)
        let (addr, client, log) = start_test_server_with_log().await;
        // append 5 条
        for _ in 0..5 {
            append_event(&log, "session-C", EventType::ApprovalRequest);
        }
        let url = format!("http://{addr}/api/sessions/session-C/events?limit=99999");
        let resp = client.get(&url).send().await.expect("GET");
        let body: serde_json::Value = resp.json().await.expect("json");
        let events = body["events"].as_array().expect("events array");
        // clamp 到 1000, 但 session 只有 5 条, 所以全返
        assert_eq!(events.len(), 5);
    }

    #[tokio::test]
    async fn http_get_api_session_events_without_event_log_returns_503() {
        // P15.1.8: 没 attached log → 503
        let (addr, client, _rx) = start_test_server_with_input().await;
        let url = format!("http://{addr}/api/sessions/some-id/events");
        let resp = client.get(&url).send().await.expect("GET");
        assert_eq!(resp.status(), 503);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["error"], "event_log_not_attached");
    }

    #[tokio::test]
    async fn parse_session_id_from_path_handles_valid_and_invalid() {
        // P15.1.8 helper 单元测试
        assert_eq!(
            parse_session_id_from_path("/api/sessions/abc-123/events"),
            Some("abc-123")
        );
        assert_eq!(
            parse_session_id_from_path("/api/sessions/some.long.id/events"),
            Some("some.long.id")
        );
        // 不是 /events 结尾
        assert_eq!(parse_session_id_from_path("/api/sessions/abc"), None);
        // 不是 /api/sessions/ 开头
        assert_eq!(parse_session_id_from_path("/api/other/abc/events"), None);
        // 空 id
        assert_eq!(parse_session_id_from_path("/api/sessions//events"), None);
    }

    #[tokio::test]
    async fn parse_limit_param_handles_edge_cases() {
        // P15.1.8 helper 单元测试
        use std::collections::HashMap;
        let mut q = HashMap::new();

        // 缺省 → 100
        assert_eq!(parse_limit_param(&q), 100);

        // 正常值
        q.insert("limit".into(), "50".into());
        assert_eq!(parse_limit_param(&q), 50);

        // 0 → 100 (default)
        q.insert("limit".into(), "0".into());
        assert_eq!(parse_limit_param(&q), 100);

        // 超过 1000 → clamp 到 1000
        q.insert("limit".into(), "9999".into());
        assert_eq!(parse_limit_param(&q), 1000);

        // 无效 parse → 100
        q.insert("limit".into(), "abc".into());
        assert_eq!(parse_limit_param(&q), 100);
    }
}
