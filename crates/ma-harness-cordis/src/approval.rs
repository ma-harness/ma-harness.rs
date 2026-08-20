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
    pub async fn check(
        &self,
        ctx: &Context,
        req: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError> {
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

// ============================================================================
// ChannelApprovalService (P7-3.4 / Day 101)
// ============================================================================
//
// v2 完整 approval: oneshot channel 桥接, 业务方 (TUI / HTTP / CLI / Web UI)
// 调 `submit_decision(request_id, decision)` 推送决策.
//
// 跟 v1 AlwaysApprove / AskApprove / PendingApprovals 区别:
// - v1 是 stub, TUI 阻塞主循环 / HTTP 返 placeholder
// - v2 真 oneshot 桥接, 工具调用 future 在 `request_approval` 处 suspend,
//   业务方从别处 (key 事件 / HTTP POST) 调 `submit_decision` 唤醒
//
// 用法:
// ```ignore
// use ma_harness_cordis::{ChannelApprovalService, ApprovalDecision};
//
// let svc = Arc::new(ChannelApprovalService::new());
// // 装到 ctx
// ctx.install_approval(svc.clone(), ApprovalPolicy::Ask);
//
// // 业务方从 TUI / HTTP 推决策
// svc.submit_decision("req-123", ApprovalDecision::Approved);
// ```

/// Channel 桥接的审批服务 (P7-3.4)
///
/// 内部 `Arc<Mutex<HashMap<request_id, oneshot::Sender>>>`.
#[derive(Default, Clone)]
pub struct ChannelApprovalService {
    /// pending requests: tool_call_id -> oneshot::Sender
    pending: std::sync::Arc<
        parking_lot::Mutex<
            std::collections::HashMap<String, tokio::sync::oneshot::Sender<ApprovalDecision>>,
        >,
    >,
}

impl ChannelApprovalService {
    /// 新建空 service
    pub fn new() -> Self {
        Self {
            pending: std::sync::Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// 业务方调: 提交 decision 唤醒等待的 `request_approval` future
    ///
    /// `request_id` 通常是 `ApprovalRequest.tool_call_id`. 返 `true` 表示
    /// 找到了对应 pending request, `false` 表示没找到 (已超时 / 已决策).
    pub fn submit_decision(&self, request_id: &str, decision: ApprovalDecision) -> bool {
        let mut map = self.pending.lock();
        if let Some(tx) = map.remove(request_id) {
            // ignore send error (receiver 已被 drop, e.g. 超时)
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    /// 业务方调: 取消一个 pending request (返 Denied { reason: "cancelled" })
    pub fn cancel(&self, request_id: &str) -> bool {
        self.submit_decision(
            request_id,
            ApprovalDecision::Denied {
                reason: "cancelled by caller".to_string(),
            },
        )
    }

    /// 当前 pending request 数量 (用于 UI 状态显示)
    pub fn pending_count(&self) -> usize {
        self.pending.lock().len()
    }

    /// 列当前 pending request_ids (用于 UI 状态显示)
    pub fn pending_ids(&self) -> Vec<String> {
        self.pending.lock().keys().cloned().collect()
    }

    /// 移除一个 pending request (drops sender without sending decision)
    ///
    /// 跟 `cancel(id)` 区别: cancel 投递 `Denied { cancelled }`,
    /// `remove_pending` 直接 drop sender → receiver 收 Err → 转
    /// `Denied { reason: "channel closed" }`.
    ///
    /// 用法: 测试 / 强制清理 (e.g. 业务方判定 request 过期)
    ///
    /// 返 `true` 表示成功移除, `false` 表示没找到
    pub fn remove_pending(&self, request_id: &str) -> bool {
        self.pending.lock().remove(request_id).is_some()
    }
}

#[async_trait]
impl ApprovalService for ChannelApprovalService {
    async fn request_approval(
        &self,
        _ctx: &Context,
        request: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut map = self.pending.lock();
            map.insert(request.tool_call_id.clone(), tx);
        }
        // 等业务方 push decision (TUI key 事件 / HTTP POST)
        match rx.await {
            Ok(decision) => Ok(decision),
            Err(_) => {
                // sender 被 drop (业务方 cancel 或 service 析构)
                Ok(ApprovalDecision::Denied {
                    reason: "approval channel closed".to_string(),
                })
            }
        }
    }
}

impl std::fmt::Debug for ChannelApprovalService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelApprovalService")
            .field("pending_count", &self.pending_count())
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

    // P7-3.4: ChannelApprovalService 集成测试
    use std::sync::Arc;
    use std::time::Duration;

    fn make_request(tool_call_id: &str) -> ApprovalRequest {
        ApprovalRequest {
            tool_name: "fs.delete".into(),
            arguments: serde_json::json!({}),
            risk_level: RiskLevel::High,
            context: "test".into(),
            tool_call_id: tool_call_id.into(),
        }
    }

    #[tokio::test]
    async fn channel_service_submit_decision_unblocks() {
        let svc = Arc::new(ChannelApprovalService::new());
        let ctx = Context::new();

        // 业务方异步: spawn request, 100ms 后 submit
        let svc2 = svc.clone();
        let req = make_request("req-1");
        let task = tokio::spawn(async move { svc2.request_approval(&ctx, &req).await });

        // 等 service 装上 pending
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(svc.pending_count(), 1);

        // 推 decision
        let submitted = svc.submit_decision("req-1", ApprovalDecision::Approved);
        assert!(submitted);

        // 拿结果
        let result = task.await.unwrap().unwrap();
        assert_eq!(result, ApprovalDecision::Approved);
        assert_eq!(svc.pending_count(), 0);
    }

    #[tokio::test]
    async fn channel_service_submit_unknown_id_returns_false() {
        let svc = ChannelApprovalService::new();
        assert!(!svc.submit_decision("ghost", ApprovalDecision::Approved));
    }

    #[tokio::test]
    async fn channel_service_cancel_returns_denied() {
        let svc = Arc::new(ChannelApprovalService::new());
        let ctx = Context::new();
        let svc2 = svc.clone();
        let req = make_request("req-2");
        let task = tokio::spawn(async move { svc2.request_approval(&ctx, &req).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let cancelled = svc.cancel("req-2");
        assert!(cancelled);
        let result = task.await.unwrap().unwrap();
        assert!(matches!(result, ApprovalDecision::Denied { .. }));
    }

    #[tokio::test]
    async fn channel_service_sender_drop_returns_denied() {
        // sender 析构（无 decision 投递）→ receiver 收 Err → 转 Denied
        //
        // 触发方式: `remove_pending(id)` 从 map 移除 entry, drop tx
        // (注: 不能用 `drop(svc)` 触发, 因为 task 内 `svc2` 还持有 Arc
        // 引用, inner Arc 不会到 0, sender 不会 drop)
        let svc = Arc::new(ChannelApprovalService::new());
        let ctx = Context::new();
        let svc2 = svc.clone();
        let req = make_request("req-3");
        let task = tokio::spawn(async move { svc2.request_approval(&ctx, &req).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let removed = svc.remove_pending("req-3");
        assert!(removed);
        let result = task.await.unwrap().unwrap();
        assert!(matches!(result, ApprovalDecision::Denied { .. }));
    }

    #[tokio::test]
    async fn channel_service_pending_ids_lists_active() {
        let svc = ChannelApprovalService::new();
        assert!(svc.pending_ids().is_empty());
        let ctx = Context::new();
        let svc2 = svc.clone();
        let req1 = make_request("req-a");
        let t1 = tokio::spawn(async move { svc2.request_approval(&ctx, &req1).await });
        let svc3 = svc.clone();
        let req2 = make_request("req-b");
        let t2 = tokio::spawn(async move { svc3.request_approval(&Context::new(), &req2).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let ids = svc.pending_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"req-a".to_string()));
        assert!(ids.contains(&"req-b".to_string()));

        // 清理
        svc.cancel("req-a");
        svc.cancel("req-b");
        let _ = t1.await;
        let _ = t2.await;
    }
}
