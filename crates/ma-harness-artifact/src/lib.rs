//! # ma-harness Vibe Coding Artifact Viewer (P11-7)
//!
//! 业务方 agent 输出 HTML / SVG / Code / JSON / Image 时, 识别 + 渲染.
//!
//! ## v1 简化
//!
//! - 按文件扩展名 + content 头部识别 artifact type
//! - 终端渲染: HTML → text summary, SVG → dimensions, JSON → pretty, Code → 简单 syntax hint
//! - 真实 Web UI 渲染留 v2
//!
//! ## 支持的 artifact types
//!
//! - `Html` — `.html` / `.htm` (text/html)
//! - `Svg` — `.svg` (image/svg+xml)
//! - `Json` — `.json` (application/json)
//! - `Code` — `.rs` / `.py` / `.js` / `.ts` / `.go` / `.java` / etc.
//! - `Markdown` — `.md` (text/markdown)
//! - `Image` — `.png` / `.jpg` / `.jpeg` / `.gif` / `.webp`
//! - `Yaml` — `.yaml` / `.yml`
//! - `Toml` — `.toml`
//! - `Text` — 其他 plain text
//! - `Binary` — 二进制文件
//!
//! ## API
//!
//! ```rust
//! use ma_harness_artifact::{detect_artifact, render_terminal};
//!
//! let bytes = b"<!DOCTYPE html><html><head><title>Hi</title></head></html>";
//! let kind = detect_artifact("test.html", bytes);
//! let rendered = render_terminal(&kind, bytes);
//! assert!(rendered.contains("Title: Hi"));
//! ```

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// Artifact 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// HTML (text/html)
    Html,
    /// SVG (image/svg+xml)
    Svg,
    /// JSON (application/json)
    Json,
    /// Code (编程语言 source code)
    Code,
    /// Markdown (text/markdown)
    Markdown,
    /// Image (PNG / JPEG / GIF / WebP)
    Image,
    /// YAML
    Yaml,
    /// TOML
    Toml,
    /// 纯文本
    Text,
    /// 二进制
    Binary,
}

impl ArtifactKind {
    /// MIME type
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Html => "text/html",
            Self::Svg => "image/svg+xml",
            Self::Json => "application/json",
            Self::Code => "text/x-code",
            Self::Markdown => "text/markdown",
            Self::Image => "image/*",
            Self::Yaml => "application/yaml",
            Self::Toml => "application/toml",
            Self::Text => "text/plain",
            Self::Binary => "application/octet-stream",
        }
    }

    /// 人类可读名
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Html => "HTML",
            Self::Svg => "SVG",
            Self::Json => "JSON",
            Self::Code => "Code",
            Self::Markdown => "Markdown",
            Self::Image => "Image",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::Text => "Text",
            Self::Binary => "Binary",
        }
    }
}

