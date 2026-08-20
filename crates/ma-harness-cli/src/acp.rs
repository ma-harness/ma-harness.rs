//! ACP (Agent Communication Protocol) JSON-RPC 2.0 stdio server.
//!
//! 设计: 跟 dsh `dsh-jsonrpc-agent` 互通 (Phase 1: 基础 3 方法).
//!
//! ## 协议
//!
//! JSON-RPC 2.0 over stdio (stdin/stdout) — 跟 dsh minimal agent 风格一致.
//! 每个请求一行, 响应/通知一行 (NDJSON).
//!
//! ## 支持的方法 (P11-4 v1)
//!
//! - `initialize` — 握手, 返回 protocolVersion + agentCapabilities
//! - `newSession` — 建新 session, 返回 sessionId
//! - `prompt` — 跑 agent, 走 session/update 通知流, 返回 stopReason
//!
//! ## 不支持 (P11-4 v2+)
//!
//! - `loadSession` — 加载历史 session
//! - `cancel` — 取消 prompt
//! - 复杂 content blocks (image / audio / file refs)
//! - MCP server 配置
//!
//! ## 用法
//!
//! ```bash
//! # 启 server (stdio)
//! mah acp serve --model stub
//!
//! # Python 业务方 (跟 dsh SDK 风格一致, 但走 JSON-RPC stdio 直接)
//! # 写 JSON-RPC 到 stdin, 从 stdout 读响应
//! ```

use std::sync::Arc;

use anyhow::{Context, Result};
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

/// ACP protocol version (跟 dsh 对齐 v1)
pub const PROTOCOL_VERSION: u32 = 1;

/// JSON-RPC 2.0 request (method + params + id)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response (result OR error)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }
    pub fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// JSON-RPC 2.0 notification (method + params, no id, no response expected)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

/// ACP method handlers.
pub struct AcpHandler {
    /// Agent loop (本地, in-process)
    pub agent: Arc<AgentLoop>,
    /// Active sessions (P12-6 v2: 跟 loadSession / cancel 用)
    pub sessions: std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, SessionInfo>>>,
    /// Cancellation flags (P12-6 v2: cancel 走)
    pub cancel_flags: std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, bool>>>,
}

/// Session metadata (P12-6 v2)
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: std::time::SystemTime,
    pub cwd: Option<String>,
    pub message_count: u32,
}

