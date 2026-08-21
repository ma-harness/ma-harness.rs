//! 把 dsh 工具注册到 ma-harness `ToolRegistry`
//!
//! P13.2 范围:
//! - `DshAdapter::register_to(registry)`: 拿 list_tools, 给每个 tool 建 ToolEntry
//! - 错误处理: isError / 超时 / 协议错误统一转 `anyhow::Error` (跟 ma-harness-core 现有 API 对齐)
//! - schema 校验: 用 `schema::validate_args_basic` (P13.2 简化版, P14+ 严格 JSON Schema)

use std::sync::Arc;

use futures::future::BoxFuture;
use ma_harness_core::{ToolConfig, ToolInvokeFn, ToolRegistry};
use serde_json::Value;

use crate::jsonrpc::JsonRpcError;
use crate::schema::{dsh_to_ma_schema, validate_args_basic};
use crate::{CallResult, ContentBlock, DshAdapter, DshError, DshToolSchema};

impl DshAdapter {
    /// 把 dsh 子进程 expose 的 tools 注册到 ma-harness `ToolRegistry`
    ///
    /// 接受 `Arc<Self>`: 让 invoke closure (要求 'static) 能 clone Arc 共享 adapter。
    /// 多 tool 注册到同一 registry 时共享同一子进程。
    ///
    /// 流程:
    /// 1. `list_tools()` 拿 dsh 全部 schema
    /// 2. 对每个 tool: 转 dsh schema → ma-harness `ToolSchema`
    /// 3. 构造 `ToolInvokeFn` closure, closure 内调 `adapter.call_tool(name, args)`
    /// 4. `registry.register_with_config(schema, invoke, config)` (用 dsh timeout + retry)
    pub async fn register_to(
        self: Arc<Self>,
        registry: &ToolRegistry,
    ) -> Result<Vec<ma_harness_core::ToolSchema>, DshError> {
        let dsh_tools = self.list_tools().await?;
        let mut ma_schemas = Vec::with_capacity(dsh_tools.len());

        for dsh_tool in &dsh_tools {
            let ma_schema = dsh_to_ma_schema(dsh_tool);
            let name = ma_schema.name.clone();
            let timeout = self.config.timeout;

            // 构造 invoke closure (clone Arc 进 'static closure, 共享子进程)
            let adapter_for_closure = Arc::clone(&self);
            let schema_for_validate = ma_schema.clone();
            let name_for_closure = name.clone();
            let invoke: ToolInvokeFn = Arc::new(move |args: Value, _ctx: &ma_harness_cordis::Context| {
                let adapter = Arc::clone(&adapter_for_closure);
                let schema = schema_for_validate.clone();
                let name = name_for_closure.clone();
                let timeout = timeout;
                Box::pin(async move {
                    // 1. schema 校验 (P13.2 简化版: required field check)
                    if let Err(e) = validate_args_basic(&schema, &args) {
                        return Err(anyhow::anyhow!("schema validation failed: {}", e));
                    }

                    // 2. 调 dsh 工具 (走 JSON-RPC)
                    match adapter.call_tool(&name, args).await {
                        Ok(call_result) => tool_result_to_value(call_result),
                        Err(DshError::JsonRpc(JsonRpcError::Server {
                            code,
                            message,
                            ..
                        })) => Err(anyhow::anyhow!("dsh server error (code {}): {}", code, message)),
                        Err(DshError::Timeout(_)) => {
                            Err(anyhow::anyhow!("tool call timeout after {:?}", timeout))
                        }
                        Err(DshError::PluginCrashed(msg)) => {
                            Err(anyhow::anyhow!("dsh plugin crashed: {}", msg))
                        }
                        Err(e) => Err(anyhow::anyhow!("dsh call failed: {}", e)),
                    }
                })
            });

            let config = ToolConfig {
                timeout: Some(timeout),
                ..Default::default()
            };
            registry.register_with_config(ma_schema.clone(), invoke, config);
            ma_schemas.push(ma_schema);
        }

        Ok(ma_schemas)
    }
}

/// dsh `CallResult` 转 ma-harness `Value` (anyhow::Result)
///
/// 错误处理:
/// - `is_error=true` → anyhow::Error ("dsh tool returned isError: ...")
/// - text content blocks 拼接成 single string
/// - 尝试 parse JSON, 失败 fallback 整段字符串
fn tool_result_to_value(call_result: CallResult) -> anyhow::Result<Value> {
    if call_result.is_error {
        let msg = call_result
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Err(anyhow::anyhow!("dsh tool returned isError: {}", msg));
    }

    // 拼接 text content blocks
    let text = call_result
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    // 尝试 parse 成 JSON value, 失败 fallback string
    serde_json::from_str(&text).or_else(|_| Ok(Value::String(text)))
}

/// helper: dsh tools 列表 → ma-harness `ToolSchema[]` 转换 (无注册, 业务方自己用)
pub fn dsh_tools_to_ma_schemas(dsh_tools: &[DshToolSchema]) -> Vec<ma_harness_core::ToolSchema> {
    dsh_tools.iter().map(dsh_to_ma_schema).collect()
}

#[allow(dead_code, clippy::let_underscore_future)]
fn _box_future_check() {
    // 编译期 check: BoxFuture 签名匹配 ToolInvokeFn
    let _: BoxFuture<'static, anyhow::Result<Value>> = Box::pin(async { Ok(Value::Null) });
}
