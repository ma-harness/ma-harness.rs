//! ma_harness_plugin_cordis ?meta 插件, 暴露 ctx 自身能力

#![deny(unsafe_code)]
#![warn(missing_docs)]

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPluginTrait;
use ma_harness_cordis::Service as CordisServiceTrait;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

pub static INSPECT_DEPTH: ma_harness_cordis::CtxKey<u32> = ctx_key!("inspect_depth");
pub const DEFAULT_INSPECT_DEPTH: u32 = 2;

#[derive(Debug, Error)]
pub enum CordisError {
    #[error("inspect depth exceeded: {0}")]
    DepthExceeded(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxSnapshot {
    pub plugin_count: usize,
    pub plugins: Vec<String>,
    pub storage_keys: Vec<String>, // Phase 1 stub
    pub services: Vec<String>,    // Phase 1 stub
    pub is_disposed: bool,
}

/// CordisService ?暴露 ctx 反射能力
pub struct CordisService;

impl CordisService {
    pub fn inspect(&self, ctx: &Context) -> Result<CtxSnapshot, CordisError> {
        let depth = ctx.get(INSPECT_DEPTH).unwrap_or(DEFAULT_INSPECT_DEPTH);
        if depth == 0 {
            return Err(CordisError::DepthExceeded(0));
        }
        let plugins = ctx.plugins();
        let is_disposed = ctx.is_disposed();
        Ok(CtxSnapshot {
            plugin_count: plugins.len(),
            plugins,
            storage_keys: Vec::new(),
            services: Vec::new(),
            is_disposed,
        })
    }
}

impl CordisServiceTrait for CordisService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(CordisService)
    }
    fn name(&self) -> &str {
        "cordis"
    }
}

impl SeamService for CordisService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(CordisService)
    }
    fn name(&self) -> &str {
        "cordis"
    }
}

pub struct CordisPlugin;

impl CordisPluginTrait for CordisPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        // 2026-08-18: fully-qualified 消歧义 (CordisService + SeamService 都有同名 install)
        let svc = <CordisService as ma_harness_cordis::Service>::install(ctx)?;
        ctx.inject(Arc::new(svc));
        ctx.set(INSPECT_DEPTH, DEFAULT_INSPECT_DEPTH);
        Ok(())
    }
    fn name(&self) -> &str {
        "cordis"
    }
}

impl SeamPlugin for CordisPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPluginTrait>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "cordis"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_empty_ctx() {
        let ctx = Context::new();
        ctx.set(INSPECT_DEPTH, 2u32);
        let svc = CordisService;
        let snap = svc.inspect(&ctx).unwrap();
        assert_eq!(snap.plugin_count, 0);
        assert!(snap.plugins.is_empty());
        assert!(!snap.is_disposed);
    }

    #[test]
    fn inspect_depth_zero_errors() {
        let ctx = Context::new();
        ctx.set(INSPECT_DEPTH, 0u32);
        let svc = CordisService;
        let result = svc.inspect(&ctx);
        assert!(matches!(result, Err(CordisError::DepthExceeded(_))));
    }
}
