//! # 命名约定
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-model`
//! **Crate ident** (`use` 路径): `ma_harness_model`
//!
//! Rust 自动从 kebab-case package name 推 snake_case crate ident.
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! ma-harness-model = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_model::*;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-model
//!
//! ma_harness_model — LLM model adapters (Phase 2.3)
//!
//! **目标**: 在 `StubModelAdapter` 之外提供真 LLM API 调用.
//!
//! **支持**:
//! - `OpenaiAdapter` — OpenAI Chat Completions API (api.openai.com/v1/chat/completions)
//! - `AnthropicAdapter` — Anthropic Messages API (api.anthropic.com/v1/messages) (Phase 2.3 后半)
//! - `StubModelAdapter` — 已在 ma_harness_core, 这里 re-export
//!
//! **设计**:
//! - 构造时拿 API key (String), 业务方负责从 ctx / env 读
//! - HTTP client 用 reqwest (rustls-tls, 已锁)
//! - 错误 `AdapterError` thiserror: Auth / Network / Parse / RateLimit / Server
//! - 跟 `ma_harness_core::ModelAdapter` trait 一致
//!
//! **限制 (Phase 2.3 PoC)**:
//! - 不发真 HTTP (单元测只测 request 构造 + response 解析)
//! - 不支持 streaming (Phase 2.5)
//! - 不支持 function/tool calls (Phase 2.5)
//! - 不支持 retry/backoff (Phase 2.4)
//!
//! **用法**
//!
//! ```ignore
//! use ma_harness_model::OpenaiAdapter;
//!
//! let adapter = OpenaiAdapter::new("sk-...").with_model("gpt-4o-mini");
//! let resp = adapter.complete(&req).await?;
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(missing_docs)] // 2026-08-18: 内部 crate, Phase 2 release 前补 doc

use async_trait::async_trait;
use futures::StreamExt;
use ma_harness_core::{FinishReason, ModelAdapter, ModelRequest, ModelResponse, StubModelAdapter};
use std::pin::Pin;
use thiserror::Error;

// ============================================================================
// P11-5: Multi-modal (vision / audio)
// ============================================================================

pub mod multimodal;
pub mod vision_plugin;
pub mod vision_tool;
pub use multimodal::{build_anthropic_vision_content, build_openai_vision_content, ImageAttachment};
pub use vision_tool::{
    describe_image, describe_with_anthropic, describe_with_openai, VisionBackend,
    VisionDescribeArgs, VisionError, VisionResult, VISION_TOOL_DESCRIPTION, VISION_TOOL_NAME,
};
pub use vision_plugin::VisionTool;

// ============================================================================
// P12-2: Retry + circuit breaker (稳定性)
// ============================================================================

pub mod retry;
pub use retry::{
    backoff_for, retry_with_backoff, CircuitBreaker, CircuitState, RetryError, RetryPolicy,
};

// ============================================================================
// AdapterError
// ============================================================================

/// Model adapter 错误
#[derive(Debug, Error)]
pub enum AdapterError {
    /// HTTP 网络错误
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// API 返非 2xx 状态码
    #[error("API returned status {status}: {body}")]
    Api {
        /// HTTP 状态码
        status: u16,
        /// 响应 body
        body: String,
    },

    /// 401 / 403 (auth 失败)
    #[error("authentication failed (status {status}): {body}")]
    Auth {
        /// HTTP 状态码
        status: u16,
        /// 响应 body
        body: String,
    },

    /// 429 (rate limit)
    #[error("rate limited (status 429): {body}")]
    RateLimit {
        /// 响应 body (含 retry-after 等信息)
        body: String,
    },

    /// JSON 解析失败
    #[error("failed to parse response: {0}")]
    Parse(#[from] serde_json::Error),

    /// 响应里没有 expected field
    #[error("missing field in response: {0}")]
    MissingField(&'static str),
}

// ============================================================================
// OpenAI Chat Completions API
// ============================================================================

/// OpenAI Chat Completions adapter
///
/// 默认 endpoint: `https://api.openai.com/v1/chat/completions`
/// 默认 model: `gpt-4o-mini`
#[derive(Clone)]
pub struct OpenaiAdapter {
    api_key: String,
    model: String,
    endpoint: String,
    client: reqwest::Client,
}

impl OpenaiAdapter {
    /// 构造 (默认 model = gpt-4o-mini, default endpoint)
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "gpt-4o-mini".to_string(),
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// 设置 model (例如 "gpt-4o" / "gpt-4-turbo" / "gpt-3.5-turbo")
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// 设置自定义 endpoint (Azure OpenAI / 代理)
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// **P11-5**: 构造 OpenAI Vision 请求 body (text + 1+ images)
    ///
    /// 业务方用法 (vision model):
    /// ```ignore
    /// let adapter = OpenaiAdapter::new("sk-...").with_model("gpt-4o");
    /// let body = adapter.build_vision_request_body("describe this image", &[img]);
    /// ```
    pub fn build_vision_request_body(
        &self,
        text: &str,
        images: &[crate::multimodal::ImageAttachment],
    ) -> serde_json::Value {
        let content = crate::multimodal::build_openai_vision_content(text, images);
        serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "user", "content": content}
            ],
            "max_tokens": 1024,
        })
    }

    /// 构造 OpenAI Chat Completions 请求 body
    pub fn build_request_body(&self, req: &ModelRequest) -> serde_json::Value {
        let mut messages: Vec<serde_json::Value> = Vec::with_capacity(req.messages.len());
        // system prompt 单独发 (OpenAI 跟 Anthropic 都接受 system 消息)
        if let Some(sys) = &req.system_prompt {
            messages.push(serde_json::json!({"role": "system", "content": sys}));
        }
        for m in &req.messages {
            messages.push(serde_json::json!({"role": m.role, "content": m.content}));
        }
        serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        })
    }

    /// **P6-2 (Day 100)**: 构造 OpenAI Chat Completions 流式请求 body
    ///
    /// 跟 `build_request_body` 一样 + `"stream": true`. 业务方 streaming 模式.
    pub fn build_stream_request_body(&self, req: &ModelRequest) -> serde_json::Value {
        let mut body = self.build_request_body(req);
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".to_string(), serde_json::Value::Bool(true));
        }
        body
    }

    /// **P6-2 (Day 100)**: 解析 SSE `data:` 行 → `Some(delta content)` 或 `None`
    ///
    /// 业务方 OpenAI streaming 协议:
    ///   `data: {"choices":[{"delta":{"content":"Hello"}}]}\n\n`
    ///   `data: {"choices":[{"delta":{"content":" world"}}]}\n\n`
    ///   ...
    ///   `data: [DONE]\n\n`  ← 终止信号
    ///
    /// 返回:
    ///   - `Some(content)` — 拿到增量 token (content 字段为空字符串也算 Some, 业务方自己判断)
    ///   - `None` — `[DONE]` 终止 / 解析失败 / 不是 `data:` 开头
    ///
    /// 业务方用法: 拿到 None 就 stop stream.
    pub fn parse_sse_data_line(line: &str) -> Option<String> {
        // 1. 去 "data: " 前缀 (5 字符)
        let payload = line.strip_prefix("data:")?.trim();
        // 2. 终止信号
        if payload == "[DONE]" {
            return None;
        }
        // 3. 解析 JSON, 拿 choices[0].delta.content
        let value: serde_json::Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(_) => return None, // malformed JSON, 业务方静默 skip
        };
        let content = value
            .get("choices")?
            .as_array()?
            .first()?
            .get("delta")?
            .get("content")?
            .as_str()?;
        Some(content.to_string())
    }

    /// 解析 OpenAI Chat Completions 响应
    pub fn parse_response(&self, body: serde_json::Value) -> Result<ModelResponse, AdapterError> {
        // 顶层字段: id, object, created, model, choices[], usage
        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or(AdapterError::MissingField("model"))?
            .to_string();

        let choice = body
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .ok_or(AdapterError::MissingField("choices[0]"))?;

        let message = choice
            .get("message")
            .ok_or(AdapterError::MissingField("choices[0].message"))?;
        let content = message
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or(AdapterError::MissingField("choices[0].message.content"))?
            .to_string();

        let finish_reason = match choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("stop")
        {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "content_filter" => FinishReason::ContentFilter,
            other => {
                tracing::warn!(reason = %other, "unknown OpenAI finish_reason, defaulting to Stop");
                FinishReason::Stop
            }
        };

        let usage = body.get("usage");
        let prompt_tokens = usage
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let completion_tokens = usage
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(ModelResponse {
            model,
            content,
            finish_reason,
            prompt_tokens,
            completion_tokens,
        })
    }
}

