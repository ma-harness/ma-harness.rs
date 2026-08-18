//! ma_harness_plugin_fs — first-party plugin
//!
//! 文件系统读 / 写 (受 sandbox 限制)
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

pub static READ_ALLOW_LIST: ma_harness_cordis::CtxKey<Vec<String>> = ctx_key!("read_allow_list");
pub static WRITE_ALLOW_LIST: ma_harness_cordis::CtxKey<Vec<String>> = ctx_key!("write_allow_list");

// ============================================================================
// Service: FsService
// ============================================================================

/// FsService (Week 3 占位, Week 5-6 实装)
pub struct FsService;

impl CordisService for FsService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(FsService)
    }
    fn name(&self) -> &str {
        "fs"
    }
}

impl SeamService for FsService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(FsService)
    }
    fn name(&self) -> &str {
        "fs"
    }
}

// ============================================================================
// Plugin: FsPlugin
// ============================================================================

/// FsPlugin
pub struct FsPlugin;

impl CordisPlugin for FsPlugin {
    fn install(&self, _ctx: &Context) -> anyhow::Result<()> {
        // Week 5-6 实装业务逻辑
        Ok(())
    }
    fn name(&self) -> &str {
        "fs"
    }
}

impl SeamPlugin for FsPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "fs"
    }
}
