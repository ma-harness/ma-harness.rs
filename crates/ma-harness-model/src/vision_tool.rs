//! P11-9: Vision tool — 业务方用 vision model 描述图片.
//!
//! 简化 v1: 一次性调用 OpenAI / Anthropic vision model, 返回文字描述.
//! 后续 v2: 跟 tool registry 集成 (作为 plugin 暴露给 agent).
//!
//! ## 用法
//!
//! ```rust,no_run
//! use ma_harness_model::{ImageAttachment};
//! use ma_harness_model::vision_tool::{describe_image, VisionBackend};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let img = ImageAttachment::from_bytes("image/png", vec![0x89, 0x50, 0x4E, 0x47]);
//! let description = describe_image(
//!     "sk-...",
//!     VisionBackend::Openai,
//!     "describe this image in detail",
//!     &[img],
//! ).await?;
//! println!("{}", description);
//! # Ok(())
//! # }
//! ```

use crate::multimodal::ImageAttachment;
use crate::{AnthropicAdapter, OpenaiAdapter};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Vision backend (P11-9 v1: OpenAI + Anthropic)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionBackend {
    /// OpenAI gpt-4o / gpt-4-turbo
    Openai,
    /// Anthropic claude-3-5-sonnet / claude-3-opus
    Anthropic,
}

impl VisionBackend {
    /// 默认 model (per backend)
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Openai => "gpt-4o",
            Self::Anthropic => "claude-3-5-sonnet-20241022",
        }
    }
}

/// Vision 工具错误
#[derive(Debug, Error)]
pub enum VisionError {
    /// HTTP 错误
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    /// API 错误
    #[error("API returned status {status}: {body}")]
    Api {
        /// 状态码
        status: u16,
        /// 响应 body
        body: String,
    },
    /// 401 / 403
    #[error("authentication failed (status {status}): {body}")]
    Auth { status: u16, body: String },
    /// 429
    #[error("rate limited: {body}")]
    RateLimit { body: String },
    /// 响应解析
    #[error("failed to parse response: {0}")]
    Parse(#[from] serde_json::Error),
    /// 响应缺字段
    #[error("missing field in response: {0}")]
    MissingField(&'static str),
}

/// Vision 工具结果 (helper, 让业务方不直接碰 ModelResponse)
pub type VisionResult<T> = std::result::Result<T, VisionError>;

/// 通用 vision 调用 — 业务方拿 api_key + image, 返回 model 文字描述
///
/// 注意: OpenaiAdapter / AnthropicAdapter 复用, 不重写一遍网络层
pub async fn describe_with_openai(
    api_key: &str,
    model: &str,
    prompt: &str,
    images: &[ImageAttachment],
) -> VisionResult<String> {
    let adapter = OpenaiAdapter::new(api_key).with_model(model);
    let body = adapter.build_vision_request_body(prompt, images);
    let client = Client::new();
    let resp = client
        .post(&adapter.endpoint)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        let body = resp.text().await.unwrap_or_default();
        return Err(VisionError::Auth {
            status: status.as_u16(),
            body,
        });
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let body = resp.text().await.unwrap_or_default();
        return Err(VisionError::RateLimit { body });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(VisionError::Api {
            status: status.as_u16(),
            body,
        });
    }
    let body: serde_json::Value = resp.json().await?;
    let content = body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or(VisionError::MissingField("choices[0].message.content"))?;
    Ok(content.to_string())
}

/// Anthropic vision 调用
pub async fn describe_with_anthropic(
    api_key: &str,
    model: &str,
    prompt: &str,
    images: &[ImageAttachment],
) -> VisionResult<String> {
    let adapter = AnthropicAdapter::new(api_key).with_model(model);
    let body = adapter.build_vision_request_body(prompt, images);
    let client = Client::new();
    let resp = client
        .post(&adapter.endpoint)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        let body = resp.text().await.unwrap_or_default();
        return Err(VisionError::Auth {
            status: status.as_u16(),
            body,
        });
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let body = resp.text().await.unwrap_or_default();
        return Err(VisionError::RateLimit { body });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(VisionError::Api {
            status: status.as_u16(),
            body,
        });
    }
    let body: serde_json::Value = resp.json().await?;
    // Anthropic: content[].text 拼接
    let content_array = body
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or(VisionError::MissingField("content"))?;
    let mut result = String::new();
    for block in content_array {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(text);
            }
        }
    }
    if result.is_empty() {
        return Err(VisionError::MissingField("content[].text"));
    }
    Ok(result)
}

/// 顶层 describe_image — 业务方选 backend
pub async fn describe_image(
    api_key: &str,
    backend: VisionBackend,
    prompt: &str,
    images: &[ImageAttachment],
) -> VisionResult<String> {
    let model = backend.default_model();
    match backend {
        VisionBackend::Openai => describe_with_openai(api_key, model, prompt, images).await,
        VisionBackend::Anthropic => describe_with_anthropic(api_key, model, prompt, images).await,
    }
}

/// Vision tool schema (给 tool registry 集成用, P11-9 v2)
///
/// Tool name: "vision_describe"
/// Args:
///   - `image_paths: Vec<String>` — 图片文件路径列表
///   - `prompt: String` — 描述 prompt
///   - `backend: String` — "openai" / "anthropic" (default "openai")
///
/// Returns: `{ "description": "..." }` 或 `{ "error": "..." }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionDescribeArgs {
    /// 图片路径列表 (1+)
    pub image_paths: Vec<String>,
    /// 描述 prompt
    pub prompt: String,
    /// Backend (默认 "openai")
    #[serde(default = "default_backend")]
    pub backend: String,
}

fn default_backend() -> String {
    "openai".to_string()
}

/// Vision tool 描述 (业务方 metadata)
pub const VISION_TOOL_NAME: &str = "vision_describe";
pub const VISION_TOOL_DESCRIPTION: &str = "Describe one or more images using a vision model (OpenAI gpt-4o or Anthropic claude-3-5-sonnet)";

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

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
    fn vision_backend_default_model() {
        assert_eq!(VisionBackend::Openai.default_model(), "gpt-4o");
        assert_eq!(
            VisionBackend::Anthropic.default_model(),
            "claude-3-5-sonnet-20241022"
        );
    }

    #[test]
    fn vision_describe_args_default_backend() {
        let json = r#"{"image_paths": ["a.png"], "prompt": "x"}"#;
        let args: VisionDescribeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.backend, "openai");
    }

    #[test]
    fn vision_describe_args_explicit_backend() {
        let json = r#"{"image_paths": ["a.png"], "prompt": "x", "backend": "anthropic"}"#;
        let args: VisionDescribeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.backend, "anthropic");
    }

    #[test]
    fn vision_tool_name_constant() {
        assert_eq!(VISION_TOOL_NAME, "vision_describe");
        assert!(!VISION_TOOL_DESCRIPTION.is_empty());
    }

    #[test]
    fn image_attachment_for_tool() {
        // 业务方准备 image 给 tool 用
        let img = ImageAttachment::from_bytes("image/png", png_bytes());
        assert_eq!(img.media_type, "image/png");
        assert!(!img.base64().is_empty());
    }

    #[test]
    fn vision_error_display() {
        let e = VisionError::Api {
            status: 500,
            body: "internal".to_string(),
        };
        assert!(e.to_string().contains("500"));
        assert!(e.to_string().contains("internal"));
    }
}
