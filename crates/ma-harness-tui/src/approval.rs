//! TUI approval service (P7-2.4 / P10-2 / Day 101)
//!
//! 业务方在 `mah tui` 启动时, 选 approval 策略:
//! - `AlwaysApprove`: 所有工具调用 auto-approve (适合 benchmark / 测试)
//! - `AskApprove`: y/n 弹窗等用户决策 (v2 oneshot 完整版, P10-2)
//!
//! # v2 oneshot 流程 (P10-2)
//!
//! 1. TUI 主循环 spawn TuiApprover::request_approval (会 suspend 等 oneshot)
//! 2. TuiApprover 装到 ChannelApprovalService (suspend 等待)
//! 3. TUI 主循环通过 peek_pending() 看到新 request
//! 4. 渲染 y/n modal
//! 5. 用户按 y/n → 调 channel.submit_decision(tool_call_id, decision)
//! 6. request_approval future 唤醒, 返 decision
//!
//! # 用法
//!
//! ```ignore
//! use ma_harness_tui::approval::{TuiApprover, AlwaysApprove};
//! use std::sync::Arc;
//!
//! let approver = Arc::new(TuiApprover::new());
//! ctx.install_approval(approver.clone(), ApprovalPolicy::Ask);
//! // TUI 主循环:
//! //   - 调 approver.peek_pending() 拿 latest request
//! //   - 渲染 modal
//! //   - 按 y 调 approver.approve(tool_call_id)
//! //   - 按 n 调 approver.deny(tool_call_id, "user declined")
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use ma_harness_cordis::{
    ApprovalDecision, ApprovalRequest, ApprovalService, BoxedError, ChannelApprovalService, Context,
};
use parking_lot::Mutex;

/// P7-2.4: 永远 auto-approve (适合测试 / benchmark)
pub struct AlwaysApprove;

#[async_trait]
impl ApprovalService for AlwaysApprove {
    async fn request_approval(
        &self,
        _ctx: &Context,
        _req: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError> {
        Ok(ApprovalDecision::AutoApprove)
    }
}

/// AskApprove v1 stub: 当前直接返 Approved
/// (P10-2 仍然保留作为备选, 实际推荐用 TuiApprover v2)
pub struct AskApprove;

#[async_trait]
impl ApprovalService for AskApprove {
    async fn request_approval(
        &self,
        _ctx: &Context,
        _req: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError> {
        eprintln!(
            "[tui-approval] ASK (stub, auto-approve): tool={} risk={:?}",
            _req.tool_name, _req.risk_level
        );
        Ok(ApprovalDecision::Approved)
    }
}

/// PendingApprovals (P7-2.4 兼容保留, v2 业务方用 TuiApprover.peek_pending 替代)
#[derive(Default)]
pub struct PendingApprovals {
    inner: Mutex<Vec<ApprovalRequest>>,
}

impl PendingApprovals {
    pub fn push(&self, req: ApprovalRequest) {
        self.inner.lock().push(req);
    }
    pub fn pop(&self) -> Option<ApprovalRequest> {
        self.inner.lock().pop()
    }
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }
}

impl std::fmt::Debug for PendingApprovals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingApprovals").finish()
    }
}

/// TuiApprover v2 (P10-2): oneshot channel 桥接 TUI 主循环
///
/// 内部用 ChannelApprovalService (P7-3.4), 业务方 (TUI 主循环):
/// - peek_pending() 拿当前最新 pending request
/// - approve(tool_call_id) / deny(tool_call_id, reason) 推 decision
///
/// v1 兼容: 保留 `pending: Arc<PendingApprovals>` 字段, 但实际推荐 v2 路径
pub struct TuiApprover {
    /// Channel service (oneshot 桥接)
    pub channel: Arc<ChannelApprovalService>,
    /// v1 兼容: pending queue (TUI 启动早期 fallback)
    pub pending: Arc<PendingApprovals>,
}

impl TuiApprover {
    /// 构造 v2 (ChannelApprovalService + PendingApprovals fallback)
    pub fn new() -> Self {
        Self {
            channel: Arc::new(ChannelApprovalService::new()),
            pending: Arc::new(PendingApprovals::default()),
        }
    }

    /// 业务方 (TUI 主循环) 调: 拿当前最新 pending request
    pub fn peek_pending(&self) -> Vec<(String, String, String)> {
        // (tool_call_id, tool_name, context)
        self.channel
            .pending_ids()
            .into_iter()
            .map(|id| (id, "<unknown>".to_string(), "<pending>".to_string()))
            .collect()
    }