impl AcpHandler {
    pub fn new(agent: Arc<AgentLoop>) -> Self {
        Self {
            agent,
            sessions: std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new())),
            cancel_flags: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::BTreeMap::new(),
            )),
        }
    }

    /// Handle `initialize` — 握手, 返回 protocol version + capabilities
    pub async fn handle_initialize(&self, _params: Option<Value>) -> Result<Value> {
        // P12-6 v2: loadSession + image 能力都开
        Ok(serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "agentCapabilities": {
                "loadSession": true,
                "promptCapabilities": {
                    "image": true,
                    "audio": false,
                },
            },
            "agentInfo": {
                "name": "ma-harness",
                "version": env!("CARGO_PKG_VERSION"),
                "title": "ma-harness.rs (Rust AI agent orchestrator)",
            },
        }))
    }

    /// Handle `newSession` — 建新 session, 返回 session id
    pub async fn handle_new_session(&self, params: Option<Value>) -> Result<Value> {
        let session_id = Uuid::new_v4().to_string();
        let cwd = params
            .as_ref()
            .and_then(|p| p.get("cwd"))
            .and_then(|v| v.as_str())
            .map(String::from);

        // P12-6 v2: 跟踪 session metadata
        self.sessions
            .lock()
            .expect("sessions lock poisoned")
            .insert(
                session_id.clone(),
                SessionInfo {
                    id: session_id.clone(),
                    created_at: std::time::SystemTime::now(),
                    cwd,
                    message_count: 0,
                },
            );
        self.cancel_flags
            .lock()
            .expect("cancel lock poisoned")
            .insert(session_id.clone(), false);

        eprintln!("[acp] newSession: {session_id}");
        Ok(serde_json::json!({
            "sessionId": session_id,
        }))
    }

    /// **P12-6 v2**: Handle `loadSession` — 加载历史 session
    pub async fn handle_load_session(&self, params: Option<Value>) -> Result<Value> {
        let session_id = params
            .as_ref()
            .and_then(|p| p.get("sessionId"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing sessionId"))?
            .to_string();

        let sessions = self.sessions.lock().expect("sessions lock poisoned");
        let info = sessions
            .get(&session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?
            .clone();

        Ok(serde_json::json!({
            "sessionId": info.id,
            "createdAt": info
                .created_at
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "cwd": info.cwd,
            "messageCount": info.message_count,
        }))
    }

    /// **P12-6 v2**: Handle `cancel` — 取消当前 prompt
    pub async fn handle_cancel(&self, params: Option<Value>) -> Result<Value> {
        let session_id = params
            .as_ref()
            .and_then(|p| p.get("sessionId"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing sessionId"))?
            .to_string();

        // 设置 cancel flag
        let mut flags = self.cancel_flags.lock().expect("cancel lock poisoned");
        flags.insert(session_id.clone(), true);
        eprintln!("[acp] cancel: session={session_id}");

        Ok(serde_json::json!({
            "cancelled": true,
            "sessionId": session_id,
        }))
    }

    /// Handle `prompt` — 跑 agent, 发 session/update 通知, 返回 stopReason
    pub async fn handle_prompt(
        &self,
        params: Option<Value>,
        notifier: impl Fn(&str, Value) + Send + Sync,
    ) -> Result<Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("missing params"))?;
        let session_id = params
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing sessionId"))?
            .to_string();
        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("missing prompt"))?;

        // P12-6 v2: 收集 prompt 内容 (text + image)
        let mut user_message = String::new();
        let mut image_count = 0;
        for block in prompt {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        if !user_message.is_empty() {
                            user_message.push('\n');
                        }
                        user_message.push_str(text);
                    }
                }
                Some("image") => {
                    image_count += 1;
                    // v1: image 计数, 实际不传给 stub model
                    // v2: 走 vision model
                }
                _ => {}
            }
        }
        if user_message.is_empty() && image_count == 0 {
            return Err(anyhow::anyhow!("no text or image block in prompt"));
        }

        eprintln!(
            "[acp] prompt: session={session_id} text={} chars + {image_count} images",
            user_message.len()
        );

        // 重置 cancel flag
        self.cancel_flags
            .lock()
            .expect("cancel lock poisoned")
            .insert(session_id.clone(), false);

        // 跑 agent (in-process)
        let req = AgentRunRequest {
            session_id: session_id.clone(),
            user_message: if image_count > 0 {
                format!("{user_message} [+{image_count} image(s)]")
            } else {
                user_message.clone()
            },
            model: "stub".to_string(),
            temperature: 0.7,
            max_tokens: 1024,
            system_prompt: None,
        };
        let resp = self.agent.run(req).await?;

        // P12-6 v2: 跟踪 session message count
        let mut sessions = self.sessions.lock().expect("sessions lock poisoned");
        if let Some(info) = sessions.get_mut(&session_id) {
            info.message_count += 1;
        }

        // 检查 cancel flag (P12-6 v2: 业务方 cancel 后, 跑完但返 cancelled)
        let cancelled = self
            .cancel_flags
            .lock()
            .expect("cancel lock poisoned")
            .get(&session_id)
            .copied()
            .unwrap_or(false);

        // 发 session/update 通知 (text chunk)
        notifier(
            "session/update",
            serde_json::json!({
                "sessionId": session_id,
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": resp.model_response.content.clone(),
                    },
                },
            }),
        );

        // P12-6 v2: cancelled → "cancelled" stopReason
        let stop_reason = if cancelled { "cancelled" } else { "end_turn" };
        Ok(serde_json::json!({
            "stopReason": stop_reason,
        }))
    }

    /// 路由 JSON-RPC request 到对应 handler
    pub async fn route(
        &self,
        req: JsonRpcRequest,
        notifier: impl Fn(&str, Value) + Send + Sync,
    ) -> Option<JsonRpcResponse> {
        let id = req.id.clone().unwrap_or(Value::Null);
        let result = match req.method.as_str() {
            "initialize" => self.handle_initialize(req.params).await,
            "newSession" => self.handle_new_session(req.params).await,
            "loadSession" => self.handle_load_session(req.params).await,
            "cancel" => self.handle_cancel(req.params).await,
            "prompt" => self.handle_prompt(req.params, notifier).await,
            method => {
                return Some(JsonRpcResponse::error(
                    id,
                    -32601, // Method not found
                    format!("method not found: {method}"),
                ));
            }
        };
        match result {
            Ok(value) => Some(JsonRpcResponse::success(id, value)),
            Err(e) => Some(JsonRpcResponse::error(id, -32603, format!("{e}"))),
        }
    }
}

