//! ma_harness_plugin_cordis — first-party plugin
//!
//! meta 插件, 暴露 ctx 自身能力 (自省 / 调试用)
//!
//! **设计**: seam 公开 API 风格 (跟 hello plugin 一致), impl cordis::Service/Plugin
//! 跟 ctx 内部对接, 业务方视角走 ma_harness_seam.
//!
//! **实现状态**: Week 3 骨架 (typed key + 占位 service + plugin). Week 5-6 实装业务逻辑.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};

// ============================================================================
// 公开 typed key (业务方可以 set 覆盖默认)
// ============================================================================

pub static INSPECT_DEPTH: ma_harness_cordis::CtxKey<u32> = ctx_key!("inspect_depth");

// ============================================================================
// Service: CordisService
// ============================================================================

/// CordisService (Week 3 占位, Week 5-6 实装)
pub struct CordisService;

impl CordisService for CordisService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(CordisService)
    }
    fn name(&self) -> &str {
        "cordis"
    }
}

impl SeamService for CordisService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(CordisService)
    }
    fn name(&self) -> &str {
        "cordis"
    }
}

// ============================================================================
// Plugin: CordisPlugin
// ============================================================================

/// CordisPlugin
pub struct CordisPlugin;

impl CordisPlugin for CordisPlugin {
    fn install(&self, _ctx: &Context) -> anyhow::Result<()> {
        // Week 5-6 实装业务逻辑
        Ok(())
    }
    fn name(&self) -> &str {
        "cordis"
    }
}

impl SeamPlugin for CordisPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "cordis"
    }
}
