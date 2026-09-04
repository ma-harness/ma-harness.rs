//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-lsp`
//! **Crate ident** (`use` 路径): `ma_harness_lsp`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-lsp = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_lsp::{LspService, LspSpec, LspResponse, LocalLspProvider};
//!
//! let provider = LocalLspProvider::new("rust-analyzer", &[]);
//! let spec = LspSpec::request(
//!     1,
//!     "textDocument/definition",
//!     serde_json::json!({
//!         "textDocument": { "uri": "file:///foo.rs" },
//!         "position": { "line": 10, "character": 5 }
//!     }),
//! );
//! let response = provider.request(&spec).await?;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-lsp
//!
//! # 设计 (Design) — P14.5
//!
//! **目标**: 抽象 `ctx.lsp` 能力缝 (跟 dsh `ctx.lsp` 1:1 对等), 业务方
//! - spawn language server (rust-analyzer / typescript-language-server / pyright)
//! - 发 LSP 请求 (`textDocument/definition` / `references` / `hover`)
//! - 拿 LSP 响应 (JSON-RPC 2.0 result / error)
//!
//! **背景**: 见 [dsh-feature-parity-table §2] `ctx.lsp`. ma-harness 之前无 LSP 集成.
//!
//! **设计决策**: 不引 `tower-lsp` 或 `lsp-types` (重型 dep)
//! - `tower-lsp`: 完整 LSP 实现, 我们只要 client 部分 (~80% 体积没用)
//! - `lsp-types`: 类型定义, ~200KB, 但我们只要少数几个 method (definition / references / hover)
//! - **自写 JSON-RPC 2.0 消息编解码** (~150 行) + [lsp-types-or-self-defined structs](~100 行)
//! - 业务方可后续 `lsp-types` 化 (P15+)
//!
//! **核心抽象**:
//! - [`LspSpec`] — LSP 请求描述 (id / method / params)
//! - [`LspResponse`] — 响应 (id / result 或 error)
//! - [`LspError`] — IO / Parse / Server / Timeout
//! - [`LspService`] trait (request / notify / provider_name)
//! - [`LocalLspProvider`] — spawn language server + JSON-RPC over stdio
//!   (委托给 `ma-harness-subprocess` (P14.1))
//!
//! **JSON-RPC 2.0 协议** (LSP 标准):
//! - Request: `{"jsonrpc":"2.0","id":N,"method":"M","params":{...}}`
//! - Response: `{"jsonrpc":"2.0","id":N,"result":R}` 或 `"error":{"code":C,"message":M}`
//! - Notification: `{"jsonrpc":"2.0","method":"M","params":{...}}` (no id, no response)
//! - Transport: `Content-Length: N\r\n\r\n<body>` (per JSON-RPC over HTTP style)
//!
//! **6 质量属性**:
//! - 可复用: LspService trait, future MockLspProvider (测试) / RemoteLspProvider (P16+)
//! - 可维护: 模块化分块, error / spec / provider / jsonrpc 集中 lib.rs
//! - 鲁棒: 错误归一化 (IO / Parse / Server / Timeout), 边界 case 显式
//! - 安全: 不 eval response, JSON parse 严格
//! - 可测: 8+ 测试覆盖 JSON-RPC 编码 / 解码 / 错误 / Content-Length
//! - 可扩展: lsp-types 化 (P15+), mock / remote provider
//!
//! # 限制 (Limitations) — P14.5.1
//!
//! - 单 request / 同步响应 (no batch, no streaming response)
//! - 不自动 `initialize` / `initialized` LSP lifecycle (P14.5.2 业务方用 higher-level helper)
//! - `plugin-lsp` 注册多 provider 留 P14.5.2
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

// ============================================================================
// LspError: 统一的 LSP 错误
// ============================================================================

/// LSP 客户端错误.
#[derive(Debug, Error)]
pub enum LspError {
    /// IO 错误 (spawn / read / write 失败)
    #[error("LSP I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 解析错误
    #[error("LSP JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),

    /// JSON-RPC 协议错误 (缺 Content-Length 头, body 长度不匹配)
    #[error("LSP protocol error: {0}")]
    Protocol(String),

    /// LSP server 返回 error 响应
    #[error("LSP server error: code={code} message={message}")]
    Server {
        /// JSON-RPC error code
        code: i32,
        /// Error message
        message: String,
    },

