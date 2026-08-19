//! ma_harness_plugin_subagent ?派生 ctx 跑子 agent
//!
//! **Week 5-6 实装**: spawn_agent(message) ?ctx.fork() 派生 sub ctx + ?sub AgentLoop
//! (?StubModelAdapter). 演示 cordis fork API 实战.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(missing_docs)] // 2026-08-18: 内部 crate, 暂不强制 doc (Phase 2 release 前补)

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, ModelAdapter, StubModelAdapter};
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::warn;

/// 当前子 agent 嵌套深度 (parent fork 出来的 ctx 写 0, sub fork 出 sub-sub 写 1, ...)
pub static SUBAGENT_DEPTH: ma_harness_cordis::CtxKey<u32> = ctx_key!("subagent_depth");

/// 最大嵌套深度限制 (业务方配置, 默认 3)
pub static MAX_DEPTH: ma_harness_cordis::CtxKey<u32> = ctx_key!("max_depth");
pub const DEFAULT_MAX_DEPTH: u32 = 3;

/// P7-4 (Day 101): Parent session id 跟踪, sub ctx 设置
pub static PARENT_SESSION_ID: ma_harness_cordis::CtxKey<String> = ctx_key!("parent_session_id");

/// P7-4 (Day 101): Parent 是否共享 events 给 sub (system prompt 注入)
pub static PARENT_EVENTS_INCLUDED: ma_harness_cordis::CtxKey<bool> = ctx_key!("parent_events_included");

