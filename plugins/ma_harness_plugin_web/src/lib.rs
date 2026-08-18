//! ma_harness_plugin_web — first-party plugin
//!
//! HTTP / HTTPS 请求 (受 sandbox 限制)
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

pub static EGRESS_ALLOW_LIST: ma_harness_cordis::CtxKey<Vec<String>> = ctx_key!("egress_allow_list");
pub static TIMEOUT_MS: ma_harness_cordis::CtxKey<u32> = ctx_key!("timeout_ms");

// ============================================================================
// Service: WebService
// ============================================================================

/// WebService (Week 3 占位, Week 5-6 实装)
pub struct WebService;

impl CordisService for WebService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(WebService)
    }
    fn name(&self) -> &str {
        "web"
    }
}

impl SeamService for WebService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(WebService)
    }
    fn name(&self) -> &str {
        "web"
    }
}

// ============================================================================
// Plugin: WebPlugin
// ============================================================================

/// WebPlugin
pub struct WebPlugin;

impl CordisPlugin for WebPlugin {
    fn install(&self, _ctx: &Context) -> anyhow::Result<()> {
        // Week 5-6 实装业务逻辑
        Ok(())
    }
    fn name(&self) -> &str {
        "web"
    }
}

impl SeamPlugin for WebPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "web"
    }
}