    /// 业务方 (TUI 主循环 y 键) 调: 批准
    pub fn approve(&self, tool_call_id: &str) -> bool {
        self.channel
            .submit_decision(tool_call_id, ApprovalDecision::Approved)
    }

    /// 业务方 (TUI 主循环 n 键) 调: 拒绝
    pub fn deny(&self, tool_call_id: &str, reason: impl Into<String>) -> bool {
        self.channel.submit_decision(
            tool_call_id,
            ApprovalDecision::Denied {
                reason: reason.into(),
            },
        )
    }

    /// 拿底层 ChannelApprovalService (HTTP 集成用)
    pub fn channel_service(&self) -> Arc<ChannelApprovalService> {
        self.channel.clone()
    }
}

impl Default for TuiApprover {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ApprovalService for TuiApprover {
    async fn request_approval(
        &self,
        ctx: &Context,
        req: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError> {
        // v2 路径: 走 ChannelApprovalService, suspend 等 TUI 主循环响应
        // 同时 push 到 pending (v1 兼容, 早期启动 fallback 显示)
        self.pending.push(req.clone());
        ma_harness_cordis::ApprovalService::request_approval(self.channel.as_ref(), ctx, req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_cordis::RiskLevel;

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
    async fn always_approve_returns_auto_approve() {
        let svc = AlwaysApprove;
        let req = make_request("tc-1");
        let decision = svc.request_approval(&Context::new(), &req).await.unwrap();
        assert_eq!(decision, ApprovalDecision::AutoApprove);
    }

    #[tokio::test]
    async fn ask_approve_v1_returns_approved() {
        let svc = AskApprove;
        let req = make_request("tc-2");
        let decision = svc.request_approval(&Context::new(), &req).await.unwrap();
        assert_eq!(decision, ApprovalDecision::Approved);
    }

    #[tokio::test]
    async fn pending_approvals_push_pop() {
        let pending = PendingApprovals::default();
        assert_eq!(pending.len(), 0);
        pending.push(make_request("tc-3"));
        assert_eq!(pending.len(), 1);
        let popped = pending.pop().unwrap();
        assert_eq!(popped.tool_name, "fs.delete");
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn tui_approver_v2_oneshot_approve_wakes_future() {
        // v2 oneshot: spawn request_approval, 调 approve 唤醒
        let approver = Arc::new(TuiApprover::new());
        let req = make_request("tc-v2-1");
        let svc2 = approver.clone();
        let task: tokio::task::JoinHandle<Result<ApprovalDecision, _>> =
            tokio::spawn(async move {
                ma_harness_cordis::ApprovalService::request_approval(
                    svc2.channel.as_ref(),
                    &Context::new(),
                    &req,
                )
                .await
            });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(approver.channel.pending_count(), 1);

        let approved = approver.approve("tc-v2-1");
        assert!(approved);

        let result = task.await.unwrap().unwrap();
        assert_eq!(result, ApprovalDecision::Approved);
    }

    #[tokio::test]
    async fn tui_approver_v2_deny_wakes_future() {
        let approver = Arc::new(TuiApprover::new());
        let req = make_request("tc-v2-2");
        let svc2 = approver.clone();
        let task: tokio::task::JoinHandle<Result<ApprovalDecision, _>> =
            tokio::spawn(async move {
                ma_harness_cordis::ApprovalService::request_approval(
                    svc2.channel.as_ref(),
                    &Context::new(),
                    &req,
                )
                .await
            });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let denied = approver.deny("tc-v2-2", "user said no");
        assert!(denied);
        let result = task.await.unwrap().unwrap();
        assert!(matches!(result, ApprovalDecision::Denied { .. }));
    }

    #[tokio::test]
    async fn tui_approver_v2_peek_pending() {
        let approver = Arc::new(TuiApprover::new());
        let req = make_request("tc-peek");
        let svc2 = approver.clone();
        let _task: tokio::task::JoinHandle<Result<ApprovalDecision, _>> =
            tokio::spawn(async move {
                ma_harness_cordis::ApprovalService::request_approval(
                    svc2.channel.as_ref(),
                    &Context::new(),
                    &req,
                )
                .await
            });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let peeked = approver.peek_pending();
        assert_eq!(peeked.len(), 1);
        // 清理
        approver.deny("tc-peek", "test cleanup");
    }

    #[tokio::test]
    async fn tui_approver_v2_approve_unknown_returns_false() {
        let approver = TuiApprover::new();
        assert!(!approver.approve("ghost"));
        assert!(!approver.deny("ghost", "x"));
    }
}

