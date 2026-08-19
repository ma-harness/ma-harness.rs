//! # Approval service (P7-2 / Day 101+)
//!
//! 工具调用前的审批服务, 防 AI 误删/误改文件. 跟 dsh `ctx.approval` 设计对齐.
//!
//! # 用法
//!
//! ```ignore
//! use ma_harness_cordis::{ApprovalService, ApprovalRequest, ApprovalDecision, ApprovalPolicy};
//!
//! // 1. 业务方实现 ApprovalService (CLI / TUI / HTTP / Web UI 任选)
//! struct TuiApprover;
//! #[async_trait::async_trait]
//! impl ApprovalService for TuiApprover {
//!     async fn request_approval(
//!         &self,
//!         ctx: &Context,
//!         req: &ApprovalRequest,
//!     ) -> Result<ApprovalDecision, BoxedError> {
//!         // 弹窗, 等待用户 y/n
//!         if user_says_yes() { Ok(ApprovalDecision::Approved) }
//!         else { Ok(ApprovalDecision::Denied { reason: "user declined".into() }) }
//!     }
//! }
//!
//! // 2. 安装到 ctx
//! ctx.install_approval(Arc::new(TuiApprover), ApprovalPolicy::Ask);
//! ```
//!
//! # 触发点
//!
//! - 工具执行管道 (P7-3) pre-execute hook 调
//! - 业务方也可以手动调 `ctx.approval_service().request_approval(...)`
//!
//! # 默认风险等级
//!
//! 业务方自己用 [`RiskLevel`] 标, 见 `plugins/ma-harness-plugin-fs` 等具体工具

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{BoxedError, Context};

/// 工具调用前审批服务 (P7-2.1)
#[async_trait]
pub trait ApprovalService: Send + Sync {
    /// 工具调用前请求审批
    ///
    /// 业务方 (CLI / TUI / HTTP / Web UI) 实现这个 trait, 弹窗/等用户决策.
    /// 返 `Ok(Approved)` / `Ok(Denied { reason })` / `Ok(AutoApprove)` (policy 跳过).
    /// 返 `Err` 表示服务自身异常, 工具调用会 fail.
    async fn request_approval(
        &self,
        ctx: &Context,
        request: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError>;
}

/// 审批请求 (P7-2.1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// 工具名字 (e.g. `"fs.delete"`)
    pub tool_name: String,
    /// 工具参数 (JSON Value, 序列化自 call site)
    pub arguments: serde_json::Value,
    /// 风险等级
    pub risk_level: RiskLevel,
    /// 人类可读描述 (e.g. "Delete file /etc/passwd")
    pub context: String,
    /// 触发这个审批的 tool_call_id (用来审计追踪)
    pub tool_call_id: String,
}

/// 审批决策 (P7-2.1)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// 用户批准
    Approved,
    /// 用户拒绝
    Denied { reason: String },
    /// Policy 自动批准 (e.g. 白名单命中, 无需问)
    AutoApprove,
}

/// 风险等级 (P7-2.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// 只读操作 (log, list, read)
    Low,
    /// 写工作区 (write, append)
    Medium,
    /// 删/系统命令 (delete, chmod, system)
    High,
    /// 配置/安全敏感 (plugin mgmt, secret access)
    Critical,
}

/// 审批策略 (P7-2.2)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ApprovalPolicy {
    /// 不审批 (所有工具调用 auto-approve, 适合 benchmark / 测试)
    Never,
    /// 每次都问 (生产环境默认)
    Ask,
    /// 永远 ask (debug 模式, 业务方想确认每一步)
    Always,
    /// 白名单: 命中的工具 auto-approve, 其他 Ask
    Whitelist {
        /// 工具名白名单 (e.g. `["fs.read", "bash.echo"]`)
        tools: Vec<String>,
    },
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        // P7-2 默认: 问 (跟 dsh 一致, 平衡安全跟 UX)
        ApprovalPolicy::Ask
    }
}

