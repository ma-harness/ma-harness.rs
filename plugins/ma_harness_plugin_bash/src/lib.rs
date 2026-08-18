//! ma_harness_plugin_bash — first-party plugin
//!
//! 执行 shell 命令 (受 sandbox 限制)
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

pub static MAX_RUNTIME_MS: ma_harness_cordis::CtxKey<u32> = ctx_key!("max_runtime_ms");

// ============================================================================
// Service: BashService
// ============================================================================

/// BashService (Week 3 占位, Week 5-6 实装)
pub struct BashService;

impl CordisService for BashService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(BashService)
    }
    fn name(&self) -> &str {
        "bash"
    }
}

impl SeamService for BashService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(BashService)
    }
    fn name(&self) -> &str {
        "bash"
    }
}

// ============================================================================
// Plugin: BashPlugin
// ============================================================================

/// BashPlugin
pub struct BashPlugin;

impl CordisPlugin for BashPlugin {
    fn install(&self, _ctx: &Context) -> anyhow::Result<()> {
        // Week 5-6 实装业务逻辑
        Ok(())
    }
    fn name(&self) -> &str {
        "bash"
    }
}

impl SeamPlugin for BashPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "bash"
    }
}