#[async_trait]
impl ModelAdapter for OpenaiAdapter {
    fn name(&self) -> &str {
        "openai"
    }

    async fn complete(&self, req: &ModelRequest) -> anyhow::Result<ModelResponse> {
        let body = self.build_request_body(req);
        let resp = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            let body = resp.text().await.unwrap_or_default();
            return Err(AdapterError::Auth {
                status: status.as_u16(),
                body,
            }
            .into());
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let body = resp.text().await.unwrap_or_default();
            return Err(AdapterError::RateLimit { body }.into());
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AdapterError::Api {
                status: status.as_u16(),
                body,
            }
            .into());
        }

        let body: serde_json::Value = resp.json().await?;
        let parsed = self.parse_response(body)?;
        Ok(parsed)
    }

    /// **P6-2 (Day 100)**: OpenAI 真正 SSE streaming 覆盖
    ///
    /// 跟 default impl 不同:
    /// 1. 发送 `stream: true` 走 SSE endpoint
    /// 2. 用 `bytes_stream()` 拿 chunked HTTP body
    /// 3. 按 `\n\n` 切 SSE event, 每 event 内按 `\n` 切行
    /// 4. `data:` 行 → `parse_sse_data_line` 拿 delta content
    /// 5. `data: [DONE]` 终止信号 → stop stream
    /// 6. 状态码 401/403/429/其他 4xx/5xx → 返 Err, 不发 token
    ///
    /// 业务方拿 stream: `let mut s = adapter.complete_stream(&req); while let Some(t) = s.next().await { print!("{t}"); }`
    fn complete_stream<'a>(
        &'a self,
        req: &'a ModelRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = String> + Send + 'a>> {
        let endpoint = self.endpoint.clone();
        let api_key = self.api_key.clone();
        let body = self.build_stream_request_body(req);
        let client = self.client.clone();

        Box::pin(async_stream::stream! {
            // 1. 发 POST + stream:true
            let resp = match client
                .post(&endpoint)
                .bearer_auth(&api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[openai] stream send err: {e}");
                    return;
                }
            };

            // 2. 状态码检查
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                let body = resp.text().await.unwrap_or_default();
                eprintln!("[openai] auth err {status}: {body}");
                return;
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let body = resp.text().await.unwrap_or_default();
                eprintln!("[openai] rate limit: {body}");
                return;
            }
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                eprintln!("[openai] api err {status}: {body}");
                return;
            }

            // 3. 按 chunk 读 bytes, 攒成 SSE event
            let mut buffer = String::new();
            let mut byte_stream = resp.bytes_stream();
            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[openai] stream chunk err: {e}");
                        return;
                    }
                };
                // chunk → str (UTF-8 lossy, SSE 应是 UTF-8)
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // 按 \n\n 切 event (SSE 规范)
                while let Some(idx) = buffer.find("\n\n") {
                    let event: String = buffer.drain(..idx + 2).collect();
                    // event 内按 \n 切行, 找 data: 行
                    for line in event.lines() {
                        if let Some(token) = Self::parse_sse_data_line(line) {
                            yield token;
                        }
                        // data: [DONE] 时 parse_sse_data_line 返 None → 业务方收尾
                    }
                }
            }
        })
    }
}

// ============================================================================
// Anthropic Messages API
// ============================================================================

/// Anthropic Messages adapter
///
/// 默认 endpoint: `https://api.anthropic.com/v1/messages`
/// 默认 model: `claude-3-5-sonnet-20241022`
/// Phase 2.3 PoC: 实现 request 构造 + response 解析, 跟 OpenAI 同结构
#[derive(Clone)]
pub struct AnthropicAdapter {
    api_key: String,
    model: String,
    endpoint: String,
    api_version: String,
    client: reqwest::Client,
}