/// 启 ACP server (stdio, 阻塞直到 stdin EOF)
pub async fn run_acp_server(model: &str) -> Result<()> {
    eprintln!("[acp] starting server, model={model}, protocol={PROTOCOL_VERSION}");

    // 跟 `mah run` 一样, in-memory EventLog + AgentLoop
    let log = EventLog::open_in_memory().context("open in-memory event log")?;
    let agent = Arc::new(AgentLoop::new(log, Arc::new(StubModelAdapter)));

    let handler = AcpHandler::new(agent);

    // notifier closure (用 channel 异步写 stdout)
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let tx_clone = tx.clone();
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if let Err(e) = out.write_all(line.as_bytes()).await {
                eprintln!("[acp] stdout write failed: {e}");
                break;
            }
            if let Err(e) = out.write_all(b"\n").await {
                eprintln!("[acp] stdout newline failed: {e}");
                break;
            }
            if let Err(e) = out.flush().await {
                eprintln!("[acp] stdout flush failed: {e}");
                break;
            }
        }
    });

    let notifier = move |method: &str, params: Value| {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };
        if let Ok(line) = serde_json::to_string(&notif) {
            let _ = tx.send(line);
        }
    };

    // stdin 读 (一行一 request)
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                eprintln!("[acp] stdin EOF, shutting down");
                break;
            }
            Ok(_n) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // 解析 JSON-RPC request
                let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
                    Ok(r) => r,
                    Err(e) => {
                        // Invalid JSON → error response (id=null)
                        let resp = JsonRpcResponse::error(
                            Value::Null,
                            -32700, // Parse error
                            format!("parse error: {e}"),
                        );
                        if let Ok(line) = serde_json::to_string(&resp) {
                            let _ = tx_clone.send(line);
                        }
                        continue;
                    }
                };

                // 路由 (notification 走 method 但无 id, 不返回)
                if req.id.is_none() {
                    // 通知 (不返回 response)
                    eprintln!("[acp] notification (no response): method={}", req.method);
                    continue;
                }

                let resp = handler.route(req, &notifier).await;
                if let Some(r) = resp {
                    if let Ok(line) = serde_json::to_string(&r) {
                        let _ = tx_clone.send(line);
                    }
                }
            }
            Err(e) => {
                eprintln!("[acp] stdin read error: {e}");
                break;
            }
        }
    }

    // 关闭所有 tx (notifier + tx_clone), writer 收到 None 退出
    drop(notifier);
    drop(tx_clone);
    let _ = writer.await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonrpc_response_success() {
        let r = JsonRpcResponse::success(serde_json::json!(1), serde_json::json!({"ok": true}));
        assert_eq!(r.jsonrpc, "2.0");
        assert_eq!(r.id, serde_json::json!(1));
        assert!(r.result.is_some());
        assert!(r.error.is_none());
    }

    #[test]
    fn jsonrpc_response_error() {
        let r = JsonRpcResponse::error(serde_json::json!(1), -32601, "method not found");
        assert_eq!(r.error.as_ref().unwrap().code, -32601);
        assert_eq!(r.error.as_ref().unwrap().message, "method not found");
    }

    #[test]
    fn jsonrpc_request_parse() {
        let raw =
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(serde_json::json!(1)));
        assert!(req.params.is_some());
    }

    #[test]
    fn jsonrpc_notification_parse() {
        let raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"x":1}}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw).unwrap();
        assert!(req.id.is_none());
        assert_eq!(req.method, "session/update");
    }
}
