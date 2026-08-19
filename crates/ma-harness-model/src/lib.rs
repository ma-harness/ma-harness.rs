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
use ma_harness_core::{FinishReason, ModelAdapter, ModelRequest, ModelResponse, StubModelAdapter};
use thiserror::Error;

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
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
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
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
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
}
