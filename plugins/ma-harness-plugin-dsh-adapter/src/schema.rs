//! dsh `defineTool` schema → ma-harness `ToolSchema` 转换
//!
//! dsh 给的 schema 形式 (per `defineTool`):
//! ```text
//! {
//!     name: "k8s_pod_status",
//!     description: "...",
//!     parameters: {
//!         namespace: { type: "string", required: true, description: "..." },
//!         labelSelector: { type: "string", required: false, description: "..." },
//!     },
//!     output: { schema: {...}, render: ... },
//! }
//! ```
//!
//! ma-harness `ToolSchema` 期望 JSON Schema 形式:
//! ```text
//! {
//!     name: "k8s_pod_status",
//!     description: "...",
//!     parameters: {
//!         type: "object",
//!         properties: { namespace: {...}, labelSelector: {...} },
//!         required: ["namespace"],
//!     },
//! }
//! ```
//!
//! **P13.2**: 简化版转换, 不支持 `oneOf` / `anyOf` / `enum` / `format` 高级字段
//! (P13.4 conformance 跑通就够了, 完整 JSON Schema draft-07 P14+)

#![allow(dead_code)] // P13.2 留白 method 给 P13.4 conformance

use serde_json::{json, Map, Value};

use ma_harness_core::ToolSchema;

use crate::DshToolSchema;

/// dsh 字段 schema 转 JSON Schema
///
/// dsh 字段: `{ type: "string" | "number" | "integer" | "boolean" | "object" | "array", required: bool, description: str, ... }`
/// JSON Schema: `{ type: "...", description: "...", ... }` (required 在外层 properties 旁边的 `required` 数组)
pub fn dsh_field_to_json_schema(field: &Map<String, Value>) -> Value {
    let mut out = Map::new();

    // 1. type
    if let Some(t) = field.get("type").and_then(|v| v.as_str()) {
        out.insert("type".to_string(), Value::String(t.to_string()));
    }

    // 2. description
    if let Some(d) = field.get("description").and_then(|v| v.as_str()) {
        out.insert("description".to_string(), Value::String(d.to_string()));
    }

    // 3. 透传其他字段 (e.g. enum / format / default), P13.4 conformance 验够
    for (k, v) in field.iter() {
        if k == "type" || k == "description" || k == "required" {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }

    Value::Object(out)
}

/// dsh `defineTool` schema 完整转 ma-harness `ToolSchema`
///
/// 转换 record-of-fields → JSON Schema object
pub fn dsh_to_ma_schema(dsh: &DshToolSchema) -> ToolSchema {
    let mut properties = Map::new();
    let mut required: Vec<String> = Vec::new();

    // dsh parameters 可能是:
    // - object: { name: { type, required, ... }, ... }  (record-of-fields, dsh 风格)
    // - object: { type: "object", properties: ..., required: ... }  (已经是 JSON Schema)
    // 我们 P13.2 只处理 record-of-fields (dsh 风格), 真 JSON Schema 透传
    if let Some(params_obj) = dsh.parameters.as_object() {
        if params_obj.get("type").and_then(|v| v.as_str()) == Some("object") {
            // 已经是 JSON Schema, 透传
            ToolSchema {
                name: dsh.name.clone(),
                description: dsh.description.clone(),
                parameters: dsh.parameters.clone(),
            }
        } else {
            // record-of-fields: { field_name: { type, required, ... } }
            for (field_name, field_schema) in params_obj.iter() {
                if let Some(field_obj) = field_schema.as_object() {
                    properties.insert(field_name.clone(), dsh_field_to_json_schema(field_obj));
                    if field_obj
                        .get("required")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        required.push(field_name.clone());
                    }
                } else {
                    // 字段 schema 不是 object, 透传
                    properties.insert(field_name.clone(), field_schema.clone());
                }
            }
            let mut parameters = Map::new();
            parameters.insert("type".to_string(), Value::String("object".to_string()));
            parameters.insert("properties".to_string(), Value::Object(properties));
            if !required.is_empty() {
                parameters.insert(
                    "required".to_string(),
                    Value::Array(required.into_iter().map(Value::String).collect()),
                );
            }
            ToolSchema {
                name: dsh.name.clone(),
                description: dsh.description.clone(),
                parameters: Value::Object(parameters),
            }
        }
    } else {
        // parameters 不是 object, 兜底: 强转 object
        ToolSchema {
            name: dsh.name.clone(),
            description: dsh.description.clone(),
            parameters: json!({
                "type": "object",
                "properties": {},
            }),
        }
    }
}