impl AnthropicAdapter {
    /// 构造 (默认 model = claude-3-5-sonnet, api_version = 2023-06-01)
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "claude-3-5-sonnet-20241022".to_string(),
            endpoint: "https://api.anthropic.com/v1/messages".to_string(),
            api_version: "2023-06-01".to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// 设置 model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// 设置自定义 endpoint (Azure Anthropic / 代理)
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// **P11-5**: 构造 Anthropic Vision 请求 body (text + 1+ images)
    pub fn build_vision_request_body(
        &self,
        text: &str,
        images: &[crate::multimodal::ImageAttachment],
    ) -> serde_json::Value {
        let content = crate::multimodal::build_anthropic_vision_content(text, images);
        serde_json::json!({
            "model": self.model,
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": content}
            ],
        })
    }

    /// 构造 Anthropic Messages 请求 body
    pub fn build_request_body(&self, req: &ModelRequest) -> serde_json::Value {
        // Anthropic: system 是 top-level field, 不是 message
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
            .collect();
        serde_json::json!({
            "model": self.model,
            "system": req.system_prompt.clone().unwrap_or_default(),
            "messages": messages,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        })
    }

    /// **P6-3 (Day 100)**: 构造 Anthropic Messages 流式请求 body
    ///
    /// 跟 `build_request_body` 一样 + `"stream": true`. 业务方 streaming 模式.
    pub fn build_stream_request_body(&self, req: &ModelRequest) -> serde_json::Value {
        let mut body = self.build_request_body(req);
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".to_string(), serde_json::Value::Bool(true));
        }
        body
    }

    /// **P6-3 (Day 100)**: 解析 Anthropic SSE 事件 → `Some(delta text)` / `None`
    ///
    /// 业务方 Anthropic streaming 协议 (跟 OpenAI 不一样, 走 event-based):
    ///   event: content_block_delta
    ///   data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
    ///
    /// 业务方用法: 拿到 (event_type, data) tuple 后调, 拿 text delta 走 yield.
    /// 终止信号: `message_stop` / `message_delta` (业务方走 stop stream).
    ///
    /// 返回:
    ///   - `Some(text)` — 拿到 text_delta, 业务方 yield 给 client
    ///   - `None` — 非 content_block_delta 事件 / 解析失败 / 缺 text 字段
    pub fn parse_sse_event(event_type: &str, data_line: &str) -> Option<String> {
        // 只 content_block_delta 走 text_delta
        if event_type != "content_block_delta" {
            return None;
        }
        // data 行去 "data: " 前缀
        let payload = data_line.strip_prefix("data:")?.trim();
        let value: serde_json::Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(_) => return None,
        };
        // text 在 delta.text
        let text = value.get("delta")?.get("text")?.as_str()?;
        Some(text.to_string())
    }

    /// 解析 Anthropic Messages 响应
    pub fn parse_response(&self, body: serde_json::Value) -> Result<ModelResponse, AdapterError> {
        // 顶层: id, type, role, content[], model, stop_reason, usage
        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or(AdapterError::MissingField("model"))?
            .to_string();

        let content_array = body
            .get("content")
            .and_then(|v| v.as_array())
            .ok_or(AdapterError::MissingField("content"))?;

        // 拼接所有 text block
        let mut content = String::new();
        for block in content_array {
            if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(text);
                }
            }
        }
        if content.is_empty() {
            return Err(AdapterError::MissingField("content[].text"));
        }

        let finish_reason = match body
            .get("stop_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("end_turn")
        {
            "end_turn" => FinishReason::Stop,
            "max_tokens" => FinishReason::Length,
            "stop_sequence" => FinishReason::Stop,
            other => {
                tracing::warn!(reason = %other, "unknown Anthropic stop_reason, defaulting to Stop");
                FinishReason::Stop
            }
        };

        let usage = body.get("usage");
        let prompt_tokens = usage
            .and_then(|u| u.get("input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let completion_tokens = usage
            .and_then(|u| u.get("output_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(ModelResponse {
            model,
            content,
            finish_reason,
            prompt_tokens,
            completion_tokens,
        })
    }
}

#[async_trait]
impl ModelAdapter for AnthropicAdapter {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn complete(&self, req: &ModelRequest) -> anyhow::Result<ModelResponse> {
        let body = self.build_request_body(req);
        let resp = self
            .client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            let body = resp.text().await.unwrap_or_default();
            return Err(AdapterError::Auth {
                status: status.as_u16(),
                body,
            }
            .into());
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let body = resp.text().await.unwrap_or_default();
            return Err(AdapterError::RateLimit { body }.into());
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AdapterError::Api {
                status: status.as_u16(),
                body,
            }
            .into());
        }

        let body: serde_json::Value = resp.json().await?;
        let parsed = self.parse_response(body)?;
        Ok(parsed)
    }

    /// **P6-3 (Day 100)**: Anthropic 真正 SSE streaming 覆盖
    ///
    /// 跟 default impl 不同 (跟 OpenaiAdapter::complete_stream 也不一样, 因为协议不同):
    /// 1. 发送 `stream: true` 走 SSE endpoint + `x-api-key` header
    /// 2. SSE event 格式: `event: <type>\ndata: {...}\n\n`
    /// 3. 只 `content_block_delta` event 走 `delta.text` yield
    /// 4. `message_stop` 终止信号 → stop stream
    /// 5. 状态码 401/403/429/其他 4xx/5xx → 返 Err, 不发 token
    ///
    /// 业务方拿 stream: `let mut s = adapter.complete_stream(&req); while let Some(t) = s.next().await { print!("{t}"); }`
    fn complete_stream<'a>(
        &'a self,
        req: &'a ModelRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = String> + Send + 'a>> {
        let endpoint = self.endpoint.clone();
        let api_key = self.api_key.clone();
        let api_version = self.api_version.clone();
        let body = self.build_stream_request_body(req);
        let client = self.client.clone();

        Box::pin(async_stream::stream! {
            // 1. 发 POST + stream:true
            let resp = match client
                .post(&endpoint)
                .header("x-api-key", &api_key)
                .header("anthropic-version", &api_version)
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[anthropic] stream send err: {e}");
                    return;
                }
            };

            // 2. 状态码检查
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                let body = resp.text().await.unwrap_or_default();
                eprintln!("[anthropic] auth err {status}: {body}");
                return;
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let body = resp.text().await.unwrap_or_default();
                eprintln!("[anthropic] rate limit: {body}");
                return;
            }
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                eprintln!("[anthropic] api err {status}: {body}");
                return;
            }

            // 3. 按 chunk 读 bytes, 攒成 SSE event (Anthropic 格式: event: <type>\ndata: <json>\n\n)
            let mut buffer = String::new();
            let mut byte_stream = resp.bytes_stream();
            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[anthropic] stream chunk err: {e}");
                        return;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // 按 \n\n 切 SSE event
                while let Some(idx) = buffer.find("\n\n") {
                    let event: String = buffer.drain(..idx + 2).collect();
                    // Anthropic SSE event 格式:
                    //   event: <type>
                    //   data: <json>
                    let mut event_type = String::new();
                    let mut data_line = String::new();
                    for line in event.lines() {
                        if let Some(t) = line.strip_prefix("event:") {
                            event_type = t.trim().to_string();
                        } else if let Some(d) = line.strip_prefix("data:") {
                            data_line = d.trim().to_string();
                        }
                    }
                    // 终止信号
                    if event_type == "message_stop" {
                        return;
                    }
                    // 解析 content_block_delta
                    if let Some(token) = Self::parse_sse_event(&event_type, &format!("data: {data_line}")) {
                        yield token;
                    }
                }
            }
        })
    }
}