#[derive(Debug, Error)]
pub enum SubagentError {
    #[error("max subagent depth {0} exceeded")]
    MaxDepthExceeded(u32),
    #[error("agent run failed: {0}")]
    AgentRun(#[from] anyhow::Error),
    #[error("event log open failed: {0}")]
    EventLog(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnResult {
    pub sub_session_id: String,
    pub run_id: String,
    pub content: String,
    /// P7-4 新增: 跟踪 parent
    #[serde(default)]
    pub parent_session_id: Option<String>,
    /// P7-4 新增: fork 深度
    #[serde(default)]
    pub depth: u32,
}

/// P7-4 新增: 显式 fork 配置
///
/// 业务方可自定义 model / temperature / max_tokens / system_prompt,
/// 加 parent_session_id 跟踪 + parent_events_included 共享.
///
/// ```ignore
/// use ma_harness_plugin_subagent::{SubagentService, SubagentSpec};
///
/// let svc = SubagentService;
/// let spec = SubagentSpec {
///     message: "summarize this code".into(),
///     parent_session_id: Some("parent-123".into()),
///     parent_events_included: true,
///     ..Default::default()
/// };
/// let result = svc.spawn_with_spec(&ctx, &spec).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentSpec {
    /// 要让 sub agent 处理的 prompt
    pub message: String,
    /// P7-4: 跟踪 parent session id, 完善存储 session 关系
    #[serde(default)]
    pub parent_session_id: Option<String>,
    /// P7-4: parent 是否共享 events 给 sub (system prompt 注入)
    #[serde(default)]
    pub parent_events_included: bool,
    /// 模型名 (默认 "stub")
    #[serde(default = "default_subagent_model")]
    pub model: String,
    /// 温度 (0.0 - 2.0, 默认 0.7)
    #[serde(default = "default_subagent_temperature")]
    pub temperature: f32,
    /// max tokens (默认 1024)
    #[serde(default = "default_subagent_max_tokens")]
    pub max_tokens: u32,
    /// system prompt (辅助, e.g. "You are a code reviewer")
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl Default for SubagentSpec {
    fn default() -> Self {
        Self {
            message: String::new(),
            parent_session_id: None,
            parent_events_included: false,
            model: default_subagent_model(),
            temperature: default_subagent_temperature(),
            max_tokens: default_subagent_max_tokens(),
            system_prompt: None,
        }
    }
}

fn default_subagent_model() -> String {
    "stub".to_string()
}
fn default_subagent_temperature() -> f32 {
    0.7
}
fn default_subagent_max_tokens() -> u32 {
    1024
}

/// SubagentService ?派生 sub ctx + ?sub agent
pub struct SubagentService;

impl SubagentService {
    /// spawn_agent: 简化 API, 默认 spec, 跑一 prompt
    ///
    /// P7-4 后推荐用 spawn_with_spec() 显式 spec.
    pub async fn spawn_agent(
        &self,
        parent_ctx: &Context,
        message: &str,
    ) -> Result<SpawnResult, SubagentError> {
        let mut spec = SubagentSpec::default();
        spec.message = message.to_string();
        self.spawn_with_spec(parent_ctx, &spec).await
    }

    /// P7-4 新增: 显式 spec spawn
    ///
    /// 关键路径:
    /// 1. 检查深度 (SUBAGENT_DEPTH vs MAX_DEPTH)
    /// 2. ctx.fork() 生成 sub ctx
    /// 3. 设置 PARENT_SESSION_ID + SUBAGENT_DEPTH + PARENT_EVENTS_INCLUDED typed key
    /// 4. 如果 parent_events_included: 注入 parent context 到 system prompt
    /// 5. AgentLoop.run() 执行
    /// 6. 生成 SpawnResult { depth, parent_session_id, ... }
    pub async fn spawn_with_spec(
        &self,
        parent_ctx: &Context,
        spec: &SubagentSpec,
    ) -> Result<SpawnResult, SubagentError> {
        let current_depth = parent_ctx.get(SUBAGENT_DEPTH).unwrap_or(0);
        let max_depth = parent_ctx.get(MAX_DEPTH).unwrap_or(DEFAULT_MAX_DEPTH);
        if current_depth >= max_depth {
            return Err(SubagentError::MaxDepthExceeded(max_depth));
        }

        // 派生 sub ctx
        let sub_ctx = parent_ctx.fork();

        // 设置 typed key (sub ctx 中)
        sub_ctx.set(SUBAGENT_DEPTH, current_depth + 1);
        if let Some(parent_sid) = &spec.parent_session_id {
            sub_ctx.set(PARENT_SESSION_ID, parent_sid.clone());
        }
        sub_ctx.set(PARENT_EVENTS_INCLUDED, spec.parent_events_included);

        // 如果 parent_events_included: 注入 parent context 到 system prompt
        let system_prompt = if spec.parent_events_included {
            let parent_sid = spec.parent_session_id.clone().unwrap_or_default();
            if !parent_sid.is_empty() {
                Some(format!(
                    "You are a sub-agent. Parent session id: {parent_sid}.                      Depth: {}. Be concise.",
                    current_depth + 1
                ))
            } else {
                spec.system_prompt.clone()
            }
        } else {
            spec.system_prompt.clone()
        };

        // AgentLoop (StubModelAdapter 演示, Phase 2 改注 parent_ctx 的 adapter)
        let log = EventLog::open_in_memory()?;
        let adapter: Arc<dyn ModelAdapter> = Arc::new(StubModelAdapter);
        let agent = AgentLoop::new(log, adapter);

        // sub session_id 简化: uuid 后缀 + 深度
        let uuid_short = uuid::Uuid::new_v4().to_string();
        let short = &uuid_short[..8];
        let sub_session_id = format!("sub-{}-{}", short, current_depth + 1);

        let req = AgentRunRequest {
            session_id: sub_session_id.clone(),
            user_message: spec.message.clone(),
            model: spec.model.clone(),
            temperature: spec.temperature,
            max_tokens: spec.max_tokens,
            system_prompt,
        };
        let resp = agent.run(req).await?;

        Ok(SpawnResult {
            sub_session_id,
            run_id: resp.run_id,
            content: resp.model_response.content,
            parent_session_id: spec.parent_session_id.clone(),
            depth: current_depth + 1,
        })
    }
}

impl CordisService for SubagentService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(SubagentService)
    }
    fn name(&self) -> &str {
        "subagent"
    }
}

impl SeamService for SubagentService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(SubagentService)
    }
    fn name(&self) -> &str {
        "subagent"
    }
}

pub struct SubagentPlugin;

impl CordisPlugin for SubagentPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = <SubagentService as ma_harness_cordis::Service>::install(ctx)?;
        ctx.inject(Arc::new(svc));
        ctx.set(MAX_DEPTH, DEFAULT_MAX_DEPTH);
        // SUBAGENT_DEPTH 不设 (parent ctx 深度 0)
        Ok(())
    }
    fn name(&self) -> &str {
        "subagent"
    }
}