/// Artifact 错误
#[derive(Debug, Error)]
pub enum ArtifactError {
    /// IO 错误
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON 解析错误
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// 按文件扩展名 + 内容头部检测 artifact kind
pub fn detect_artifact(path: impl AsRef<Path>, bytes: &[u8]) -> ArtifactKind {
    // 1. 按扩展名
    let ext = path
        .as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some("html") | Some("htm") => return ArtifactKind::Html,
        Some("svg") => return ArtifactKind::Svg,
        Some("json") => return ArtifactKind::Json,
        Some("md") | Some("markdown") => return ArtifactKind::Markdown,
        Some("yaml") | Some("yml") => return ArtifactKind::Yaml,
        Some("toml") => return ArtifactKind::Toml,
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") | Some("bmp") => {
            return ArtifactKind::Image
        }
        Some("rs") | Some("py") | Some("js") | Some("ts") | Some("tsx") | Some("jsx")
        | Some("go") | Some("java") | Some("kt") | Some("swift") | Some("c") | Some("cpp")
        | Some("h") | Some("hpp") | Some("cs") | Some("rb") | Some("php") | Some("sh")
        | Some("bash") | Some("zsh") | Some("fish") | Some("sql") | Some("html") => {
            return ArtifactKind::Code
        }
        _ => {}
    }

    // 2. 按 content 头部 (无扩展名 / 扩展名未知时)
    if bytes.is_empty() {
        return ArtifactKind::Text;
    }
    let head = &bytes[..bytes.len().min(512)];

    // SVG: <?xml ... <svg ...
    if head.starts_with(b"<?xml") || head.windows(4).any(|w| w == b"<svg") {
        return ArtifactKind::Svg;
    }

    // HTML: <!DOCTYPE html ... 或 <html ... 或 <head ...
    let head_lower = head.to_ascii_lowercase();
    if head_lower.starts_with(b"<!doctype html")
        || head_lower.starts_with(b"<html")
        || head_lower.windows(6).any(|w| w == b"<head>")
    {
        return ArtifactKind::Html;
    }

    // JSON: 必须是 { 或 [ 开头 (用 strict mode 检测)
    if head.iter().find(|&&b| !b.is_ascii_whitespace()).copied() == Some(b'{')
        || head.iter().find(|&&b| !b.is_ascii_whitespace()).copied() == Some(b'[')
    {
        if serde_json::from_slice::<serde_json::Value>(bytes).is_ok() {
            return ArtifactKind::Json;
        }
    }

    // TOML: 简单检查含 [section] 或 key = value
    if std::str::from_utf8(head)
        .map(|s| s.contains('[') && s.contains(']') && s.contains('='))
        .unwrap_or(false)
    {
        return ArtifactKind::Toml;
    }

    // YAML: 简单检查含 "---" 开头 或 "key: value"
    if head.starts_with(b"---")
        || (std::str::from_utf8(head)
            .map(|s| s.contains(": ") && !s.contains('{'))
            .unwrap_or(false))
    {
        return ArtifactKind::Yaml;
    }

    // 二进制: 含 null bytes 或非 UTF-8
    if !std::str::from_utf8(bytes).is_ok() {
        return ArtifactKind::Binary;
    }

    ArtifactKind::Text
}

