//! Tool execution pipeline (P7-3 / Day 101)
//!
//! 7-stage pipeline 走 `ToolRegistry::invoke_with_pipeline`:
//! 1. **pre-execute**  — 业务方 pre-hook (e.g. 写 start event, args validation)
//! 2. **guard**        — 沙箱/路径检查 (P5 已加 sandbox, 此处 hook 点)
//! 3. **approval**     — `ctx.approval().check()` (P7-2.3 已集成)
//! 4. **execute**      — 调 tool with `timeout` + `retry`
//! 5. **post**         — 业务方 post-hook (e.g. 写 result event, 错误规范化)
//! 6. **finalize**     — cleanup, 审计
//! 7. **result**       — 返 `Result<Value>` 给 caller
//!
//! ## per-tool config
//!
//! 每个 tool 注册时可声明 `ToolConfig`:
//! - `timeout: Option<Duration>` — execute 阶段超时
//! - `retry: Option<RetryPolicy>` — 失败重试 (max_attempts + backoff)
//! - `risk_level: Option<RiskLevel>` — 审批风险等级 (P7-2.3 启发式 fallback)
//!
//! ## Backward compat
//!
//! `ToolRegistry::invoke()` 走默认 pipeline (无 pre/post hook, 用 infer_risk_level,
//! 无 timeout, 无 retry). 业务方想用完整控制调 `invoke_with_pipeline`.
//!
//! ## 设计
//!
//! 内部用 `Arc<Context>` (因为 `Context` 不可 Clone, 多个 stage 要 share ref).
//! ToolInvokeFn 改 `Fn(Value, &Context)` 让 retry cheap 复 ctx.

use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use serde_json::Value;

use ma_harness_cordis::{ApprovalDecision, ApprovalRequest, Context, RiskLevel};

use crate::tool::ToolEntry;

/// Per-tool config (P7-3.2)
#[derive(Debug, Clone, Default)]
pub struct ToolConfig {
    /// Execute 阶段超时 (None = 无超时, 等到底)
    pub timeout: Option<Duration>,
    /// 失败重试策略 (None = 不重试, 跟旧 invoke 行为一致)
    pub retry: Option<RetryPolicy>,
    /// 显式声明风险等级 (None = 走 infer_risk_level 启发式)
    pub risk_level: Option<RiskLevel>,
}

/// 重试策略 (P7-3.2)
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// 最多尝试次数 (含第 1 次, e.g. 3 = 1 次 + 2 次重试)
    pub max_attempts: u32,
    /// 初始 backoff (ms)
    pub initial_backoff_ms: u64,
    /// backoff 倍数 (e.g. 2.0 = 指数退避)
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 100,
            backoff_multiplier: 2.0,
        }
    }
}

/// Pipeline invoke 上下文 (P7-3.1)
#[derive(Debug, Clone)]
pub struct InvokeContext {
    /// Tool 名字
    pub tool_name: String,
    /// Tool 参数
    pub args: Value,
    /// Tool entry (含 schema + config)
    pub entry: ToolEntry,
    /// Cordis ctx (Arc-wrapped, 因为 Context 不可 Clone)
    pub ctx: Arc<Context>,
    /// Tool call id (审计追踪)
    pub tool_call_id: String,
}

/// Pipeline 阶段结果 (P7-3.1)
#[derive(Debug)]
pub enum PipelineStage {
    /// 成功, 继续
    Continue,
    /// 终止, 跳到 result
    ShortCircuit(anyhow::Result<Value>),
}

/// Pre-hook: 业务方在 execute 前跑 (P7-3.3)
pub type PreHookFn =
    Arc<dyn Fn(&InvokeContext) -> BoxFuture<'static, anyhow::Result<PipelineStage>> + Send + Sync>;

/// Post-hook: 业务方在 execute 后跑 (成功/失败都跑) (P7-3.3)
///
/// result 走 `Arc<InnerResult>` (cheap clone), `anyhow::Error` 不可 Clone, 所以包一层.
pub type PostHookFn =
    Arc<dyn Fn(&InvokeContext, Arc<InnerResult>) -> BoxFuture<'static, ()> + Send + Sync>;