    /// 超时
    #[error("LSP request timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// 业务方 method 名无效
    #[error("invalid LSP method: {0}")]
    InvalidMethod(String),
}

// ============================================================================
// LspSpec: LSP 请求描述
// ============================================================================

/// LSP request spec (id + method + params).
///
/// **JSON-RPC 2.0**:
/// - `id` 用于 request-response 配对, 业务方必须用 `LspSpec::request`
/// - `params` 是 method-specific JSON
#[derive(Debug, Clone)]
pub struct LspSpec {
    /// Request ID (唯一, 业务方用 `next_id()` 拿)
    pub id: i64,
    /// Method 名 (e.g. "textDocument/definition")
    pub method: String,
    /// Params (JSON Value)
    pub params: serde_json::Value,
    /// 超时 (None = 无限等待)
    pub timeout: Option<std::time::Duration>,
}

impl LspSpec {
    /// 创建一个 request spec (业务方一般用 `next_id()` 拿 id)
    pub fn request(id: i64, method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            id,
            method: method.into(),
            params,
            timeout: None,
        }
    }

    /// 设置超时
    pub fn with_timeout(mut self, dur: std::time::Duration) -> Self {
        self.timeout = Some(dur);
        self
    }

    /// 验证 spec (method 不能以 "$/" 开头 — 这是 server 内部用)
    pub fn validate(&self) -> Result<(), LspError> {
        if self.method.is_empty() {
            return Err(LspError::InvalidMethod("empty".into()));
        }
        if self.method.starts_with("$/") {
            return Err(LspError::InvalidMethod(format!(
                "method starts with $/ (server-internal): {}",
                self.method
            )));
        }
        Ok(())
    }
}

/// 全局 atomic counter 给 request id (业务方 `LocalLspProvider::next_id()`).
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// 拿下一个 request ID (业务方一般用)
pub fn next_id() -> i64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed) as i64
}

// ============================================================================
// LspResponse: LSP 响应 (id + result 或 error)
// ============================================================================

/// LSP response (业务方拿到的 LspSpec 对应响应).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspResponse {
    /// 对应 request id
    pub id: i64,
    /// Result (成功时, JSON Value)
    pub result: Option<serde_json::Value>,
    /// Error (失败时)
    pub error: Option<LspServerError>,
}

/// LSP server 返回的错误 (JSON-RPC error object)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspServerError {
    /// Error code (JSON-RPC 2.0 standard codes: -32700 parse, -32600 invalid request, -32601 method not found, -32602 invalid params, -32603 internal)
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional data (可选)
    pub data: Option<serde_json::Value>,
}

impl LspResponse {
    /// 是否有 result
    pub fn is_success(&self) -> bool {
        self.result.is_some() && self.error.is_none()
    }

    /// 转换为 Result<Value, LspServerError>
    pub fn into_result(self) -> Result<serde_json::Value, LspServerError> {
        if let Some(err) = self.error {
            Err(err)
        } else {
            Ok(self.result.unwrap_or(serde_json::Value::Null))
        }
    }
}

// ============================================================================
// JSON-RPC 2.0 编码 / 解码 (内部)
// ============================================================================

/// 编码 JSON-RPC message 到 `Content-Length: N\r\n\r\n<body>` 格式.
fn encode_message(body: &serde_json::Value) -> Result<Vec<u8>, LspError> {
    let body_bytes = serde_json::to_vec(body)?;
    let header = format!("Content-Length: {}\r\n\r\n", body_bytes.len());
    let mut out = header.into_bytes();
    out.extend_from_slice(&body_bytes);
    Ok(out)
}

/// 解码 `Content-Length: N\r\n\r\n<body>` 格式到 JSON body.
///
/// **简化**: 假设 stdin/stdout 只走 LSP 协议 (没有别的 Content-Type).
/// 业务方如果有混用, 用 `lsp-types` 化 (P15+).
async fn decode_message<R>(reader: &mut R) -> Result<serde_json::Value, LspError>
where
    R: tokio::io::AsyncBufRead + Unpin + Send,
{
    // 读 headers
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(LspError::Protocol("unexpected EOF in headers".into()));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            // 空行 = headers 结束
            break;
        }
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            content_length =
                Some(rest.trim().parse().map_err(|_| {
                    LspError::Protocol(format!("invalid Content-Length: {}", line))
                })?);
        }
        // 忽略别的 header (Content-Type 等, 我们只用 Content-Length)
    }
    let len =
        content_length.ok_or_else(|| LspError::Protocol("missing Content-Length header".into()))?;
    let mut body = vec![0u8; len];
    tokio::io::AsyncReadExt::read_exact(reader, &mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}