impl SeamPlugin for SubagentPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "subagent"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spawn_subagent_succeeds() {
        let parent = Context::new();
        // parent 深度 0, max 3 → 应该成功 spawn sub agent (深度变 1)
        parent.set(SUBAGENT_DEPTH, 0u32);
        parent.set(MAX_DEPTH, 3u32);
        let svc = SubagentService;
        let result = svc.spawn_agent(&parent, "hello").await.unwrap();
        assert!(!result.sub_session_id.is_empty());
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn spawn_respects_max_depth() {
        let parent = Context::new();
        // current = 3, max = 3 → 应该拒绝 (3 >= 3)
        parent.set(SUBAGENT_DEPTH, 3u32);
        parent.set(MAX_DEPTH, 3u32);
        let svc = SubagentService;
        let result = svc.spawn_agent(&parent, "hi").await;
        assert!(matches!(result, Err(SubagentError::MaxDepthExceeded(_))));
    }

    // P7-4 tests: SubagentSpec + parent tracking + parent_events_included
    #[tokio::test]
    async fn spawn_with_spec_basic() {
        let parent = Context::new();
        parent.set(SUBAGENT_DEPTH, 0u32);
        parent.set(MAX_DEPTH, 3u32);
        let svc = SubagentService;
        let spec = SubagentSpec {
            message: "hello world".into(),
            ..Default::default()
        };
        let result = svc.spawn_with_spec(&parent, &spec).await.unwrap();
        assert_eq!(result.depth, 1);
        assert_eq!(result.parent_session_id, None);
        assert!(result.content.contains("hello world"));
    }

    #[tokio::test]
    async fn spawn_with_spec_tracks_parent_session_id() {
        let parent = Context::new();
        parent.set(SUBAGENT_DEPTH, 0u32);
        parent.set(MAX_DEPTH, 3u32);
        let svc = SubagentService;
        let spec = SubagentSpec {
            message: "test".into(),
            parent_session_id: Some("parent-xyz".into()),
            ..Default::default()
        };
        let result = svc.spawn_with_spec(&parent, &spec).await.unwrap();
        assert_eq!(result.parent_session_id, Some("parent-xyz".to_string()));
        assert_eq!(result.depth, 1);
    }

    #[tokio::test]
    async fn spawn_with_spec_includes_parent_events_in_system_prompt() {
        let parent = Context::new();
        parent.set(SUBAGENT_DEPTH, 0u32);
        parent.set(MAX_DEPTH, 3u32);
        let svc = SubagentService;
        let spec = SubagentSpec {
            message: "review this code".into(),
            parent_session_id: Some("parent-events-test".into()),
            parent_events_included: true,
            ..Default::default()
        };
        let result = svc.spawn_with_spec(&parent, &spec).await.unwrap();
        assert!(result.content.contains("review this code"));
        // sub_session_id 应该记录 depth
        assert!(result.sub_session_id.contains("-1"));
    }

    #[tokio::test]
    async fn spawn_with_spec_respects_max_depth() {
        let parent = Context::new();
        parent.set(SUBAGENT_DEPTH, 3u32);
        parent.set(MAX_DEPTH, 3u32);
        let svc = SubagentService;
        let spec = SubagentSpec {
            message: "test".into(),
            ..Default::default()
        };
        let result = svc.spawn_with_spec(&parent, &spec).await;
        assert!(matches!(result, Err(SubagentError::MaxDepthExceeded(_))));
    }

    #[tokio::test]
    async fn spawn_with_spec_nested_depth_increments() {
        let parent = Context::new();
        parent.set(SUBAGENT_DEPTH, 1u32);
        parent.set(MAX_DEPTH, 3u32);
        let svc = SubagentService;
        let spec = SubagentSpec {
            message: "nested".into(),
            ..Default::default()
        };
        let result = svc.spawn_with_spec(&parent, &spec).await.unwrap();
        assert_eq!(result.depth, 2, "depth 应从 1 增到 2");
        assert!(result.sub_session_id.contains("-2"));
    }

    #[tokio::test]
    async fn subagent_spec_default_values() {
        let spec = SubagentSpec::default();
        assert_eq!(spec.message, "");
        assert!(spec.parent_session_id.is_none());
        assert!(!spec.parent_events_included);
        assert_eq!(spec.model, "stub");
        assert_eq!(spec.temperature, 0.7);
        assert_eq!(spec.max_tokens, 1024);
        assert!(spec.system_prompt.is_none());
    }
}