/// Inner result (P7-3.3): `Ok(Value)` 或 `Err(String)` (anyhow 转 String, 避免 Clone 问题)
#[derive(Debug, Clone)]
pub enum InnerResult {
    /// 成功
    Ok(Value),
    /// 失败 (anyhow::Error → String)
    Err(String),
}

impl InnerResult {
    /// 构造 (从 anyhow::Result<Value>)
    pub fn from_anyhow(r: anyhow::Result<Value>) -> Self {
        match r {
            Ok(v) => Self::Ok(v),
            Err(e) => Self::Err(format!("{e:#}")),
        }
    }

    /// 是否成功
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    /// 是否失败
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }
}

/// Pipeline 配置 (P7-3.3)
#[derive(Default, Clone)]
pub struct PipelineConfig {
    /// pre-execute hooks (按调用顺序)
    pub pre_hooks: Vec<PreHookFn>,
    /// post hooks (按调用顺序)
    pub post_hooks: Vec<PostHookFn>,
}

/// 7-stage pipeline 入口 (P7-3.1)
///
/// 跟 `ToolRegistry::invoke` 的区别:
/// - 走 pre/post hooks
/// - 用 `entry.config.risk_level` 优先, fallback `infer_risk_level`
/// - `timeout` + `retry` 走 `ToolConfig`
pub async fn invoke_with_pipeline(
    entry: ToolEntry,
    args: Value,
    ctx: Context,
    pipeline: &PipelineConfig,
) -> anyhow::Result<Value> {
    let tool_name = entry.schema.name.clone();
    let tool_call_id = uuid::Uuid::new_v4().to_string();
    let ctx_arc = Arc::new(ctx);
    let invoke_ctx = InvokeContext {
        tool_name: tool_name.clone(),
        args: args.clone(),
        entry: entry.clone(),
        ctx: ctx_arc.clone(),
        tool_call_id: tool_call_id.clone(),
    };

    // Stage 1: pre-execute hooks
    for pre in &pipeline.pre_hooks {
        match pre(&invoke_ctx).await? {
            PipelineStage::Continue => continue,
            PipelineStage::ShortCircuit(r) => return r,
        }
    }

    // Stage 2: guard (TODO P7-3.5: 集成 sandbox check, 走 ctx)
    // v1 简化: 跳过, 留 hook 点

    // Stage 3: approval (走 ctx.approval().check)
    if let Some(approval) = ctx_arc.approval() {
        let risk_level = entry
            .config
            .risk_level
            .unwrap_or_else(|| infer_risk_level(&tool_name));
        let req = ApprovalRequest {
            tool_name: tool_name.clone(),
            arguments: args.clone(),
            risk_level,
            context: format!("invoke tool: {tool_name}"),
            tool_call_id: tool_call_id.clone(),
        };
        match approval.check(&ctx_arc, &req).await {
            Ok(ApprovalDecision::Approved | ApprovalDecision::AutoApprove) => {}
            Ok(ApprovalDecision::Denied { reason }) => {
                let ir = Arc::new(InnerResult::Err(format!("approval denied: {reason}")));
                run_post_hooks(&invoke_ctx, ir, &pipeline.post_hooks).await;
                return Err(anyhow::anyhow!("approval denied: {reason}"));
            }
            Err(e) => {
                let ir = Arc::new(InnerResult::Err(format!("approval service error: {e}")));
                run_post_hooks(&invoke_ctx, ir, &pipeline.post_hooks).await;
                return Err(anyhow::anyhow!("approval service error: {e}"));
            }
        }
    }

    // Stage 4: execute (with timeout + retry)
    let result = execute_with_retry(&entry, args, &ctx_arc).await;

    // Stage 5 + 6: post + finalize
    let ir = Arc::new(match &result {
        Ok(v) => InnerResult::Ok(v.clone()),
        Err(e) => InnerResult::Err(format!("{e:#}")),
    });
    run_post_hooks(&invoke_ctx, ir, &pipeline.post_hooks).await;

    // Stage 7: result
    result
}