/// ma-harness 端 schema 校验 (简化版)
/// P13.2 范围: 校验 args 是 object, 必填字段都在
/// P13.4 严格 JSON Schema draft-07 校验 (P14+)
pub fn validate_args_basic(schema: &ToolSchema, args: &Value) -> Result<(), String> {
    // 1. args 必须是 object
    if !args.is_object() {
        return Err(format!("args must be object, got {}", args_type_name(args)));
    }

    // 2. 检查必填字段
    if let Some(params_obj) = schema.parameters.as_object() {
        if let Some(required_arr) = params_obj.get("required").and_then(|v| v.as_array()) {
            let args_obj = args.as_object().unwrap(); // safe, 已 check
            for req in required_arr {
                if let Some(field_name) = req.as_str() {
                    if !args_obj.contains_key(field_name) {
                        return Err(format!("missing required field: {}", field_name));
                    }
                }
            }
        }
    }

    Ok(())
}

fn args_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DshToolSchema;

    #[test]
    fn dsh_record_of_fields_to_json_schema() {
        let dsh = DshToolSchema {
            name: "k8s_pod_status".into(),
            description: "Check pod status".into(),
            parameters: json!({
                "namespace": {
                    "type": "string",
                    "required": true,
                    "description": "k8s namespace",
                },
                "labelSelector": {
                    "type": "string",
                    "required": false,
                    "description": "optional label selector",
                },
            }),
            output_schema: None,
        };
        let ma = dsh_to_ma_schema(&dsh);
        assert_eq!(ma.name, "k8s_pod_status");
        assert_eq!(ma.description, "Check pod status");

        let params = ma.parameters.as_object().unwrap();
        assert_eq!(params.get("type").unwrap(), "object");
        let props = params.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("namespace"));
        assert!(props.contains_key("labelSelector"));
        let ns = props.get("namespace").unwrap().as_object().unwrap();
        assert_eq!(ns.get("type").unwrap(), "string");
        assert_eq!(ns.get("description").unwrap(), "k8s namespace");

        let required = params.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "namespace");
    }

    #[test]
    fn json_schema_passthrough() {
        // dsh parameters 已经是 JSON Schema 时, 透传
        let dsh = DshToolSchema {
            name: "passthrough".into(),
            description: "passthrough test".into(),
            parameters: json!({
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "required": ["x"],
            }),
            output_schema: None,
        };
        let ma = dsh_to_ma_schema(&dsh);
        assert_eq!(ma.parameters, dsh.parameters);
    }

    #[test]
    fn validate_args_required_fields_missing() {
        let schema = ToolSchema {
            name: "test".into(),
            description: "test".into(),
            parameters: json!({
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "required": ["x"],
            }),
        };
        let args = json!({}); // 缺 x
        let result = validate_args_basic(&schema, &args);
        assert!(result.is_err(), "expected error for missing required field");
        assert!(result.unwrap_err().contains("x"));
    }

    #[test]
    fn validate_args_required_fields_present() {
        let schema = ToolSchema {
            name: "test".into(),
            description: "test".into(),
            parameters: json!({
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "required": ["x"],
            }),
        };
        let args = json!({ "x": "ok" });
        let result = validate_args_basic(&schema, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_args_wrong_type() {
        let schema = ToolSchema {
            name: "test".into(),
            description: "test".into(),
            parameters: json!({ "type": "object" }),
        };
        let args = json!("not an object");
        let result = validate_args_basic(&schema, &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be object"));
    }
}
