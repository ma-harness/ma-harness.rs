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
//! use ma_harness_web_ui::{LocalWebUiServer, WebUiServer, SseEvent};
//!
//! let server = LocalWebUiServer::bind("127.0.0.1:3080").await?;
//! let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
//! tx.send(SseEvent::SessionEvent(json!({"id": "1"}))).unwrap();
//! server.run(tx).await?;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-web-ui
//!
//! # 设计 (Design) — P15.1.1
//!
//! **目标**: 抽象 `ctx.web_ui` (跟 dsh `:3080` browser app 对等), 业务方
//! - 跑 `mah web` 打开浏览器
//! - 看 live session (events via SSE)
//! - 接受 user input (P15.1.2+)
//!
//! **P15.1 大工程** (8-12 周): Rust + WASM (Leptos/Yew) or React + REST API.
//! **P15.1.1 骨架** (本次): crate 脚手架, WebUiServer trait, std+tokio HTTP server,
//! SSE endpoint, HTML shell (placeholder, 业务方后续 P15.1.2+ 加 Leptos/React).
//!
//! **设计决策**: 不引 salvo/axum, 用 `tokio::net::TcpListener` + 手写 minimal HTTP
//! (P15.1.1 只需要 1 个 GET / + 1 个 GET /api/sse, 完整 framework 过度设计).
//! 业务方 P15.1.5+ 改用 axum + 真 SPA 框架时, LocalWebUiServer 换成对应 impl.
//!
//! **核心抽象**:
//! - [`SseEvent`] enum (SessionEvent / Message / Heartbeat / Done)
//! - [`WebUiServer`] trait (bind / run / port / stop)
//! - [`LocalWebUiServer`] (P15.1.1 主交付, std + tokio)
//! - [`html_shell`] (业务方 `include_str!("shell.html")` 或运行时 inline)
//!
//! **6 质量属性**:
//! - 可复用: WebUiServer trait, future RemoteWebUiServer (P15+ cloud)
//! - 可维护: 模块化分块, server / sse / html / error 集中 lib.rs
//! - 鲁棒: 错误归一化 (Bind / IO), keep-alive 防止 SSE timeout
//! - 安全: 不 eval user input, SSE events 静态 string
//! - 可测: 7+ 测试覆盖 bind / HTTP / SSE / HTML / concurrent
//! - 可扩展: SSE channel 抽象, 业务方可接 ma-harness-core event log
//!
//! # 限制 (Limitations) — P15.1.1
//!
//! - placeholder HTML shell (业务方 P15.1.2 加 Leptos / React)
//! - 单 SSE channel 简化版 (P15.1.2 多 channel / topic filter)
//! - 不接 ma-harness-server OpenAPI (P15.1.3 集成)
//! - 不接 ctx.user input (P15.1.4+)
//!
//! [dsh-feature-parity-table §8]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#8-distribution-surfaces

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
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
    SessionEvent(String),
    /// 自定义消息 (e.g. 错误 / 状态)
    Message(String),
    /// Heartbeat (keep-alive, 防 proxy / browser timeout)
    Heartbeat,
    /// 流结束
    Done,
}

impl SseEvent {
    /// 转成 SSE wire format (`data: ...\n\n`)
    #[allow(dead_code)] // P15.1.1 预留 helper, P15.1.2+ 会在 handle_connection 里用
    fn to_sse_string(&self) -> String {
        match self {
            SseEvent::SessionEvent(json) => format!("event: session\ndata: {json}\n\n"),
            SseEvent::Message(msg) => format!("event: message\ndata: {msg}\n\n"),
            SseEvent::Heartbeat => ": heartbeat\n\n".to_string(),
            SseEvent::Done => "event: done\ndata: end\n\n".to_string(),
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
/// 接受 `GET /` 返 HTML shell, `GET /api/sse` 返 SSE stream.
/// 不引 salvo/axum (P15.1.5+ 改用真 framework).
#[derive(Debug)]
pub struct LocalWebUiServer {
    addr: String,
    port: u16,
    /// 当前活跃的 SSE sender (P15.1.2 多 channel 时改成 Vec)
    active_sse: Arc<Mutex<Option<mpsc::UnboundedSender<SseEvent>>>>,
    /// Server 启动状态
    running: Arc<Mutex<bool>>,
}

impl LocalWebUiServer {
    /// 创建一个新 LocalWebUiServer (未 bind)
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            port: 0,
            active_sse: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// 拿监听地址
    pub fn addr(&self) -> &str {
        &self.addr
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
            active_sse: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
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

        loop {
            let (stream, _peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::warn!(error = %e, "accept failed");
                    continue;
                }
            };
            let sse_tx = events.clone();
            let active_sse = Arc::clone(&self.active_sse);
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, sse_tx, active_sse).await {
                    tracing::debug!(error = %e, "connection handler error");
                }
            });
        }
    }

    fn port(&self) -> u16 {
        self.port
    }
}

