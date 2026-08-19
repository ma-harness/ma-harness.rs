//! P11-5: Multi-modal content (vision / audio).
//!
//! ## 设计
//!
//! `ImageAttachment` 跟 `ModelRequest` 平行, 不动现有 `ModelMessage.content: String` (保持后向兼容).
//! 业务方调 `OpenaiAdapter::build_vision_request_body(text, images)` 走 vision 模型.
//!
//! ## OpenAI Vision 协议
//!
//! ```json
//! {
//!   "model": "gpt-4o",
//!   "messages": [
//!     {
//!       "role": "user",
//!       "content": [
//!         {"type": "text", "text": "What's in this image?"},
//!         {"type": "image_url", "image_url": {"url": "data:image/png;base64,<base64>"}}
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! ## Anthropic Vision 协议
//!
//! ```json
//! {
//!   "model": "claude-3-5-sonnet-20241022",
//!   "messages": [
//!     {
//!       "role": "user",
//!       "content": [
//!         {"type": "text", "text": "What's in this image?"},
//!         {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "<base64>"}}
//!       ]
//!     }
//!   ]
//! }
//! ```

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

/// 图像附件 (P11-5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAttachment {
    /// MIME type (e.g. "image/png", "image/jpeg", "image/webp", "image/gif")
    pub media_type: String,
    /// 原始 bytes (PNG / JPEG / WebP / GIF)
    pub data: Vec<u8>,
    /// 可选文件名 (debug 用)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

impl ImageAttachment {
    /// 构造 (从 file path 读, business 一次性)
    pub fn from_path(path: &std::path::Path) -> std::io::Result<Self> {
        let data = std::fs::read(path)?;
        let media_type = guess_media_type(path)
            .unwrap_or("application/octet-stream")
            .to_string();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from);
        Ok(Self {
            media_type,
            data,
            filename,
        })
    }

    /// 构造 (从 bytes + media_type, 业务方 inline 用)
    pub fn from_bytes(media_type: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            media_type: media_type.into(),
            data,
            filename: None,
        }
    }

    /// base64 编码 (给 API 用)
    pub fn base64(&self) -> String {
        BASE64.encode(&self.data)
    }

    /// size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// 按文件扩展名猜 media_type
fn guess_media_type(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => return None,
    })
}

/// OpenAI vision helper — 构造 vision content array
///
/// 业务方用法:
/// ```ignore
/// let body = OpenaiAdapter::new("sk-...").with_model("gpt-4o")
///     .build_vision_request_body("describe", &[ImageAttachment::from_path(path)?]);
/// ```
pub fn build_openai_vision_content(
    text: &str,
    images: &[ImageAttachment],
) -> serde_json::Value {
    let mut content = vec![serde_json::json!({"type": "text", "text": text})];
    for img in images {
        let data_url = format!("data:{};base64,{}", img.media_type, img.base64());
        content.push(serde_json::json!({
            "type": "image_url",
            "image_url": {"url": data_url}
        }));
    }
    serde_json::Value::Array(content)
}

/// Anthropic vision helper — 构造 vision content array
pub fn build_anthropic_vision_content(
    text: &str,
    images: &[ImageAttachment],
) -> serde_json::Value {
    let mut content = vec![serde_json::json!({"type": "text", "text": text})];
    for img in images {
        content.push(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": img.media_type,
                "data": img.base64(),
            }
        }));
    }
    serde_json::Value::Array(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes() -> Vec<u8> {
        // 1x1 transparent PNG
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
            0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
            0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
            0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn image_attachment_base64() {
        let img = ImageAttachment::from_bytes("image/png", png_bytes());
        assert_eq!(img.media_type, "image/png");
        let b64 = img.base64();
        // 1x1 PNG base64 应非空
        assert!(!b64.is_empty());
        // decode 回去应 = 原 bytes
        let decoded = BASE64.decode(&b64).unwrap();
        assert_eq!(decoded, png_bytes());
    }

    #[test]
    fn image_attachment_from_path() {
        // 写 1x1 PNG 到 tempfile, 再读
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.png");
        std::fs::write(&path, png_bytes()).unwrap();
        let img = ImageAttachment::from_path(&path).unwrap();
        assert_eq!(img.media_type, "image/png");
        assert_eq!(img.data, png_bytes());
        assert_eq!(img.filename.as_deref(), Some("test.png"));
    }

    #[test]
    fn image_attachment_guess_media_type() {
        let png = ImageAttachment::from_bytes("image/png", vec![]);
        let jpg = ImageAttachment::from_bytes("image/jpeg", vec![]);
        let webp = ImageAttachment::from_bytes("image/webp", vec![]);
        assert_eq!(png.media_type, "image/png");
        assert_eq!(jpg.media_type, "image/jpeg");
        assert_eq!(webp.media_type, "image/webp");
    }

    #[test]
    fn openai_vision_content_text_only() {
        let content = build_openai_vision_content("hello", &[]);
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "hello");
    }

    #[test]
    fn openai_vision_content_text_plus_image() {
        let img = ImageAttachment::from_bytes("image/png", png_bytes());
        let content = build_openai_vision_content("describe", &[img]);
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "describe");
        assert_eq!(arr[1]["type"], "image_url");
        // data URL 应含 base64
        let url = arr[1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn openai_vision_content_multi_image() {
        let img1 = ImageAttachment::from_bytes("image/png", png_bytes());
        let img2 = ImageAttachment::from_bytes("image/jpeg", vec![0xFF, 0xD8]);
        let content = build_openai_vision_content("compare", &[img1, img2]);
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 3); // text + 2 images
        assert!(arr[1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/png;base64,"));
        assert!(arr[2]["image_url"]["url"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn anthropic_vision_content_text_plus_image() {
        let img = ImageAttachment::from_bytes("image/png", png_bytes());
        let content = build_anthropic_vision_content("describe", &[img]);
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "describe");
        assert_eq!(arr[1]["type"], "image");
        assert_eq!(arr[1]["source"]["type"], "base64");
        assert_eq!(arr[1]["source"]["media_type"], "image/png");
        // data 应是非空 base64
        assert!(!arr[1]["source"]["data"].as_str().unwrap().is_empty());
    }
}
