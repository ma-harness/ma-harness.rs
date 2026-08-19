//! Tool — model-callable 工具注册表
//!
//! Week 1 Day 9 实现. 设计见 `docs/macro-design.md` §4.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde_json::Value;

use ma_harness_cordis::{ApprovalDecision, ApprovalRequest, Context, RiskLevel};

/// Tool schema (喂给 LLM 的)
#[derive(Debug, Clone)]
pub struct ToolSchema {
    /// 工具名
    pub name: String,
    /// 一句话描述
    pub description: String,
    /// JSON Schema (parameters)
    pub parameters: Value,
}

/// Tool invoke 函数签名
///
/// 实际签名: `async fn(args: Value, ctx: &Context) -> Result<Value>`
/// 装箱为 `Box<dyn Fn(...) -> Pin<Box<dyn Future<...>...>> + Send + Sync>`.
pub type ToolInvokeFn = Arc<
    dyn Fn(
            Value,
            Context, // by value 简化, Phase 2 改 Arc<Context>
        ) -> futures::future::BoxFuture<'static, anyhow::Result<Value>>
        + Send
        + Sync,
>;

/// Tool entry
pub struct ToolEntry {
    pub schema: ToolSchema,
    pub invoke: ToolInvokeFn,
}

/// ToolRegistry — 工具注册表
#[derive(Default)]
pub struct ToolRegistry {
    inner: RwLock<HashMap<String, ToolEntry>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("count", &self.inner.read().len())
            .finish()
    }
}

/// P7-2.3 启发式: 工具名匹配 risk level
/// TODO(P7-3): 工具注册时声明 `risk_level`, 走 `ToolEntry.risk_level` 字段
fn infer_risk_level(tool_name: &str) -> RiskLevel {
    if tool_name.contains("delete") || tool_name.contains("rm") || tool_name.contains("chmod") {
        RiskLevel::High
    } else if tool_name.contains("write")
        || tool_name.contains("append")
        || tool_name.contains("edit")
        || tool_name.contains("create")
    {
        RiskLevel::Medium
    } else if tool_name.contains("plugin") || tool_name.contains("config") {
        RiskLevel::Critical
    } else {
        // read / list / search / log / echo / fetch 等只读
        RiskLevel::Low
    }
}
impl ToolRegistry {
    /// 新建
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 tool
    pub fn register(&self, schema: ToolSchema, invoke: ToolInvokeFn) {
        let mut inner = self.inner.write();
        inner.insert(schema.name.clone(), ToolEntry { schema, invoke });
    }

    /// 调用一个 tool
    pub async fn invoke(
        &self,
        name: &str,
        args: Value,
        ctx: Context,
    ) -> anyhow::Result<Value> {
        let invoke = {
            let inner = self.inner.read();
            inner
                .get(name)
                .map(|e| e.invoke.clone())
                .ok_or_else(|| anyhow::anyhow!("tool not found: {}", name))?
        };

        // P7-2.3: pre-execute approval hook (走 ctx.approval().check)
        // 业务方装了 approval registry 才走审批; 没装 → auto-approve (backward-compat)
        if let Some(approval) = ctx.approval() {
            use ma_harness_cordis::{ApprovalDecision, ApprovalRequest, RiskLevel};
            // 简化: 风险等级跟 tool name 启发式匹配 (P7-3 工具会自带 risk level)
            // TODO(P7-3): 工具注册时声明 risk_level, 走 ToolEntry.risk_level
            let risk_level = infer_risk_level(name);
            let req = ApprovalRequest {
                tool_name: name.to_string(),
                arguments: args.clone(),
                risk_level,
                context: format!("invoke tool: {name}"),
                tool_call_id: uuid::Uuid::new_v4().to_string(),
            };
            match approval.check(&ctx, &req).await {
                Ok(ApprovalDecision::Approved | ApprovalDecision::AutoApprove) => {
                    // 继续 invoke
                }
                Ok(ApprovalDecision::Denied { reason }) => {
                    return Err(anyhow::anyhow!("approval denied: {reason}"));
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("approval service error: {e}"));
                }
            }
        }

        invoke(args, ctx).await
    }



    /// 列出所有 tool 的 schema
    pub fn list_schemas(&self) -> Vec<ToolSchema> {
        self.inner
            .read()
            .values()
            .map(|e| e.schema.clone())
            .collect()
    }

    /// 数量
    pub fn count(&self) -> usize {
        self.inner.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;

    fn stub_invoke(
        _args: Value,
        _ctx: Context,
    ) -> BoxFuture<'static, anyhow::Result<Value>> {
        Box::pin(async move { Ok(Value::String("ok".to_string())) })
    }

    #[test]
    fn register_and_count() {
        let r = ToolRegistry::new();
        assert_eq!(r.count(), 0);
        r.register(
            ToolSchema {
                name: "test".to_string(),
                description: "test tool".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
            Arc::new(stub_invoke),
        );
        assert_eq!(r.count(), 1);
    }

    #[test]
    fn list_schemas_returns_all() {
        let r = ToolRegistry::new();
        r.register(
            ToolSchema {
                name: "a".to_string(),
                description: "a".to_string(),
                parameters: serde_json::json!({}),
            },
            Arc::new(stub_invoke),
        );
        r.register(
            ToolSchema {
                name: "b".to_string(),
                description: "b".to_string(),
                parameters: serde_json::json!({}),
            },
            Arc::new(stub_invoke),
        );
        let schemas = r.list_schemas();
        assert_eq!(schemas.len(), 2);
    }

    #[tokio::test]
    async fn invoke_calls_registered_tool() {
        let r = ToolRegistry::new();
        r.register(
            ToolSchema {
                name: "echo".to_string(),
                description: "echo".to_string(),
                parameters: serde_json::json!({}),
            },
            Arc::new(stub_invoke),
        );
        let result = r
            .invoke("echo", serde_json::json!({}), Context::new())
            .await
            .unwrap();
        assert_eq!(result, Value::String("ok".to_string()));
    }

    #[tokio::test]
    async fn invoke_unknown_tool_errors() {
        let r = ToolRegistry::new();
        let result = r
            .invoke("nonexistent", serde_json::json!({}), Context::new())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