// ============================================================================
// LspService: 能力缝 trait
// ============================================================================

/// LSP 能力缝 (跟 dsh `ctx.lsp` 对等).
///
/// **核心方法**:
/// - [`request`](Self::request) — 发 request, 阻塞等 response
/// - [`notify`](Self::notify) — 发 notification, 不等响应
/// - [`provider_name`](Self::provider_name) — Provider 标识
#[async_trait]
pub trait LspService: Send + Sync + 'static {
    /// 发 LSP request, 阻塞等 response
    async fn request(&self, spec: &LspSpec) -> Result<LspResponse, LspError>;

    /// 发 LSP notification (不需 response, e.g. `initialized`, `textDocument/didOpen`)
    async fn notify(&self, method: &str, params: serde_json::Value) -> Result<(), LspError>;

    /// Provider 标识 (日志 / 调试)
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// LocalLspProvider: spawn language server + JSON-RPC over stdio
// ============================================================================

/// 本地 LSP provider (P14.5.1 主交付).
///
/// **实现**: spawn language server subprocess (e.g. `rust-analyzer`),
/// JSON-RPC 2.0 通信 over child stdin/stdout.
/// 委托给 `ma-harness-subprocess` (P14.1) 实际 spawn.
/// 串行调用: LSP server 处理 request 是 FIFO, 业务方发多 request 用 ID 配对.
pub struct LocalLspProvider {
    server_program: String,
    server_args: Vec<String>,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    stdout: Mutex<Option<BufReader<ChildStdout>>>,
    stderr: Mutex<Option<BufReader<ChildStderr>>>,
}

impl LocalLspProvider {
    /// 创建一个 LocalLspProvider (不立即 spawn, 第一次 request 时 lazy init)
    ///
    /// # Arguments
    /// - `server_program`: e.g. "rust-analyzer" / "typescript-language-server" / "pyright-langserver"
    /// - `server_args`: 启动参数 (e.g. `&["--stdio"]` for rust-analyzer)
    pub fn new(server_program: impl Into<String>, server_args: &[&str]) -> Self {
        Self {
            server_program: server_program.into(),
            server_args: server_args.iter().map(|s| s.to_string()).collect(),
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            stdout: Mutex::new(None),
            stderr: Mutex::new(None),
        }
    }

    /// Lazy init: 第一次 request 时 spawn language server
    async fn ensure_started(&self) -> Result<(), LspError> {
        if self.child.lock().await.is_some() {
            return Ok(());
        }
        // P14.5.1: 直接用 tokio Command (因为要拿 stdin/stdout handles)
        let mut cmd = std::process::Command::new(&self.server_program);
        cmd.args(&self.server_args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env_clear();
        let mut child = Command::from(cmd).spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Protocol("failed to take child stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Protocol("failed to take child stdout".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| LspError::Protocol("failed to take child stderr".into()))?;

        *self.child.lock().await = Some(child);
        *self.stdin.lock().await = Some(stdin);
        *self.stdout.lock().await = Some(BufReader::new(stdout));
        *self.stderr.lock().await = Some(BufReader::new(stderr));

        tracing::debug!(
            program = %self.server_program,
            "LSP server started"
        );
        Ok(())
    }

    /// 关闭 LSP server (业务方 finish 时调)
    pub async fn shutdown(&self) -> Result<(), LspError> {
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
            *self.stdin.lock().await = None;
            *self.stdout.lock().await = None;
            *self.stderr.lock().await = None;
        }
        Ok(())
    }
}

impl Drop for LocalLspProvider {
    fn drop(&mut self) {
        // 业务方忘 shutdown 时, 兜底 (sync context, 不能 await)
        // tokio::sync::Mutex 阻塞 lock 在 drop 里不安全, 改用 try_lock
        if let Ok(mut child_guard) = self.child.try_lock() {
            if let Some(mut child) = child_guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}

#[async_trait]
impl LspService for LocalLspProvider {
    async fn request(&self, spec: &LspSpec) -> Result<LspResponse, LspError> {
        spec.validate()?;
        self.ensure_started().await?;

        // 构造 JSON-RPC request
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": spec.id,
            "method": spec.method,
            "params": spec.params,
        });
        let bytes = encode_message(&body)?;

        // 写 stdin (拿 lock 写完释放, 不阻塞其他 request)
        {
            let mut stdin_guard = self.stdin.lock().await;
            let stdin = stdin_guard
                .as_mut()
                .ok_or_else(|| LspError::Protocol("stdin not initialized".into()))?;
            stdin.write_all(&bytes).await?;
            stdin.flush().await?;
        }

        // 读 response (可能 server 先发 notification, 跳过 notification 直接读 response)
        let response = match spec.timeout {
            Some(dur) => tokio::time::timeout(dur, self.read_response(spec.id))
                .await
                .map_err(|_| LspError::Timeout(dur))??,
            None => self.read_response(spec.id).await?,
        };
        Ok(response)
    }