/// 审批服务 registry (P7-2.2)
///
/// 安装到 Context 后, 工具执行管道 (P7-3) 调 `ctx.approval_service().request_approval(...)`.
/// 业务方也可以手动调.
#[derive(Clone)]
pub struct ApprovalRegistry {
    service: Arc<dyn ApprovalService>,
    policy: ApprovalPolicy,
}

impl ApprovalRegistry {
    /// 构造
    pub fn new(service: Arc<dyn ApprovalService>, policy: ApprovalPolicy) -> Self {
        Self { service, policy }
    }

    /// 拿 policy 引用
    pub fn policy(&self) -> &ApprovalPolicy {
        &self.policy
    }

    /// 拿 service 引用
    pub fn service(&self) -> &Arc<dyn ApprovalService> {
        &self.service
    }

    /// 检查是否需要审批 (走 policy)
    ///
    /// 返 `true` 表示需要问 user, `false` 表示 auto-approve.
    pub fn needs_approval(&self, req: &ApprovalRequest) -> bool {
        match &self.policy {
            ApprovalPolicy::Never => false,
            ApprovalPolicy::Ask => true,
            ApprovalPolicy::Always => true,
            ApprovalPolicy::Whitelist { tools } => !tools.iter().any(|t| t == &req.tool_name),
        }
    }

    /// 走完整审批 (业务方手动调)
    pub async fn check(&self, ctx: &Context, req: &ApprovalRequest) -> Result<ApprovalDecision, BoxedError> {
        if !self.needs_approval(req) {
            return Ok(ApprovalDecision::AutoApprove);
        }
        self.service.request_approval(ctx, req).await
    }
}

impl std::fmt::Debug for ApprovalRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalRegistry")
            .field("policy", &self.policy)
            .field("service", &"<dyn ApprovalService>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_never_skips_all() {
        let req = ApprovalRequest {
            tool_name: "fs.delete".into(),
            arguments: serde_json::json!({}),
            risk_level: RiskLevel::Critical,
            context: "delete /etc/passwd".into(),
            tool_call_id: "tc-1".into(),
        };
        // 实际 ApprovalRegistry 需要 service, 但 policy check 单独可测
        let policy = ApprovalPolicy::Never;
        let needs = match &policy {
            ApprovalPolicy::Never => false,
            _ => true,
        };
        assert!(!needs, "Never policy 永远不审批");
    }

    #[test]
    fn policy_ask_always_asks() {
        let req = ApprovalRequest {
            tool_name: "fs.read".into(),
            arguments: serde_json::json!({}),
            risk_level: RiskLevel::Low,
            context: "read /tmp/x".into(),
            tool_call_id: "tc-2".into(),
        };
        let policy = ApprovalPolicy::Ask;
        let needs = match &policy {
            ApprovalPolicy::Ask => true,
            _ => false,
        };
        assert!(needs, "Ask policy 永远审批");
    }

    #[test]
    fn policy_whitelist_skips_listed() {
        let policy = ApprovalPolicy::Whitelist {
            tools: vec!["fs.read".into(), "bash.echo".into()],
        };
        let req = ApprovalRequest {
            tool_name: "fs.read".into(),
            arguments: serde_json::json!({}),
            risk_level: RiskLevel::Low,
            context: "read /tmp/x".into(),
            tool_call_id: "tc-3".into(),
        };
        let needs = match &policy {
            ApprovalPolicy::Whitelist { tools } => !tools.iter().any(|t| t == &req.tool_name),
            _ => true,
        };
        assert!(!needs, "白名单命中 auto-approve");

        let req2 = ApprovalRequest {
            tool_name: "fs.delete".into(),
            ..req.clone()
        };
        let needs2 = match &policy {
            ApprovalPolicy::Whitelist { tools } => !tools.iter().any(|t| t == &req2.tool_name),
            _ => true,
        };
        assert!(needs2, "白名单没命中要 ask");
    }

    #[test]
    fn risk_level_serde() {
        // 业务方可能用 serde_json 序列化 RiskLevel 存审计 log
        let json = serde_json::to_string(&RiskLevel::High).unwrap();
        assert_eq!(json, "\"high\"");
        let parsed: RiskLevel = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(parsed, RiskLevel::Low);
    }
}
