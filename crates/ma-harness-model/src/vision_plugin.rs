//! P12-8: Vision tool v2 — 跟 ma_harness_core::ToolRegistry 集成
//!
//! 业务方注册 vision tool 到 ToolRegistry, 跟其他 tool (bash / fs / 等) 一起被 agent 调用.
//!
//! ## 设计
//!
//! - `VisionTool` 暴露 `schema()` + `invoke()` 方法
//! - 业务方用 `register_to_registry(&mut registry, &api_key, VisionBackend::Openai)` 注册
//! - Agent LLM 看到 vision_describe tool, 业务方传 image_paths + prompt, tool 返 description
//!
//! ## 用法
//!
//! ```rust,ignore
//! use ma_harness_model::vision_plugin::VisionTool;
//! use ma_harness_model::VisionBackend;
//! use ma_harness_core::ToolRegistry;
//!
//! let mut registry = ToolRegistry::default();
//! let tool = VisionTool::new("sk-...", VisionBackend::Openai);
//! tool.register(&mut registry);
//!
//! // agent 看到 vision_describe tool
//! // 业务方 agent 调: { "image_paths": ["a.png"], "prompt": "describe" }
//! ```

use crate::multimodal::ImageAttachment;
use crate::vision_tool::VisionBackend;
use ma_harness_cordis::Context;
use ma_harness_core::{ToolInvokeFn, ToolSchema, ToolRegistry};
use serde_json::{json, Value};

/// Vision tool (P12-8 v2)
pub struct VisionTool {
    /// API key for vision model
    api_key: String,
    /// Backend (Openai / Anthropic)
    backend: VisionBackend,
    /// Optional override of model name
    model_override: Option<String>,
    /// Tool description (业务方 register 时给 LLM 看)
    description: String,
}

impl VisionTool {
    /// 构造 (默认 model: backend.default_model())
    pub fn new(api_key: impl Into<String>, backend: VisionBackend) -> Self {
        Self {
            api_key: api_key.into(),
            backend,
            model_override: None,
            description: crate::vision_tool::VISION_TOOL_DESCRIPTION.to_string(),
        }
    }

    /// Override model (业务方传 "gpt-4-turbo" / "claude-3-opus-20240229" 等)
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model_override = Some(model.into());
        self
    }

    /// Override description (业务方接更具体的 prompt)
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// 业务方拿 schema (给 LLM 看)
    pub fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: crate::vision_tool::VISION_TOOL_NAME.to_string(),
            description: self.description.clone(),
            // JSON Schema: image_paths: string[], prompt: string, backend?: string
            parameters: json!({
                "type": "object",
                "properties": {
                    "image_paths": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Paths to image files (PNG / JPEG / GIF / WebP)"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Description prompt (e.g. 'describe this image in detail')"
                    },
                    "backend": {
                        "type": "string",
                        "enum": ["openai", "anthropic"],
                        "description": "Vision backend (default: 'openai')"
                    }
                },
                "required": ["image_paths", "prompt"]
            }),
        }
    }

    /// 业务方 invoke tool (实际 vision API 调用)
    pub async fn invoke(&self, args: Value) -> anyhow::Result<Value> {
        // 解析 args
        let image_paths = args
            .get("image_paths")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("missing image_paths"))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>();

        if image_paths.is_empty() {
            return Err(anyhow::anyhow!("image_paths must not be empty"));
        }

        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing prompt"))?
            .to_string();

        // 业务方 backend 可 override
        let backend = args
            .get("backend")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "openai" => Some(VisionBackend::Openai),
                "anthropic" => Some(VisionBackend::Anthropic),
                _ => None,
            })
            .unwrap_or(self.backend);

        // 业务方 load images
        let mut images = Vec::with_capacity(image_paths.len());
        for path in &image_paths {
            let img = ImageAttachment::from_path(std::path::Path::new(path))
                .map_err(|e| anyhow::anyhow!("failed to load image {path}: {e}"))?;
            images.push(img);
        }

        // 业务方 call vision model
        let model = self
            .model_override
            .clone()
            .unwrap_or_else(|| backend.default_model().to_string());

        let description = match backend {
            VisionBackend::Openai => {
                crate::vision_tool::describe_with_openai(
                    &self.api_key,
                    &model,
                    &prompt,
                    &images,
                )
                .await
            }
            VisionBackend::Anthropic => {
                crate::vision_tool::describe_with_anthropic(
                    &self.api_key,
                    &model,
                    &prompt,
                    &images,
                )
                .await
            }
        }
        .map_err(|e| anyhow::anyhow!("vision API failed: {e}"))?;

        // 业务方返 result (JSON)
        Ok(json!({
            "description": description,
            "image_count": images.len(),
            "model": model,
            "backend": match backend {
                VisionBackend::Openai => "openai",
                VisionBackend::Anthropic => "anthropic",
            },
        }))
    }

    /// 业务方注册到 ToolRegistry (P12-8 v2 主 API)
    pub fn register(self, registry: &ToolRegistry) {
        let schema = self.schema();
        let api_key = self.api_key.clone();
        let backend = self.backend;
        let model_override = self.model_override.clone();

        // 注意: ctx 不用 (vision tool 不需要 plugin context, 直接 HTTP 调)
        let invoke: ToolInvokeFn = std::sync::Arc::new(move |args: Value, _ctx: &Context| {
            let tool = VisionTool {
                api_key: api_key.clone(),
                backend,
                model_override: model_override.clone(),
                description: String::new(),
            };
            Box::pin(async move { tool.invoke(args).await })
        });

        registry.register(schema, invoke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_tool_schema_has_required_params() {
        let tool = VisionTool::new("sk-test", VisionBackend::Openai);
        let schema = tool.schema();
        assert_eq!(schema.name, "vision_describe");
        assert!(schema.description.contains("image") || schema.description.contains("vision"));
        // 业务方校验 parameters 是 valid JSON Schema
        let params = &schema.parameters;
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["image_paths"].is_object());
        assert!(params["properties"]["prompt"].is_object());
        // image_paths + prompt required
        let required = params["required"].as_array().expect("required array");
        assert!(required.iter().any(|v| v == "image_paths"));
        assert!(required.iter().any(|v| v == "prompt"));
    }

    #[test]
    fn vision_tool_with_model_override() {
        let tool = VisionTool::new("sk-test", VisionBackend::Openai)
            .with_model("gpt-4-turbo");
        assert_eq!(tool.model_override, Some("gpt-4-turbo".to_string()));
    }

    #[test]
    fn vision_tool_with_description_override() {
        let tool = VisionTool::new("sk-test", VisionBackend::Anthropic)
            .with_description("Custom vision tool for OCR");
        assert_eq!(tool.description, "Custom vision tool for OCR");
    }

    #[test]
    fn vision_tool_register_to_registry() {
        let registry = ToolRegistry::default();
        let tool = VisionTool::new("sk-test", VisionBackend::Openai);
        tool.register(&registry);
        let entry = registry.get("vision_describe").expect("tool registered");
        assert_eq!(entry.schema.name, "vision_describe");
    }
}