/// 渲染 artifact 到 terminal (人类可读)
pub fn render_terminal(kind: &ArtifactKind, bytes: &[u8]) -> String {
    let mut out = String::new();
    out.push_str(&format!("Artifact: {}\n", kind.display_name()));
    out.push_str(&format!("MIME: {}\n", kind.mime_type()));
    out.push_str(&format!("Size: {} bytes\n", bytes.len()));
    out.push_str("---\n");

    match kind {
        ArtifactKind::Html => {
            // 提取 <title> 跟 <h1> 跟 <body> 前 200 字符
            if let Ok(text) = std::str::from_utf8(bytes) {
                if let Some(title) = extract_tag(text, "title") {
                    out.push_str(&format!("Title: {}\n", title));
                }
                if let Some(h1) = extract_tag(text, "h1") {
                    out.push_str(&format!("H1: {}\n", h1));
                }
                if let Some(body) = extract_tag(text, "body") {
                    let snippet: String = body.chars().take(200).collect();
                    out.push_str(&format!("Body preview: {}\n", snippet));
                }
                let tag_count = count_tags(text);
                out.push_str(&format!("Tags: {} total\n", tag_count));
            }
        }
        ArtifactKind::Svg => {
            // 提取 width / height / viewBox
            if let Ok(text) = std::str::from_utf8(bytes) {
                if let Some(w) = extract_attr(text, "svg", "width") {
                    out.push_str(&format!("Width: {}\n", w));
                }
                if let Some(h) = extract_attr(text, "svg", "height") {
                    out.push_str(&format!("Height: {}\n", h));
                }
                if let Some(vb) = extract_attr(text, "svg", "viewBox") {
                    out.push_str(&format!("ViewBox: {}\n", vb));
                }
                let elem_count = count_svg_elements(text);
                out.push_str(&format!("Elements: {} total\n", elem_count));
            }
        }
        ArtifactKind::Json => {
            // 重新格式化 JSON
            match serde_json::from_slice::<serde_json::Value>(bytes) {
                Ok(v) => {
                    let pretty = serde_json::to_string_pretty(&v).unwrap_or_default();
                    let preview: String = pretty.chars().take(500).collect();
                    out.push_str(&format!("{}\n", preview));
                    if pretty.len() > 500 {
                        out.push_str("... (truncated)\n");
                    }
                }
                Err(e) => {
                    out.push_str(&format!("JSON parse failed: {e}\n"));
                }
            }
        }
        ArtifactKind::Code => {
            // 行数 + 前 30 行
            if let Ok(text) = std::str::from_utf8(bytes) {
                let lines: Vec<&str> = text.lines().collect();
                out.push_str(&format!("Lines: {}\n", lines.len()));
                out.push_str("---\n");
                for line in lines.iter().take(30) {
                    out.push_str(&format!("{}\n", line));
                }
                if lines.len() > 30 {
                    out.push_str(&format!("... ({} more lines)\n", lines.len() - 30));
                }
            }
        }
        ArtifactKind::Markdown => {
            // 提取 # / ## 标题
            if let Ok(text) = std::str::from_utf8(bytes) {
                let lines: Vec<&str> = text.lines().collect();
                out.push_str(&format!("Lines: {}\n", lines.len()));
                out.push_str("Headings:\n");
                for line in &lines {
                    let trimmed = line.trim();
                    if trimmed.starts_with("# ") {
                        out.push_str(&format!("  H1: {}\n", &trimmed[2..]));
                    } else if trimmed.starts_with("## ") {
                        out.push_str(&format!("  H2: {}\n", &trimmed[3..]));
                    }
                }
            }
        }
        ArtifactKind::Image => {
            // PNG: 检查签名
            if bytes.len() >= 8 && bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
                out.push_str("Format: PNG\n");
            } else if bytes.len() >= 3 && bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                out.push_str("Format: JPEG\n");
            } else if bytes.len() >= 6 && bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
                out.push_str("Format: GIF\n");
            } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
                out.push_str("Format: WebP\n");
            } else {
                out.push_str("Format: Unknown image\n");
            }
        }
        ArtifactKind::Yaml | ArtifactKind::Toml => {
            // 行数 + 前 30 行
            if let Ok(text) = std::str::from_utf8(bytes) {
                let lines: Vec<&str> = text.lines().collect();
                out.push_str(&format!("Lines: {}\n", lines.len()));
                out.push_str("---\n");
                for line in lines.iter().take(30) {
                    out.push_str(&format!("{}\n", line));
                }
                if lines.len() > 30 {
                    out.push_str(&format!("... ({} more lines)\n", lines.len() - 30));
                }
            }
        }
        ArtifactKind::Text => {
            if let Ok(text) = std::str::from_utf8(bytes) {
                let lines: Vec<&str> = text.lines().collect();
                out.push_str(&format!("Lines: {}\n", lines.len()));
                let preview: String = text.chars().take(200).collect();
                out.push_str(&format!("Preview: {}\n", preview));
                if text.len() > 200 {
                    out.push_str("... (truncated)\n");
                }
            }
        }
        ArtifactKind::Binary => {
            out.push_str("Binary content, no terminal preview.\n");
            out.push_str(&format!("First 32 bytes (hex): {}\n", hex_preview(&bytes[..bytes.len().min(32)])));
        }
    }
    out
}

/// 从 text 提取 <tag>...</tag> 第一个匹配的内容
fn extract_tag(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = text.find(&open)?;
    let after_open = &text[start..];
    let content_start = after_open.find('>')? + 1;
    let content = &after_open[content_start..];
    let end = content.find(&close)?;
    Some(content[..end].trim().to_string())
}

/// 从 text 提取 <tag attr="value" ...> 第一个 attr 值
fn extract_attr(text: &str, tag: &str, attr: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let start = text.find(&open)?;
    let after = &text[start..];
    let end = after.find('>')?;
    let tag_str = &after[..=end];
    // 找 attr="value" 或 attr='value'
    let attr_pattern = format!("{}=\"", attr);
    let attr_pattern_alt = format!("{}='", attr);
    if let Some(p) = tag_str.find(&attr_pattern) {
        let v_start = p + attr_pattern.len();
        let rest = &tag_str[v_start..];
        let v_end = rest.find('"')?;
        return Some(rest[..v_end].to_string());
    }
    if let Some(p) = tag_str.find(&attr_pattern_alt) {
        let v_start = p + attr_pattern_alt.len();
        let rest = &tag_str[v_start..];
        let v_end = rest.find('\'')?;
        return Some(rest[..v_end].to_string());
    }
    None
}

/// 粗略数 tags
fn count_tags(text: &str) -> usize {
    let mut count = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' && i + 1 < bytes.len() && bytes[i + 1] != b'/' && bytes[i + 1] != b'!' {
            count += 1;
        }
        i += 1;
    }
    count
}