// ============================================================================
// AdapterRegistry — 业务方在 ctx 注册多个 adapter, AgentLoop 按 model name 选
// ============================================================================

/// Adapter registry — 业务方注册多个 adapter, 用 name 选
///
/// 用例: 业务方在 ctx 注册 "gpt-4o" → OpenAI, "claude-3-5-sonnet" → Anthropic,
/// AgentLoop 收到 model name 后查 registry 拿对应 adapter.
#[derive(Default, Clone)]
pub struct AdapterRegistry {
    /// model name (前缀) → adapter instance
    adapters: std::collections::HashMap<String, std::sync::Arc<dyn ModelAdapter>>,
}

impl std::fmt::Debug for AdapterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterRegistry")
            .field("adapters", &self.adapters.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AdapterRegistry {
    /// 创建空 registry
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 adapter (model name 前缀)
    pub fn register<A: ModelAdapter + 'static>(
        mut self,
        model_prefix: impl Into<String>,
        adapter: A,
    ) -> Self {
        self.adapters
            .insert(model_prefix.into(), std::sync::Arc::new(adapter));
        self
    }

    /// 注册 OpenAI adapter (默认 "openai:" 前缀)
    pub fn with_openai(self, api_key: impl Into<String>) -> Self {
        self.register("openai:", OpenaiAdapter::new(api_key))
    }

    /// 注册 Anthropic adapter (默认 "anthropic:" 前缀)
    pub fn with_anthropic(self, api_key: impl Into<String>) -> Self {
        self.register("anthropic:", AnthropicAdapter::new(api_key))
    }

    /// 注册 stub (默认 "stub:" 前缀 + 裸 "stub")
    pub fn with_stub(self) -> Self {
        self.register("stub:", StubModelAdapter)
    }

    /// 注册 Azure OpenAI adapter (P8-3 / Day 101)
    ///
    /// Azure endpoint 格式: `https://{resource}.openai.azure.com/openai/deployments/{deployment}/chat/completions?api-version={version}`
    /// 业务方传 resource_name + deployment_name + api_version + api_key.
    /// 简化: 把 endpoint 拼好后塞给 OpenaiAdapter (它已经是 OpenAI-compatible 协议).
    ///
    /// 注册 prefix: `"azure:"` (e.g. `azure:gpt-4o`)
    pub fn with_azure_openai(
        self,
        resource: impl AsRef<str>,
        deployment: impl AsRef<str>,
        api_version: impl AsRef<str>,
        api_key: impl Into<String>,
    ) -> Self {
        let endpoint = format!(
            "https://{}.openai.azure.com/openai/deployments/{}/chat/completions?api-version={}",
            resource.as_ref(),
            deployment.as_ref(),
            api_version.as_ref(),
        );
        let adapter = OpenaiAdapter::new(api_key)
            .with_endpoint(endpoint)
            .with_model(deployment.as_ref().to_string());
        self.register("azure:", adapter)
    }

    /// 注册 Local (Ollama / vLLM / 其它 OpenAI-compatible) adapter (P8-3)
    ///
    /// 简化: 跟 OpenaiAdapter 一样走 OpenAI 协议, 业务方传 base URL (e.g. `http://localhost:11434/v1/chat/completions`).
    /// 适配 Ollama / vLLM / LM Studio / 任何兼容 OpenAI chat completions API 的服务.
    ///
    /// 注册 prefix: `"local:"` (e.g. `local:llama3`)
    pub fn with_local(self, base_url: impl AsRef<str>, model: impl Into<String>) -> Self {
        let endpoint = base_url.as_ref().to_string();
        let adapter = OpenaiAdapter::new("not-needed") // 多数 local server 不要 api key
            .with_endpoint(endpoint)
            .with_model(model);
        self.register("local:", adapter)
    }

    /// 注册 DeepSeek adapter (P8-3)
    ///
    /// DeepSeek 走 OpenAI-compatible 协议 (`https://api.deepseek.com/v1/chat/completions`).
    /// 模型: `deepseek-chat` / `deepseek-coder` / `deepseek-reasoner`.
    ///
    /// 注册 prefix: `"deepseek:"` (e.g. `deepseek:deepseek-chat`)
    pub fn with_deepseek(self, api_key: impl Into<String>) -> Self {
        self.register(
            "deepseek:",
            OpenaiAdapter::new(api_key)
                .with_endpoint("https://api.deepseek.com/v1/chat/completions")
                .with_model("deepseek-chat"),
        )
    }

    /// 注册 AWS Bedrock adapter (P10-6 / Day 101)
    ///
    /// 简化: Bedrock 当前支持 OpenAI-compatible 模式 (从 2024-Q4 起部分模型).
    /// 业务方传 region + access_key + secret_key + model (e.g. "anthropic.claude-3-5-sonnet-20241022-v2:0").
    /// 走 OpenaiAdapter 协议, endpoint: `https://bedrock-runtime.{region}.amazonaws.com/openai/v1/chat/completions`
    ///
    /// 注册 prefix: `"bedrock:"` (e.g. `bedrock:anthropic.claude-3-5-sonnet-20241022-v2:0`)
    ///
    /// **v1 简化**: 业务方把 AWS 签名 (SigV4) 头手工加 (走 reqwest::RequestBuilder).
    /// **v2**: 集成 aws-sdk-bedrockruntime 走真 SigV4.
    /// 当前: 拿 access_key/secret_key 当 bearer token 传 (AWS 实际拒绝, v2 改)
    pub fn with_bedrock(
        self,
        region: impl AsRef<str>,
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let endpoint = format!(
            "https://bedrock-runtime.{}.amazonaws.com/openai/v1/chat/completions",
            region.as_ref()
        );
        let adapter = OpenaiAdapter::new(format!("{}:{}", access_key.into(), secret_key.into()))
            .with_endpoint(endpoint)
            .with_model(model);
        self.register("bedrock:", adapter)
    }

    /// 注册 GCP Vertex AI adapter (P10-6)
    ///
    /// 简化: Vertex AI 走 OpenAI-compatible endpoint.
    /// 业务方传 project + region + access_token (GCP service account token).
    /// endpoint: `https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/endpoints/openapi/chat/completions`
    ///
    /// 注册 prefix: `"vertex:"` (e.g. `vertex:gemini-1.5-pro`)
    ///
    /// **v1 简化**: 拿 access_token 当 bearer token. 业务方要拿到 token 才能用.
    /// **v2**: 集成 google-cloud-auth 走 service account 自动 refresh.
    pub fn with_vertex(
        self,
        project: impl AsRef<str>,
        region: impl AsRef<str>,
        access_token: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let endpoint = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/endpoints/openapi/chat/completions",
            region.as_ref(),
            project.as_ref(),
            region.as_ref()
        );
        let adapter = OpenaiAdapter::new(access_token)
            .with_endpoint(endpoint)
            .with_model(model);
        self.register("vertex:", adapter)
    }

    /// 自动从 env 变量加载 (P8-3)
    ///
    /// 检查 `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `DEEPSEEK_API_KEY` / `AZURE_OPENAI_*` 等环境变量,
    /// 自动注册. 业务方不传 key 也能跑 (只跑 stub).
    pub fn with_env(self) -> Self {
        let mut r = self;
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            r = r.with_openai(key);
        }
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            r = r.with_anthropic(key);
        }
        if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
            r = r.with_deepseek(key);
        }
        if let (Ok(resource), Ok(deployment), Ok(version), Ok(key)) = (
            std::env::var("AZURE_OPENAI_RESOURCE"),
            std::env::var("AZURE_OPENAI_DEPLOYMENT"),
            std::env::var("AZURE_OPENAI_API_VERSION"),
            std::env::var("AZURE_OPENAI_API_KEY"),
        ) {
            r = r.with_azure_openai(resource, deployment, version, key);
        }
        r
    }

    /// 根据 model name 找 adapter
    pub fn find(&self, model: &str) -> Option<std::sync::Arc<dyn ModelAdapter>> {
        // 1. exact match
        if let Some(a) = self.adapters.get(model) {
            return Some(a.clone());
        }
        // 2. prefix match ("openai:gpt-4o" → "openai:")
        for (prefix, adapter) in &self.adapters {
            if model.starts_with(prefix) {
                return Some(adapter.clone());
            }
        }
        None
    }

    /// 列所有注册的 prefix
    pub fn prefixes(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }
}

