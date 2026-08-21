//! JSON-RPC 2.0 client 跟 dsh `@deepseek-ai/dsh-sdk-jsonrpc-server` 配对
//!
//! 协议: [JSON-RPC 2.0](https://www.jsonrpc.org/specification) over stdio (newline-delimited JSON)
//! 跟 LSP / ACP 同款 framing (一行一个 JSON, LF 结尾)
//!
//! **P13.1 范围**: 单 client 单 request, 不做 batch / notification / 异步 id 池
//! P13.2+ 视需要扩展 (e.g. cancellation 用 notification `$/cancelRequest`)

#![allow(dead_code)] // P13.1 留白字段 (e.g. error.data) 给 P13.2 用

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;

/// JSON-RPC 错误
#[derive(Debug, Error)]
pub enum JsonRpcError {
    /// IO 错 (pipe close / read 错 / write 错)
    #[error("JSON-RPC IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 解析错
    #[error("JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),

    /// 协议层错 (missing id / wrong shape)
    #[error("JSON-RPC protocol error: {0}")]
    Protocol(String),

    /// 客户端被 take 后再用
    #[error("JSON-RPC client state invalid: {0}")]
    Client(String),

    /// dsh server 返回的 JSON-RPC error
    #[error("dsh server error (code {code}): {message}")]
    Server {
        /// error code
        code: i64,
        /// error message
        message: String,
        /// 可选 data
        data: Option<Value>,
    },
}

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// 协议版本, 总是 "2.0"
    pub jsonrpc: String,
    /// 方法名
    pub method: String,
    /// 参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// id (P13.1 简单递增, P13.2+ 可换 UUID)
    pub id: u64,
}

impl JsonRpcRequest {
    /// 构造 request (id 自动分配)
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        // 单 client 单线程递增 (P13.1 简化, 假设串行 await, 不并发多请求)
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
            id,
        }
    }
}

/// JSON-RPC 2.0 response (成功或失败)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// 协议版本, 总是 "2.0"
    pub jsonrpc: String,
    /// 跟 request 的 id 匹配
    pub id: u64,
    /// 成功结果 (跟 error 互斥)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// 错误 (跟 result 互斥)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorBody>,
}

/// JSON-RPC error body (server 返回的)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorBody {
    /// error code (JSON-RPC 预定义 -32700..-32600 跟 server 自定义)
    pub code: i64,
    /// error message
    pub message: String,
    /// 可选 data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    /// 拿 result, 如果是 error 就转 JsonRpcError::Server
    pub fn into_result(self) -> Result<Value, JsonRpcError> {
        if let Some(err) = self.error {
            return Err(JsonRpcError::Server {
                code: err.code,
                message: err.message,
                data: err.data,
            });
        }
        self.result
            .ok_or_else(|| JsonRpcError::Protocol("response has neither result nor error".into()))
    }
}

/// JSON-RPC 2.0 client, framed over (stdin, stdout) line-delimited JSON
///
/// P13.1 单线程串行 (一个 client 一个 in-flight request), P13.2+ 才并发
pub struct JsonRpcClient {
    /// 写 (单写者, Mutex 保护)
    writer: BufWriterForStdin,
    /// 读 (单读者, Mutex 保护)
    reader: Mutex<BufReader<ChildStdout>>,
}

impl JsonRpcClient {
    /// 构造 client (从 node 子进程的 stdin/stdout 拿)
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            writer: BufWriterForStdin::new(stdin),
            reader: Mutex::new(BufReader::new(stdout)),
        }
    }

    /// 发 request + 读 response (串行, 一个 in-flight)
    pub async fn request(&mut self, req: JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError> {
        // 1. serialize request 成一行 JSON + \n
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');

        // 2. 写
        self.writer.write_line(&line).await?;

        // 3. 读一行
        let mut reader_guard = self.reader.lock().await;
        let mut buf = String::new();
        let n = reader_guard.read_line(&mut buf).await?;
        if n == 0 {
            return Err(JsonRpcError::Protocol(
                "EOF before response (subprocess closed pipe)".into(),
            ));
        }
        drop(reader_guard); // 早 drop, 允许下一个 await

        // 4. 解析
        let response: JsonRpcResponse = serde_json::from_str(&buf)?;

        // 5. 校验 id 匹配
        if response.id != req.id {
            return Err(JsonRpcError::Protocol(format!(
                "response id {} != request id {}",
                response.id, req.id
            )));
        }
        Ok(response)
    }
}

// ============================================================================
// 内部 helper: BufWriter for ChildStdin (要支持 `&mut self` 调 async write)
// ============================================================================

/// 简单的 BufWriter wrap ChildStdin (P13.1 简化, 不用 tokio::io::BufWriter 因为
/// 它要 `&mut self` 而我们用 Mutex 锁).
struct BufWriterForStdin {
    inner: Mutex<ChildStdin>,
}

impl BufWriterForStdin {
    fn new(inner: ChildStdin) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    async fn write_line(&self, line: &str) -> Result<(), std::io::Error> {
        let mut guard = self.inner.lock().await;
        guard.write_all(line.as_bytes()).await?;
        guard.flush().await?;
        Ok(())
    }
}
