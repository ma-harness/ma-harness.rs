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

pub static MAX_DEPTH: ma_harness_cordis::CtxKey<u32> = ctx_key!("max_depth");
pub const DEFAULT_MAX_DEPTH: u32 = 3;

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
}

/// SubagentService ?派生 sub ctx + ?sub agent
pub struct SubagentService;

impl SubagentService {
    /// spawn_agent: ?parent ctx 下派?sub agent, 跑一?prompt
    pub async fn spawn_agent(
        &self,
        parent_ctx: &Context,
        message: &str,
    ) -> Result<SpawnResult, SubagentError> {
        let current_depth = parent_ctx.get(MAX_DEPTH).unwrap_or(0);
        let max_depth = parent_ctx.get(MAX_DEPTH).unwrap_or(DEFAULT_MAX_DEPTH);
        if current_depth >= max_depth {
            return Err(SubagentError::MaxDepthExceeded(max_depth));
        }

        // 派生 sub ctx (继承 parent service 引用)
        let sub_ctx = parent_ctx.fork();
        // ?typed key: sub_depth = current_depth + 1 (递归保护)
        sub_ctx.set(MAX_DEPTH, current_depth + 1);

        // ?AgentLoop (?StubModelAdapter 演示)
        let log = EventLog::open_in_memory()?;
        let adapter: Arc<dyn ModelAdapter> = Arc::new(StubModelAdapter);
        let agent = AgentLoop::new(log, adapter);

        // sub session_id ?parent 不同 (Phase 1 简? ?uuid 后缀)
        let uuid_short = uuid::Uuid::new_v4().to_string();
        let short = &uuid_short[..8];
        let sub_session_id = format!("sub-{}-{}", short, current_depth + 1);

        let req = AgentRunRequest {
            session_id: sub_session_id.clone(),
            user_message: message.to_string(),
            model: "stub".to_string(),
            temperature: 0.7,
            max_tokens: 1024,
            system_prompt: None,
        };
        let resp = agent.run(req).await?;

        Ok(SpawnResult {
            sub_session_id,
            run_id: resp.run_id,
            content: resp.model_response.content,
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
        parent.set(MAX_DEPTH, 0u32);
        parent.set(MAX_DEPTH, 3u32);
        let svc = SubagentService;
        let result = svc.spawn_agent(&parent, "hello").await.unwrap();
        assert!(!result.sub_session_id.is_empty());
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn spawn_respects_max_depth() {
        let parent = Context::new();
        parent.set(MAX_DEPTH, 3u32);
        parent.set(MAX_DEPTH, 3u32); // current = max ?应该拒绝
        let svc = SubagentService;
        let result = svc.spawn_agent(&parent, "hi").await;
        assert!(matches!(result, Err(SubagentError::MaxDepthExceeded(_))));
    }
}