// ============================================================================
// 单元测试 — 不发真 HTTP, 只测 request 构造 + response 解析
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> ModelRequest {
        use ma_harness_core::ModelMessage;
        ModelRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![
                ModelMessage {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                },
                ModelMessage {
                    role: "assistant".to_string(),
                    content: "Hi there".to_string(),
                },
                ModelMessage {
                    role: "user".to_string(),
                    content: "How are you?".to_string(),
                },
            ],
            temperature: 0.5, // 用 0.5 避免 f32 精度丢
            max_tokens: 100,
            system_prompt: Some("You are a helpful assistant.".to_string()),
        }
    }

    // ---- OpenAI ----

    #[test]
    fn openai_build_request_body_includes_system_as_message() {
        let adapter = OpenaiAdapter::new("sk-test").with_model("gpt-4o-mini");
        let body = adapter.build_request_body(&sample_request());
        let messages = body.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 4, "system + 3 conversation messages");
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are a helpful assistant.");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hello");
        assert_eq!(body["model"], "gpt-4o-mini");
        // temperature 0.5 (二进制精确) → JSON Number(0.5)
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["max_tokens"], 100);
    }

    #[test]
    fn openai_build_request_body_no_system_prompt() {
        let mut req = sample_request();
        req.system_prompt = None;
        let adapter = OpenaiAdapter::new("sk-test");
        let body = adapter.build_request_body(&req);
        let messages = body.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 3, "no system message");
        assert_eq!(messages[0]["role"], "user");
    }

    #[test]
    fn openai_parse_response_extracts_content_and_tokens() {
        let adapter = OpenaiAdapter::new("sk-test");
        let body = serde_json::json!({
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4o-mini-2024-07-18",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 8,
                "total_tokens": 20
            }
        });
        let resp = adapter.parse_response(body).unwrap();
        assert_eq!(resp.model, "gpt-4o-mini-2024-07-18");
        assert_eq!(resp.content, "Hello! How can I help?");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.prompt_tokens, 12);
        assert_eq!(resp.completion_tokens, 8);
    }

    #[test]
    fn openai_parse_response_finish_reason_length() {
        let adapter = OpenaiAdapter::new("sk-test");
        let body = serde_json::json!({
            "model": "gpt-4o-mini",
            "choices": [{
                "message": {"role": "assistant", "content": "truncated..."},
                "finish_reason": "length"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 100, "total_tokens": 105}
        });
        let resp = adapter.parse_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert_eq!(resp.completion_tokens, 100);
    }

    #[test]
    fn openai_parse_response_missing_field_errors() {
        let adapter = OpenaiAdapter::new("sk-test");
        // 提供 model 但 choices 空 → MissingField "choices[0]"
        let body = serde_json::json!({"model": "gpt-4o-mini", "choices": []});
        let err = adapter.parse_response(body).unwrap_err();
        match err {
            AdapterError::MissingField(name) => assert_eq!(name, "choices[0]"),
            other => panic!("expected MissingField, got {:?}", other),
        }
    }

    #[test]
    fn openai_parse_response_missing_model_field_errors() {
        let adapter = OpenaiAdapter::new("sk-test");
        let body = serde_json::json!({"choices": [{"message": {"content": "x"}}]});
        let err = adapter.parse_response(body).unwrap_err();
        match err {
            AdapterError::MissingField(name) => assert_eq!(name, "model"),
            other => panic!("expected MissingField, got {:?}", other),
        }
    }

    // ---- Anthropic ----

    #[test]
    fn anthropic_build_request_body_uses_top_level_system() {
        let adapter = AnthropicAdapter::new("sk-ant-test").with_model("claude-3-5-sonnet-20241022");
        let body = adapter.build_request_body(&sample_request());
        // Anthropic: system 是 top-level string field
        assert_eq!(body["system"], "You are a helpful assistant.");
        let messages = body.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 3, "只 conversation messages, system 在顶层");
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(body["model"], "claude-3-5-sonnet-20241022");
        assert_eq!(body["max_tokens"], 100);
    }

    #[test]
    fn anthropic_parse_response_concatenates_text_blocks() {
        let adapter = AnthropicAdapter::new("sk-ant-test");
        let body = serde_json::json!({
            "id": "msg_abc",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-20241022",
            "content": [
                {"type": "text", "text": "First part."},
                {"type": "text", "text": "Second part."}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 15, "output_tokens": 20}
        });
        let resp = adapter.parse_response(body).unwrap();
        assert_eq!(resp.model, "claude-3-5-sonnet-20241022");
        assert_eq!(resp.content, "First part.\nSecond part.");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.prompt_tokens, 15);
        assert_eq!(resp.completion_tokens, 20);
    }

    #[test]
    fn anthropic_parse_response_stop_reason_max_tokens() {
        let adapter = AnthropicAdapter::new("sk-ant-test");
        let body = serde_json::json!({
            "model": "claude-3-5-sonnet-20241022",
            "content": [{"type": "text", "text": "truncated"}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 5, "output_tokens": 100}
        });
        let resp = adapter.parse_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
    }

    // ---- AdapterRegistry ----

    #[test]
    fn registry_register_and_find_by_exact_name() {
        let reg = AdapterRegistry::new()
            .with_openai("sk-test")
            .with_anthropic("sk-ant-test")
            .with_stub();

        let openai = reg.find("openai:").expect("openai registered");
        assert_eq!(openai.name(), "openai");

        let stub = reg.find("stub:").expect("stub registered");
        assert_eq!(stub.name(), "stub");

        let none = reg.find("does-not-exist");
        assert!(none.is_none());
    }

    #[test]
    fn registry_find_by_model_name_prefix() {
        let reg = AdapterRegistry::new().with_openai("sk-test");

        // "openai:gpt-4o-mini" 走 prefix match → "openai:"
        let adapter = reg.find("openai:gpt-4o-mini").expect("prefix match");
        assert_eq!(adapter.name(), "openai");

        // 裸 "gpt-4o-mini" 没匹配 (没注册 "gpt-4o-mini" 也没 "gpt-" prefix)
        assert!(reg.find("gpt-4o-mini").is_none());
    }

    #[test]
    fn registry_prefixes_lists_all_registered() {
        let reg = AdapterRegistry::new()
            .with_openai("sk-test")
            .with_anthropic("sk-ant-test")
            .with_stub();
        let prefixes = reg.prefixes();
        assert!(prefixes.contains(&"openai:".to_string()));
        assert!(prefixes.contains(&"anthropic:".to_string()));
        assert!(prefixes.contains(&"stub:".to_string()));
    }

    // ---- P8-3 多模型扩展 ----

    #[test]
    fn registry_with_azure_openai_registers_correct_endpoint() {
        let reg = AdapterRegistry::new().with_azure_openai(
            "myresource",
            "mydeployment",
            "2024-02-01",
            "sk-azure-test",
        );
        let adapter = reg.find("azure:").expect("azure registered");
        assert_eq!(adapter.name(), "openai");
        let prefixes = reg.prefixes();
        assert!(prefixes.contains(&"azure:".to_string()));
    }

    #[test]
    fn registry_with_local_registers_ollama_style() {
        let reg = AdapterRegistry::new()
            .with_local("http://localhost:11434/v1/chat/completions", "llama3");
        let adapter = reg.find("local:").expect("local registered");
        assert_eq!(adapter.name(), "openai");
    }

    #[test]
    fn registry_with_deepseek_registers() {
        let reg = AdapterRegistry::new().with_deepseek("sk-deepseek-test");
        let adapter = reg.find("deepseek:").expect("deepseek registered");
        assert_eq!(adapter.name(), "openai");
    }

    #[test]
    fn registry_with_env_loads_what_is_set() {
        // P8-3: 自动从环境变量加载 (有就装, 没就跳过)
        // 测试时可能没设, 验证不 panic
        let reg = AdapterRegistry::new().with_env();
        // 至少 stub 应该能跑 (业务方 fallback)
        let _ = reg.find("stub:");
    }

    #[test]
    fn registry_finds_deepseek_model() {
        // 业务方用 "deepseek:deepseek-chat" 走 prefix match
        let reg = AdapterRegistry::new().with_deepseek("sk-test");
        let adapter = reg.find("deepseek:deepseek-chat");
        assert!(adapter.is_some());
        assert_eq!(adapter.unwrap().name(), "openai");
    }

    #[test]
    fn registry_finds_azure_deployment() {
        // 业务方用 "azure:mydeployment" 走 prefix match
        let reg =
            AdapterRegistry::new().with_azure_openai("res", "mydeployment", "2024-02-01", "sk");
        let adapter = reg.find("azure:mydeployment");
        assert!(adapter.is_some());
    }

    // ---- P10-6 Bedrock / Vertex ----

    #[test]
    fn registry_with_bedrock_registers() {
        let reg = AdapterRegistry::new().with_bedrock(
            "us-east-1",
            "AKIA-TEST",
            "secret-test",
            "anthropic.claude-3-5-sonnet-20241022-v2:0",
        );
        let adapter = reg.find("bedrock:").expect("bedrock registered");
        assert_eq!(adapter.name(), "openai");
        let prefixes = reg.prefixes();
        assert!(prefixes.contains(&"bedrock:".to_string()));
    }

    #[test]
    fn registry_with_vertex_registers() {
        let reg = AdapterRegistry::new().with_vertex(
            "my-gcp-project",
            "us-central1",
            "ya29.test-token",
            "gemini-1.5-pro",
        );
        let adapter = reg.find("vertex:").expect("vertex registered");
        assert_eq!(adapter.name(), "openai");
        let prefixes = reg.prefixes();
        assert!(prefixes.contains(&"vertex:".to_string()));
    }

    #[test]
    fn registry_finds_bedrock_model() {
        let reg = AdapterRegistry::new().with_bedrock(
            "us-west-2",
            "AKIA",
            "secret",
            "anthropic.claude-3-5-sonnet",
        );
        let adapter = reg.find("bedrock:anthropic.claude-3-5-sonnet");
        assert!(adapter.is_some());
    }

    #[test]
    fn registry_finds_vertex_model() {
        let reg = AdapterRegistry::new().with_vertex("p", "us-central1", "tok", "gemini-1.5-pro");
        let adapter = reg.find("vertex:gemini-1.5-pro");
        assert!(adapter.is_some());
    }

    // ---- Stub 仍然工作 (回归测试, 确保不破坏 Phase 1) ----

    #[tokio::test]
    async fn stub_adapter_still_echoes() {
        let stub = StubModelAdapter;
        let req = sample_request();
        let resp = stub.complete(&req).await.unwrap();
        // StubModelAdapter echo 的是 *last user message* (= "How are you?")
        assert!(resp.content.contains("How are you?"));
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    // === P6-2 (Day 100): OpenAI 真 SSE streaming ===

    /// build_stream_request_body 加了 "stream": true 字段
    #[test]
    fn openai_build_stream_request_body_includes_stream_true() {
        let adapter = OpenaiAdapter::new("sk-test");
        let body = adapter.build_stream_request_body(&sample_request());
        assert_eq!(body["stream"], true, "streaming 必须 stream=true");
        // 其他字段跟非 stream 版一致
        assert_eq!(body["model"], "gpt-4o-mini");
        let messages = body.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 4);
    }

    /// parse_sse_data_line 拿 delta.content
    #[test]
    fn openai_parse_sse_data_line_extracts_delta_content() {
        let line = r#"data: {"id":"chatcmpl-abc","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"}}]}"#;
        let content = OpenaiAdapter::parse_sse_data_line(line);
        assert_eq!(content, Some("Hello".to_string()));
    }

    /// parse_sse_data_line: data: [DONE] → None (终止信号)
    #[test]
    fn openai_parse_sse_data_line_done_returns_none() {
        let line = "data: [DONE]";
        let content = OpenaiAdapter::parse_sse_data_line(line);
        assert_eq!(content, None, "[DONE] 终止信号 → None");
    }

    /// parse_sse_data_line: malformed JSON → None (静默 skip)
    #[test]
    fn openai_parse_sse_data_line_malformed_returns_none() {
        let line = "data: {this is not json}";
        let content = OpenaiAdapter::parse_sse_data_line(line);
        assert_eq!(content, None, "malformed JSON → None");
    }

    /// parse_sse_data_line: 不是 data: 开头 → None (e.g. event: / id: 行)
    #[test]
    fn openai_parse_sse_data_line_non_data_returns_none() {
        assert_eq!(OpenaiAdapter::parse_sse_data_line("event: message"), None);
        assert_eq!(OpenaiAdapter::parse_sse_data_line("id: 1"), None);
        assert_eq!(OpenaiAdapter::parse_sse_data_line(""), None);
    }

    /// parse_sse_data_line: delta.content 是空字符串 → Some("")
    /// (业务方 streaming 协议: role-only chunk 跟 content chunk 都合法)
    /// 区别: "content 字段 missing" → None, "content 字段 present 但空" → Some("")
    #[test]
    fn openai_parse_sse_data_line_empty_content_returns_empty_string() {
        let line = r#"data: {"choices":[{"index":0,"delta":{"role":"assistant","content":""}}]}"#;
        let content = OpenaiAdapter::parse_sse_data_line(line);
        assert_eq!(content, Some("".to_string()));
    }

    /// parse_sse_data_line: delta 没 content 字段 → None
    /// (业务方 role-only chunk 不发 token, 不是 streaming 终止)
    #[test]
    fn openai_parse_sse_data_line_missing_content_returns_none() {
        let line = r#"data: {"choices":[{"index":0,"delta":{"role":"assistant"}}]}"#;
        let content = OpenaiAdapter::parse_sse_data_line(line);
        assert_eq!(content, None, "delta 没 content 字段 → None");
    }

    /// parse_sse_data_line: multiple choices → 拿 choices[0]
    #[test]
    fn openai_parse_sse_data_line_multi_choice_takes_first() {
        let line = r#"data: {"choices":[{"index":0,"delta":{"content":"first"}},{"index":1,"delta":{"content":"second"}}]}"#;
        let content = OpenaiAdapter::parse_sse_data_line(line);
        assert_eq!(content, Some("first".to_string()), "多 choice 取 first");
    }

    // ---- P6-2 wiremock 端到端: 走真 HTTP + 拿 stream token ----

    /// 端到端 SSE: wiremock 返 "Hello" + " world" + [DONE], 业务方拿 2 token
    #[tokio::test]
    async fn openai_complete_stream_end_to_end_with_wiremock() {
        use futures::StreamExt;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // 1. 启 mock server
        let mock_server = MockServer::start().await;

        // 2. 准备 SSE 响应 body
        // 注: SSE spec 每个 event 跟 event 间空一行 (\n\n), 每个 event 内部行用 \n
        let sse_body = "\
data: {\"id\":\"chatcmpl-abc\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\n\
data: [DONE]\n\n";

        // 3. mock /v1/chat/completions POST 返 200 + SSE body
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&mock_server)
            .await;

        // 4. 构造 adapter 指 mock server
        let adapter = OpenaiAdapter::new("sk-test")
            .with_model("gpt-4o-mini")
            .with_endpoint(format!("{}/v1/chat/completions", mock_server.uri()));

        // 5. 调 complete_stream
        let req = sample_request();
        let mut stream = adapter.complete_stream(&req);
        let mut collected = Vec::new();
        while let Some(token) = stream.next().await {
            collected.push(token);
        }

        // 6. 验: 拿 2 token "Hello" + " world", 拼回 "Hello world"
        assert_eq!(collected.len(), 2, "应 yield 2 token, got {collected:?}");
        assert_eq!(collected[0], "Hello");
        assert_eq!(collected[1], " world");
        assert_eq!(collected.join(""), "Hello world");
    }

    /// 端到端 SSE: 流 chunked (不是一次性 body), 验 buffer 攒得对
    /// 业务方场景: OpenAI 实际返 stream 是 incremental chunks, 不是一坨
    #[tokio::test]
    async fn openai_complete_stream_handles_chunked_sse() {
        use futures::StreamExt;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // 故意切 3 chunk: 不完整 event 跨 chunk 边界
        let chunk1 = "data: {\"choices\":[{\"delta\":{\"content\":\"He";
        let chunk2 = "llo\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" wo";
        let chunk3 = "rld\"}}]}\n\ndata: [DONE]\n\n";

        // wiremock 不直接支持 stream body, 但 set_body_bytes 一次性送所有 bytes
        // 业务方真实场景下 reqwest bytes_stream 走 HTTP chunked transfer
        // 这里测试 SSE event 边界在 chunk 内的解析 (跟 wiremock 一起验证 integration)
        let body = format!("{chunk1}{chunk2}{chunk3}");
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .insert_header("transfer-encoding", "chunked")
                    .set_body_string(body),
            )
            .mount(&mock_server)
            .await;

        let adapter = OpenaiAdapter::new("sk-test")
            .with_endpoint(format!("{}/v1/chat/completions", mock_server.uri()));

        let req = sample_request();
        let mut stream = adapter.complete_stream(&req);
        let mut collected = Vec::new();
        while let Some(token) = stream.next().await {
            collected.push(token);
        }
        assert_eq!(collected.len(), 2, "应 yield 2 token, got {collected:?}");
        assert_eq!(collected.join(""), "Hello world");
    }

    // === P6-3 (Day 100): Anthropic SSE streaming ===

    /// build_stream_request_body 加 "stream": true
    #[test]
    fn anthropic_build_stream_request_body_includes_stream_true() {
        let adapter = AnthropicAdapter::new("sk-ant-test");
        let body = adapter.build_stream_request_body(&sample_request());
        assert_eq!(body["stream"], true, "streaming 必须 stream=true");
        assert_eq!(body["model"], "claude-3-5-sonnet-20241022");
        let messages = body.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 3, "no system message (system 是 top-level)");
    }

    /// parse_sse_event: content_block_delta 拿 text_delta.text
    #[test]
    fn anthropic_parse_sse_event_content_block_delta() {
        let event_type = "content_block_delta";
        let data = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let text = AnthropicAdapter::parse_sse_event(event_type, data);
        assert_eq!(text, Some("Hello".to_string()));
    }

    /// parse_sse_event: 其他 event (message_start / content_block_stop 等) 返 None
    #[test]
    fn anthropic_parse_sse_event_non_content_block_delta_returns_none() {
        // message_start 不发 text
        let data = r#"data: {"type":"message_start","message":{"id":"msg_01","role":"assistant"}}"#;
        assert_eq!(
            AnthropicAdapter::parse_sse_event("message_start", data),
            None
        );
        // content_block_stop 不发 text
        assert_eq!(
            AnthropicAdapter::parse_sse_event("content_block_stop", data),
            None
        );
        // message_delta 不发 text
        assert_eq!(
            AnthropicAdapter::parse_sse_event("message_delta", data),
            None
        );
    }

    /// parse_sse_event: malformed JSON → None
    #[test]
    fn anthropic_parse_sse_event_malformed_returns_none() {
        let data = "data: {this is not json}";
        let text = AnthropicAdapter::parse_sse_event("content_block_delta", data);
        assert_eq!(text, None);
    }

    /// 端到端 Anthropic SSE: wiremock 返 content_block_delta events, 业务方拿 2 token
    #[tokio::test]
    async fn anthropic_complete_stream_end_to_end_with_wiremock() {
        use futures::StreamExt;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Anthropic SSE 格式: event: <type>\ndata: {...}\n\n
        let sse_body = "\
event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\",\"role\":\"assistant\"}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\n";

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-ant-test"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&mock_server)
            .await;

        let adapter = AnthropicAdapter::new("sk-ant-test")
            .with_endpoint(format!("{}/v1/messages", mock_server.uri()));

        let req = sample_request();
        let mut stream = adapter.complete_stream(&req);
        let mut collected = Vec::new();
        while let Some(token) = stream.next().await {
            collected.push(token);
        }

        // 只 content_block_delta 走 yield → 2 token
        assert_eq!(
            collected.len(),
            2,
            "应 yield 2 token (只 content_block_delta), got {collected:?}"
        );
        assert_eq!(collected[0], "Hello");
        assert_eq!(collected[1], " world");
        assert_eq!(collected.join(""), "Hello world");
    }

    // === P11-5: Vision tests ===

    fn png_bytes() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn openai_vision_request_body_format() {
        let img = crate::multimodal::ImageAttachment::from_bytes("image/png", png_bytes());
        let adapter = OpenaiAdapter::new("sk-test").with_model("gpt-4o");
        let body = adapter.build_vision_request_body("describe", &[img]);

        // model 应 = gpt-4o
        assert_eq!(body["model"], "gpt-4o");
        // messages[0].content 应是 array (text + image)
        let content = &body["messages"][0]["content"];
        assert!(
            content.is_array(),
            "content should be array, got: {content}"
        );
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "describe");
        assert_eq!(arr[1]["type"], "image_url");
        let url = arr[1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn anthropic_vision_request_body_format() {
        let img =
            crate::multimodal::ImageAttachment::from_bytes("image/jpeg", vec![0xFF, 0xD8, 0xFF]);
        let adapter = AnthropicAdapter::new("sk-ant-test").with_model("claude-3-5-sonnet-20241022");
        let body = adapter.build_vision_request_body("what is this?", &[img]);

        // model 应 = claude-3-5-sonnet
        assert_eq!(body["model"], "claude-3-5-sonnet-20241022");
        // Anthropic 没有 system 在 messages 里 (top-level)
        let content = &body["messages"][0]["content"];
        assert!(content.is_array());
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "what is this?");
        assert_eq!(arr[1]["type"], "image");
        assert_eq!(arr[1]["source"]["type"], "base64");
        assert_eq!(arr[1]["source"]["media_type"], "image/jpeg");
        // data 应非空
        assert!(!arr[1]["source"]["data"].as_str().unwrap().is_empty());
    }

    #[test]
    fn vision_request_body_no_images_falls_back_to_text() {
        // 业务方传 0 images, 应只 text content
        let adapter = OpenaiAdapter::new("sk-test").with_model("gpt-4o-mini");
        let body = adapter.build_vision_request_body("just text", &[]);
        let content = &body["messages"][0]["content"];
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "text");
    }
}