/// 粗略数 SVG 元素 (rect / circle / path / g / etc.)
fn count_svg_elements(text: &str) -> usize {
    let elements = ["rect", "circle", "ellipse", "line", "path", "polyline", "polygon", "g", "text", "use"];
    let mut total = 0;
    for elem in &elements {
        total += text.matches(&format!("<{}", elem)).count();
    }
    total
}

/// Hex preview for binary
fn hex_preview(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_html_by_extension() {
        let bytes = b"<html><body>hello</body></html>";
        assert_eq!(detect_artifact("test.html", bytes), ArtifactKind::Html);
        assert_eq!(detect_artifact("TEST.HTM", bytes), ArtifactKind::Html);
    }

    #[test]
    fn detect_html_by_content() {
        let bytes = b"<!DOCTYPE html><html><head><title>Hi</title></head><body>x</body></html>";
        assert_eq!(detect_artifact("no-ext", bytes), ArtifactKind::Html);
    }

    #[test]
    fn detect_svg_by_extension() {
        let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        assert_eq!(detect_artifact("test.svg", bytes), ArtifactKind::Svg);
    }

    #[test]
    fn detect_svg_by_content() {
        let bytes = b"<?xml version=\"1.0\"?><svg></svg>";
        assert_eq!(detect_artifact("no-ext", bytes), ArtifactKind::Svg);
    }

    #[test]
    fn detect_json_by_extension() {
        let bytes = br#"{"key": "value"}"#;
        assert_eq!(detect_artifact("test.json", bytes), ArtifactKind::Json);
    }

    #[test]
    fn detect_json_by_content() {
        let bytes = br#"{"key": "value", "arr": [1, 2, 3]}"#;
        assert_eq!(detect_artifact("no-ext", bytes), ArtifactKind::Json);
    }

    #[test]
    fn detect_invalid_json_falls_back_to_text() {
        let bytes = b"{this is not valid json";
        // 不是合法 JSON, 也不是 binary, 走 Text
        let k = detect_artifact("no-ext", bytes);
        assert!(k == ArtifactKind::Text || k == ArtifactKind::Binary, "got: {k:?}");
    }

    #[test]
    fn detect_code_by_extension() {
        let bytes = b"fn main() {}";
        assert_eq!(detect_artifact("main.rs", bytes), ArtifactKind::Code);
        assert_eq!(detect_artifact("script.py", bytes), ArtifactKind::Code);
        assert_eq!(detect_artifact("app.js", bytes), ArtifactKind::Code);
        assert_eq!(detect_artifact("lib.ts", bytes), ArtifactKind::Code);
        assert_eq!(detect_artifact("main.go", bytes), ArtifactKind::Code);
    }

    #[test]
    fn detect_markdown_by_extension() {
        let bytes = b"# Title\n\nbody";
        assert_eq!(detect_artifact("README.md", bytes), ArtifactKind::Markdown);
    }

    #[test]
    fn detect_image_by_extension() {
        let png_sig = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(detect_artifact("logo.png", &png_sig), ArtifactKind::Image);
        assert_eq!(detect_artifact("photo.jpg", &[0xFF, 0xD8, 0xFF]), ArtifactKind::Image);
    }

    #[test]
    fn detect_yaml_by_content() {
        let bytes = b"---\nkey: value\nother: 42\n";
        assert_eq!(detect_artifact("no-ext", bytes), ArtifactKind::Yaml);
    }

    #[test]
    fn detect_toml_by_content() {
        let bytes = b"[section]\nkey = \"value\"\n";
        assert_eq!(detect_artifact("no-ext", bytes), ArtifactKind::Toml);
    }

    #[test]
    fn detect_binary_by_content() {
        let bytes: &[u8] = &[0xFF, 0xFE, 0x00, 0x01, 0x02, 0x03];
        // 扩展名无, content 非 UTF-8 → Binary
        // 但 fallback 逻辑先看 SVG/HTML/JSON 都不匹配, 然后看 TOML/YAML, 最后看 UTF-8
        let k = detect_artifact("no-ext", bytes);
        assert_eq!(k, ArtifactKind::Binary);
    }

    #[test]
    fn detect_empty_bytes() {
        assert_eq!(detect_artifact("no-ext", b""), ArtifactKind::Text);
    }

    #[test]
    fn mime_type_per_kind() {
        assert_eq!(ArtifactKind::Html.mime_type(), "text/html");
        assert_eq!(ArtifactKind::Svg.mime_type(), "image/svg+xml");
        assert_eq!(ArtifactKind::Json.mime_type(), "application/json");
        assert_eq!(ArtifactKind::Code.mime_type(), "text/x-code");
        assert_eq!(ArtifactKind::Image.mime_type(), "image/*");
    }

    #[test]
    fn display_name_per_kind() {
        assert_eq!(ArtifactKind::Html.display_name(), "HTML");
        assert_eq!(ArtifactKind::Svg.display_name(), "SVG");
        assert_eq!(ArtifactKind::Json.display_name(), "JSON");
    }

    #[test]
    fn render_html_with_title() {
        let bytes = b"<!DOCTYPE html><html><head><title>My Page</title></head><body><h1>Hello</h1><p>world</p></body></html>";
        let out = render_terminal(&ArtifactKind::Html, bytes);
        assert!(out.contains("Title: My Page"));
        assert!(out.contains("H1: Hello"));
        assert!(out.contains("Body preview:"));
    }

    #[test]
    fn render_svg_with_dimensions() {
        let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\" viewBox=\"0 0 100 50\"><rect x=\"0\" y=\"0\" width=\"100\" height=\"50\"/></svg>";
        let out = render_terminal(&ArtifactKind::Svg, bytes);
        assert!(out.contains("Width: 100"));
        assert!(out.contains("Height: 50"));
        assert!(out.contains("ViewBox: 0 0 100 50"));
        assert!(out.contains("Elements:"));
    }

    #[test]
    fn render_json_pretty() {
        let bytes = br#"{"name": "test", "version": "1.0", "tags": ["a", "b"]}"#;
        let out = render_terminal(&ArtifactKind::Json, bytes);
        assert!(out.contains("\"name\": \"test\""));
        assert!(out.contains("JSON"));
    }

    #[test]
    fn render_code_line_count() {
        let bytes = b"line 1\nline 2\nline 3\n";
        let out = render_terminal(&ArtifactKind::Code, bytes);
        assert!(out.contains("Lines: 3"));
        assert!(out.contains("line 1"));
        assert!(out.contains("line 3"));
    }

    #[test]
    fn render_code_truncates_long() {
        let mut bytes = Vec::new();
        for i in 0..100 {
            bytes.extend_from_slice(format!("line {}\n", i).as_bytes());
        }
        let out = render_terminal(&ArtifactKind::Code, &bytes);
        assert!(out.contains("Lines: 100"));
        assert!(out.contains("more lines"));
    }

    #[test]
    fn render_markdown_headings() {
        let bytes = b"# Title\n\n## Subtitle\n\nbody\n";
        let out = render_terminal(&ArtifactKind::Markdown, bytes);
        assert!(out.contains("H1: Title"));
        assert!(out.contains("H2: Subtitle"));
    }

    #[test]
    fn render_image_format_detection() {
        let png_sig: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        let out = render_terminal(&ArtifactKind::Image, png_sig);
        assert!(out.contains("Format: PNG"));

        let jpeg_sig: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];
        let out = render_terminal(&ArtifactKind::Image, jpeg_sig);
        assert!(out.contains("Format: JPEG"));

        let gif_sig: &[u8] = b"GIF89a...";
        let out = render_terminal(&ArtifactKind::Image, gif_sig);
        assert!(out.contains("Format: GIF"));

        let webp_sig: &[u8] = b"RIFF....WEBPVP8";
        let out = render_terminal(&ArtifactKind::Image, webp_sig);
        assert!(out.contains("Format: WebP"));
    }

    #[test]
    fn render_binary_shows_hex() {
        let bytes: &[u8] = &[0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE];
        let out = render_terminal(&ArtifactKind::Binary, bytes);
        assert!(out.contains("Binary content"));
        assert!(out.contains("hex"));
        assert!(out.contains("00 01 02 03"));
    }

    #[test]
    fn end_to_end_detect_and_render() {
        let bytes = b"<!DOCTYPE html><html><head><title>Test</title></head><body>x</body></html>";
        let kind = detect_artifact("test.html", bytes);
        let out = render_terminal(&kind, bytes);
        assert!(out.contains("HTML"));
        assert!(out.contains("Title: Test"));
        assert!(out.contains("Size:"));
    }
}
