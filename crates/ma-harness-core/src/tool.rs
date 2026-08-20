//! Tool — model-callable 工具注册表
//!
//! Week 1 Day 9 实现. 设计见 `docs/macro-design.md` §4.
//!
//! P7-3 (Day 101): 工具执行管道升级, 7 阶段 (pre/guard/approval/exec/post/finalize/result),
//! 详见 `tool_pipeline.rs`. `ToolEntry` 加 `config: ToolConfig` 字段支持 timeout / retry
//! / 显式 risk_level 声明.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde_json::Value;

use ma_harness_cordis::{ApprovalDecision, ApprovalRequest, Context, RiskLevel};

pub use crate::tool_pipeline::ToolConfig;

/// Tool schema (喂给 LLM 的)
// P10-1.8 v2: 加 Deserialize (host 端 parse JSON schemas)
#[derive(Debug, Clone, serde::Deserialize)]
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
/// P7-3 改 `Context` → `&Context` (cheap clone for retry).
pub type ToolInvokeFn = Arc<
    dyn Fn(
            Value,
            &Context, // P7-3: by reference 简化 retry
        ) -> futures::future::BoxFuture<'static, anyhow::Result<Value>>
        + Send
        + Sync,
>;

/// Tool entry
#[derive(Clone)]
pub struct ToolEntry {
    pub schema: ToolSchema,
    pub invoke: ToolInvokeFn,
    /// P7-3.2: per-tool config (timeout / retry / risk_level)
    pub config: ToolConfig,
}

impl std::fmt::Debug for ToolEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolEntry")
            .field("schema", &self.schema)
            .field("config", &self.config)
            .field("invoke", &"<fn>")
            .finish()
    }
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

/// P7-2.3 启发式已迁移到 `tool_pipeline::infer_risk_level` (P7-3 收尾).
/// 业务方应显式 `ToolConfig.risk_level`; None 时 tool_pipeline 兜底.
impl ToolRegistry {
    /// 新建
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 tool (默认 config: 无 timeout / 无 retry / 启发式 risk level)
    pub fn register(&self, schema: ToolSchema, invoke: ToolInvokeFn) {
        self.register_with_config(schema, invoke, ToolConfig::default());
    }

    /// 注册一个 tool, 显式声明 config (P7-3.2)
    pub fn register_with_config(
        &self,
        schema: ToolSchema,
        invoke: ToolInvokeFn,
        config: ToolConfig,
    ) {
        let mut inner = self.inner.write();
        inner.insert(
            schema.name.clone(),
            ToolEntry {
                schema,
                invoke,
                config,
            },
        );
    }

    /// 拿 tool entry 引用 (供 `invoke_with_pipeline` 用)
    pub fn get(&self, name: &str) -> Option<ToolEntry> {
        self.inner.read().get(name).cloned()
    }

    /// 调用一个 tool (走默认 pipeline, 无 pre/post hook, 用 invoke_with_pipeline 复用)
    ///
    /// 行为跟 P7-3 前一致 (backward-compat). 业务方想用完整 7-stage pipeline 调
    /// [`crate::tool_pipeline::invoke_with_pipeline`] 自己传 `PipelineConfig`.
    pub async fn invoke(
        &self,
        name: &str,
        args: Value,
        ctx: Context,
    ) -> anyhow::Result<Value> {
        let entry = {
            let inner = self.inner.read();
            inner
                .get(name)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("tool not found: {}", name))?
        };

        // P7-3: 走 invoke_with_pipeline (默认无 hook, 但 timeout/retry/explicit risk_level
        // 走 ToolConfig)
        crate::tool_pipeline::invoke_with_pipeline(
            entry,
            args,
            ctx,
            &crate::tool_pipeline::PipelineConfig::default(),
        )
        .await
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
        _ctx: &Context,
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