/// Stage 4: execute with optional timeout + retry
async fn execute_with_retry(
    entry: &ToolEntry,
    args: Value,
    ctx: &Arc<Context>,
) -> anyhow::Result<Value> {
    let max_attempts = entry.config.retry.map(|r| r.max_attempts).unwrap_or(1);
    let mut backoff_ms = entry
        .config
        .retry
        .map(|r| r.initial_backoff_ms)
        .unwrap_or(0);
    let backoff_mult = entry
        .config
        .retry
        .map(|r| r.backoff_multiplier)
        .unwrap_or(1.0);

    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=max_attempts {
        let fut = (entry.invoke)(args.clone(), ctx);
        let attempt_result = if let Some(timeout) = entry.config.timeout {
            match tokio::time::timeout(timeout, fut).await {
                Ok(r) => r,
                Err(_) => Err(anyhow::anyhow!(
                    "tool '{}' timeout after {:?}",
                    entry.schema.name,
                    timeout
                )),
            }
        } else {
            fut.await
        };

        match attempt_result {
            Ok(v) => return Ok(v),
            Err(e) => {
                if attempt >= max_attempts {
                    return Err(e);
                }
                last_err = Some(e);
                if backoff_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms as f64 * backoff_mult) as u64;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("retry exhausted")))
}

async fn run_post_hooks(
    invoke_ctx: &InvokeContext,
    result: Arc<InnerResult>,
    hooks: &[PostHookFn],
) {
    for post in hooks {
        post(invoke_ctx, result.clone()).await;
    }
}