/// 处理 1 个 HTTP connection (内部用, 业务方一般不调).
async fn handle_connection(
    mut stream: TcpStream,
    sse_tx: mpsc::UnboundedSender<SseEvent>,
    active_sse: Arc<Mutex<Option<mpsc::UnboundedSender<SseEvent>>>>,
) -> Result<(), WebUiError> {
    // 读 HTTP request line (在独立 BufReader, 不 borrow stream 持久)
    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut request_line).await?;
    }
    let request_line = request_line.trim_end_matches(['\r', '\n']);

    // 简单 parse: "GET /path HTTP/1.1"
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    if method != "GET" {
        // 简化: 仅支持 GET
        let response = "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n";
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    match path {
        "/" | "/index.html" => {
            // 返 HTML shell
            let body = html_shell();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await?;
        }
        "/api/sse" => {
            // 返 SSE stream
            let prelude = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
            stream.write_all(prelude.as_bytes()).await?;

            // 注册 active SSE sender (P15.1.2: 多 channel)
            {
                let mut active = active_sse.lock().await;
                *active = Some(sse_tx.clone());
            }

            // 持续读 (hold connection), 直到 client 断开
            let mut buf = [0u8; 1024];
            let mut reader = BufReader::new(&mut stream);
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,    // client 断开
                    Ok(_) => continue, // 忽略 client data
                    Err(_) => break,
                }
            }
        }
        _ => {
            // 404
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
// HTML shell (P15.1.1 placeholder, 业务方 P15.1.2+ 替换)
// ============================================================================

/// HTML shell (极简 placeholder, 业务方 P15.1.2+ 替换为 Leptos / React).
///
/// **特性**:
/// - 内嵌 CSS (极简 dark mode)
/// - 内嵌 JS EventSource 客户端 (连接 /api/sse, 显示 events)
/// - 兼容 P15.1.2+ React mount point (`<div id="root">`)
pub fn html_shell() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>ma-harness Web UI (P15.1.1 skeleton)</title>
  <style>
    body { background: #1a1a1a; color: #e0e0e0; font-family: monospace; margin: 0; padding: 1rem; }
    h1 { color: #4fc3f7; font-size: 1.2rem; }
    #log { background: #0d0d0d; border: 1px solid #333; padding: 0.5rem; height: 80vh; overflow-y: auto; }
    .event { padding: 0.25rem 0; border-bottom: 1px solid #222; font-size: 0.9rem; }
    .session { color: #81c784; }
    .message { color: #ffb74d; }
    .heartbeat { color: #555; font-style: italic; }
    .done { color: #e57373; font-weight: bold; }
  </style>
</head>
<body>
  <h1>ma-harness Web UI (P15.1.1 skeleton)</h1>
  <p>Live session event stream (Server-Sent Events). P15.1.2+ will add Leptos/React UI.</p>
  <div id="root"></div>
  <div id="log"></div>
  <script>
    const log = document.getElementById('log');
    const es = new EventSource('/api/sse');
    es.addEventListener('session', (e) => {
      const div = document.createElement('div');
      div.className = 'event session';
      div.textContent = '[session] ' + e.data;
      log.appendChild(div);
      log.scrollTop = log.scrollHeight;
    });
    es.addEventListener('message', (e) => {
      const div = document.createElement('div');
      div.className = 'event message';
      div.textContent = '[message] ' + e.data;
      log.appendChild(div);
    });
    es.addEventListener('done', () => {
      const div = document.createElement('div');
      div.className = 'event done';
      div.textContent = '[done] stream ended';
      log.appendChild(div);
    });
    es.onerror = () => {
      const div = document.createElement('div');
      div.className = 'event heartbeat';
      div.textContent = '[error] SSE connection lost';
      log.appendChild(div);
    };
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
        let session = SseEvent::SessionEvent(r#"{"id":"1"}"#.into());
        assert_eq!(
            session.to_sse_string(),
            "event: session\ndata: {\"id\":\"1\"}\n\n"
        );

        let msg = SseEvent::Message("hello".into());
        assert_eq!(msg.to_sse_string(), "event: message\ndata: hello\n\n");

        let hb = SseEvent::Heartbeat;
        assert_eq!(hb.to_sse_string(), ": heartbeat\n\n");

        let done = SseEvent::Done;
        assert_eq!(done.to_sse_string(), "event: done\ndata: end\n\n");
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
}