    async fn notify(&self, method: &str, params: serde_json::Value) -> Result<(), LspError> {
        if method.is_empty() {
            return Err(LspError::InvalidMethod("empty".into()));
        }
        self.ensure_started().await?;

        // Notification 没 id, 不需 response
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let bytes = encode_message(&body)?;

        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard
            .as_mut()
            .ok_or_else(|| LspError::Protocol("stdin not initialized".into()))?;
        stdin.write_all(&bytes).await?;
        stdin.flush().await?;
        Ok(())
    }

    fn provider_name(&self) -> &'static str {
        "local-stdio"
    }
}

impl LocalLspProvider {
    /// 读 response (跳过 notifications, 找到 id 匹配的 response)
    async fn read_response(&self, id: i64) -> Result<LspResponse, LspError> {
        let mut stdout_guard = self.stdout.lock().await;
        let reader = stdout_guard
            .as_mut()
            .ok_or_else(|| LspError::Protocol("stdout not initialized".into()))?;

        loop {
            let body = decode_message(reader).await?;
            // 跳过 server-initiated notifications (no id, only method)
            if body.get("id").is_none() {
                tracing::debug!(
                    method = ?body.get("method"),
                    "LSP server notification, skip"
                );
                continue;
            }
            // 构造 LspResponse
            let resp_id = body
                .get("id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| LspError::Protocol("response missing 'id' field".into()))?;
            if resp_id != id {
                tracing::warn!(
                    expected = id,
                    got = resp_id,
                    "LSP response id mismatch, skip"
                );
                continue;
            }
            let result = body.get("result").cloned();
            let error = body.get("error").map(|e| LspServerError {
                code: e.get("code").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                message: e
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                data: e.get("data").cloned(),
            });
            return Ok(LspResponse {
                id: resp_id,
                result,
                error,
            });
        }
    }
}

// ============================================================================
// DefaultLspProvider: 平台默认 (P14.5.1: LocalLspProvider stub)
// ============================================================================

/// 平台默认 LSP provider (P14.5.1: LocalLspProvider, 业务方传 server program)
pub type DefaultLspProvider = LocalLspProvider;

// ============================================================================
// 单元测试 (mod tests) — 8 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn encode_message_writes_content_length_header() {
        let body = serde_json::json!({"id": 1, "method": "test"});
        let bytes = encode_message(&body).expect("encode");
        let text = std::str::from_utf8(&bytes).expect("utf8");
        assert!(text.starts_with("Content-Length: "));
        let header_end = text.find("\r\n\r\n").expect("header end");
        let content_length = text[16..header_end].parse::<usize>().expect("parse len");
        let json_body = &text[header_end + 4..];
        assert_eq!(content_length, json_body.len());
        // body 应是有效 JSON
        let parsed: serde_json::Value = serde_json::from_str(json_body).expect("json");
        assert_eq!(parsed["id"], 1);
    }

    #[tokio::test]
    async fn decode_message_parses_content_length() {
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 2, "result": "ok"});
        let encoded = encode_message(&body).expect("encode");
        let mut reader = BufReader::new(encoded.as_slice());
        let parsed = decode_message(&mut reader).await.expect("decode");
        assert_eq!(parsed["id"], 2);
        assert_eq!(parsed["result"], "ok");
    }

    #[tokio::test]
    async fn decode_message_eof_returns_protocol_error() {
        let mut reader = BufReader::new(&[][..]);
        let err = decode_message(&mut reader).await.unwrap_err();
        assert!(matches!(err, LspError::Protocol(_)));
    }

    #[tokio::test]
    async fn decode_message_missing_content_length_errors() {
        // 只有 body, 没 Content-Length 头
        let bad = b"\r\n\r\n{\"id\":1}\r\n";
        let mut reader = BufReader::new(&bad[..]);
        let err = decode_message(&mut reader).await.unwrap_err();
        assert!(matches!(err, LspError::Protocol(_)));
    }

    #[test]
    fn lsp_spec_validates_method() {
        let good = LspSpec::request(1, "textDocument/definition", serde_json::json!({}));
        good.validate().expect("valid");
        let bad = LspSpec::request(1, "", serde_json::json!({}));
        assert!(bad.validate().is_err());
        let internal = LspSpec::request(1, "$/internal", serde_json::json!({}));
        assert!(internal.validate().is_err());
    }

    #[test]
    fn lsp_response_into_result() {
        let success = LspResponse {
            id: 1,
            result: Some(serde_json::json!({"ok": true})),
            error: None,
        };
        assert!(success.is_success());
        let val = success.into_result().expect("ok");
        assert_eq!(val["ok"], true);

        let fail = LspResponse {
            id: 2,
            result: None,
            error: Some(LspServerError {
                code: -32601,
                message: "method not found".into(),
                data: None,
            }),
        };
        assert!(!fail.is_success());
        let err = fail.into_result().unwrap_err();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "method not found");
    }

    #[tokio::test]
    async fn local_provider_request_with_mock_server() {
        // 用一个 mock LSP server (Node.js script 模拟 LSP 协议)
        // 简化: 用 python 写一个 mock server
        // P14.5.1: 业务方本机没装 rust-analyzer, 用 mock 测试 request/response
        let mock_script = r#"
import sys
import json

# LSP 协议: 读 headers, 读 body, 回 response
def read_message():
    headers = {}
    while True:
        line = sys.stdin.readline()
        if not line:
            return None
        line = line.rstrip('\r\n')
        if not line:
            break
        k, _, v = line.partition(':')
        headers[k.strip()] = v.strip()
    length = int(headers['Content-Length'])
    body = sys.stdin.read(length)
    return json.loads(body)

def write_message(msg):
    body = json.dumps(msg)
    sys.stdout.write(f'Content-Length: {len(body)}\r\n\r\n{body}')
    sys.stdout.flush()

# 处理 request: 回 echo
while True:
    msg = read_message()
    if msg is None:
        break
    if 'id' in msg:
        # Request: 回 response
        resp = {
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {"echo": msg.get("params"), "method": msg.get("method")},
        }
        write_message(resp)
    else:
        # Notification: 不回
        pass
"#;
        let tmp = tempfile::tempdir().expect("tempdir");
        let mock_path = tmp.path().join("mock_lsp.py");
        std::fs::write(&mock_path, mock_script).expect("write mock");

        // 业务方本机有 python 吗? 检查
        let python_check = std::process::Command::new("python")
            .arg("--version")
            .output();
        if python_check.is_err() {
            // 没 python, skip e2e test
            eprintln!("python not found, skip mock LSP test");
            return;
        }

        let provider = LocalLspProvider::new("python", &["-u", mock_path.to_str().unwrap()]);
        let spec = LspSpec::request(
            1,
            "textDocument/definition",
            serde_json::json!({
                "textDocument": { "uri": "file:///foo.rs" },
                "position": { "line": 10, "character": 5 }
            }),
        )
        .with_timeout(std::time::Duration::from_secs(5));

        let response = provider.request(&spec).await.expect("request");
        assert!(response.is_success());
        assert_eq!(response.id, 1);
        assert_eq!(
            response.result.unwrap()["method"],
            "textDocument/definition"
        );

        provider.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn local_provider_request_with_nonexistent_server_errors() {
        let provider = LocalLspProvider::new("this-binary-does-not-exist-12345", &[]);
        let spec = LspSpec::request(1, "test", serde_json::json!({}))
            .with_timeout(std::time::Duration::from_secs(2));
        let err = provider.request(&spec).await.unwrap_err();
        // spawn 失败 → IO error
        assert!(matches!(err, LspError::Io(_)));
        provider.shutdown().await.ok();
    }

    #[tokio::test]
    async fn next_id_is_monotonic() {
        let id1 = next_id();
        let id2 = next_id();
        assert!(id2 > id1);
    }
}