/// P7-2.3 启发式: 工具名匹配 risk level (fallback when ToolConfig 没声明)
pub fn infer_risk_level(tool_name: &str) -> RiskLevel {
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
        RiskLevel::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;

    type InvokeFn =
        dyn Fn(Value, &Context) -> BoxFuture<'static, anyhow::Result<Value>> + Send + Sync;

    fn stub_invoke(_args: Value, _ctx: &Context) -> BoxFuture<'static, anyhow::Result<Value>> {
        Box::pin(async move { Ok(Value::String("ok".to_string())) })
    }

    fn slow_invoke(_args: Value, _ctx: &Context) -> BoxFuture<'static, anyhow::Result<Value>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            Ok(Value::String("slow".to_string()))
        })
    }

    fn fail_invoke(_args: Value, _ctx: &Context) -> BoxFuture<'static, anyhow::Result<Value>> {
        Box::pin(async move { Err(anyhow::anyhow!("tool failed")) })
    }

    fn entry(name: &str, invoke: Arc<InvokeFn>) -> ToolEntry {
        ToolEntry {
            schema: crate::tool::ToolSchema {
                name: name.to_string(),
                description: format!("{name} tool"),
                parameters: serde_json::json!({}),
            },
            invoke,
            config: ToolConfig::default(),
        }
    }

    #[tokio::test]
    async fn pipeline_basic_continue() {
        let e = entry("echo", Arc::new(stub_invoke));
        let result = invoke_with_pipeline(e, json!({}), Context::new(), &PipelineConfig::default())
            .await
            .unwrap();
        assert_eq!(result, "ok");
    }

    #[tokio::test]
    async fn pipeline_short_circuit_pre_hook() {
        // pre-hook 直接返结果, 跳过 execute
        let e = entry("echo", Arc::new(stub_invoke));
        let pre: PreHookFn = Arc::new(|_ctx| {
            Box::pin(async move {
                Ok(PipelineStage::ShortCircuit(Ok(Value::String(
                    "intercepted".to_string(),
                ))))
            })
        });
        let pipeline = PipelineConfig {
            pre_hooks: vec![pre],
            post_hooks: vec![],
        };
        let result = invoke_with_pipeline(e, json!({}), Context::new(), &pipeline)
            .await
            .unwrap();
        assert_eq!(result, "intercepted");
    }

    #[tokio::test]
    async fn pipeline_post_hook_runs_on_success() {
        let e = entry("echo", Arc::new(stub_invoke));
        let counter = Arc::new(parking_lot::Mutex::new(0u32));
        let c2 = counter.clone();
        let post: PostHookFn = Arc::new(move |_ctx, result| {
            let c = c2.clone();
            Box::pin(async move {
                if result.is_ok() {
                    *c.lock() += 1;
                }
            })
        });
        let pipeline = PipelineConfig {
            pre_hooks: vec![],
            post_hooks: vec![post],
        };
        invoke_with_pipeline(e, json!({}), Context::new(), &pipeline)
            .await
            .unwrap();
        assert_eq!(*counter.lock(), 1);
    }

    #[tokio::test]
    async fn pipeline_post_hook_runs_on_failure() {
        let e = entry("fail", Arc::new(fail_invoke));
        let counter = Arc::new(parking_lot::Mutex::new(0u32));
        let c2 = counter.clone();
        let post: PostHookFn = Arc::new(move |_ctx, result| {
            let c = c2.clone();
            Box::pin(async move {
                if result.is_err() {
                    *c.lock() += 1;
                }
            })
        });
        let pipeline = PipelineConfig {
            pre_hooks: vec![],
            post_hooks: vec![post],
        };
        let _ = invoke_with_pipeline(e, json!({}), Context::new(), &pipeline).await;
        assert_eq!(*counter.lock(), 1, "post hook 应在 execute fail 后跑");
    }

    #[tokio::test]
    async fn pipeline_timeout_triggers_error() {
        let mut e = entry("slow", Arc::new(slow_invoke));
        e.config.timeout = Some(Duration::from_millis(50));
        let result =
            invoke_with_pipeline(e, json!({}), Context::new(), &PipelineConfig::default()).await;
        let err = result.unwrap_err();
        assert!(err.to_string().contains("timeout"), "got: {err}");
    }

    #[tokio::test]
    async fn pipeline_retry_recovers_on_transient_fail() {
        // 第 2 次才成功 (模拟 transient fail)
        let counter = Arc::new(parking_lot::Mutex::new(0u32));
        let c2 = counter.clone();
        let flaky: Arc<InvokeFn> = Arc::new(move |args, ctx| {
            let c = c2.clone();
            Box::pin(async move {
                let mut count = c.lock();
                *count += 1;
                if *count < 2 {
                    Err(anyhow::anyhow!("transient"))
                } else {
                    Ok(Value::String("recovered".to_string()))
                }
            })
        });
        let e = ToolEntry {
            schema: crate::tool::ToolSchema {
                name: "flaky".to_string(),
                description: "flaky tool".to_string(),
                parameters: json!({}),
            },
            invoke: flaky,
            config: ToolConfig {
                retry: Some(RetryPolicy {
                    max_attempts: 3,
                    initial_backoff_ms: 1, // 测试用 1ms 加速
                    backoff_multiplier: 1.0,
                }),
                ..Default::default()
            },
        };
        let result = invoke_with_pipeline(e, json!({}), Context::new(), &PipelineConfig::default())
            .await
            .unwrap();
        assert_eq!(result, "recovered");
    }

    #[tokio::test]
    async fn pipeline_retry_exhausted_returns_last_error() {
        let e = ToolEntry {
            schema: crate::tool::ToolSchema {
                name: "fail".to_string(),
                description: "always fail".to_string(),
                parameters: json!({}),
            },
            invoke: Arc::new(fail_invoke),
            config: ToolConfig {
                retry: Some(RetryPolicy {
                    max_attempts: 2,
                    initial_backoff_ms: 1,
                    backoff_multiplier: 1.0,
                }),
                ..Default::default()
            },
        };
        let result =
            invoke_with_pipeline(e, json!({}), Context::new(), &PipelineConfig::default()).await;
        let err = result.unwrap_err();
        assert!(err.to_string().contains("tool failed"));
    }
}

use serde_json::json;
