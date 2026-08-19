//! TUI approval service (P7-2.4 / Day 101+)
//!
//! 业务方在 `mah tui` 启动时, 选 approval 策略:
//! - `AlwaysApprove`: 所有工具调用 auto-approve (适合 benchmark / 测试)
//! - `AskApprove`: y/n 弹窗等用户决策 (P7-2.4 v2 完整版, 当前 stub)
//!
//! # 用法
//!
//! ```ignore
//! use ma_harness_tui::approval::{AlwaysApprove, AskApprove};
//! use std::sync::Arc;
//!
//! let app = TuiApp::new_with_log_and_store(log, store)
//!     .install_approval(Arc::new(AlwaysApprove));
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use ma_harness_cordis::{
    ApprovalDecision, ApprovalRequest, ApprovalService, BoxedError, Context,
};
use parking_lot::Mutex;

/// P7-2.4: 永远 auto-approve (适合测试 / benchmark)
///
/// 业务方不想被打扰, 走 Never policy 跟 AlwaysApprove 等价.
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

/// P7-2.4 v1: AskApprove stub — 当前直接返 Approved (跟 dsh 设计差距)
/// TODO(P7-2.4 v2): 用 oneshot::channel 桥接 TUI 主循环, 真等用户 y/n.
/// 现在业务方如果想真审批, 应该用:
///   - HTTP 审批: P7-2.5 (POST /v1/approvals/{tool_call_id})
///   - TUI 弹窗: P7-2.4 v2 (后续 Phase 7+ 跟 P7-3 工具管道一起做)
pub struct AskApprove;

#[async_trait]
impl ApprovalService for AskApprove {
    async fn request_approval(
        &self,
        _ctx: &Context,
        req: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError> {
        // v1 stub: 记录到 stderr, 返 Approved (跟 dsh Ask 行为不一致, v2 会改)
        eprintln!(
            "[tui-approval] ASK (v1 stub, auto-approve): tool={} risk={:?} ctx={}",
            req.tool_name, req.risk_level, req.context
        );
        Ok(ApprovalDecision::Approved)
    }
}

/// 业务方共享的 pending approval queue (TUI 主循环读, Approve service 写)
/// 简化 v1: 用 `parking_lot::Mutex<Vec<ApprovalRequest>>`, TUI 跑时轮询弹出
/// 实际 v2: `HashMap<tool_call_id, oneshot::Sender<ApprovalDecision>>` 异步
#[derive(Default)]
pub struct PendingApprovals {
    inner: Mutex<Vec<ApprovalRequest>>,
}

impl PendingApprovals {
    /// 业务方 push approval request
    pub fn push(&self, req: ApprovalRequest) {
        self.inner.lock().push(req);
    }

    /// 业务方 pop 等待用户决策的 approval
    pub fn pop(&self) -> Option<ApprovalRequest> {
        self.inner.lock().pop()
    }

    /// 当前等待的数量
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }
}

impl std::fmt::Debug for PendingApprovals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingApprovals").finish()
    }
}

/// TuiApprover: 业务方装到 ctx, TUI 跑时主循环检查 pending
pub struct TuiApprover {
    pub pending: Arc<PendingApprovals>,
}

impl TuiApprover {
    /// 构造
    pub fn new(pending: Arc<PendingApprovals>) -> Self {
        Self { pending }
    }
}

#[async_trait]
impl ApprovalService for TuiApprover {
    async fn request_approval(
        &self,
        _ctx: &Context,
        req: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError> {
        // v1: push pending, TUI 主循环 pop 显示
        // 业务方简化: TUI 启动后没线程 poll, 返 Approved (跟 AskApprove stub 等价)
        // 完整 v2: 改用 oneshot, 阻塞等 TUI 主循环响应
        eprintln!(
            "[tui-approver] v1 stub, push to pending: tool={}",
            req.tool_name
        );
        self.pending.push(req.clone());
        Ok(ApprovalDecision::Approved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_cordis::RiskLevel;

    #[tokio::test]
    async fn always_approve_returns_auto_approve() {
        let svc = AlwaysApprove;
        let req = ApprovalRequest {
            tool_name: "fs.delete".into(),
            arguments: serde_json::json!({}),
            risk_level: RiskLevel::Critical,
            context: "delete /etc/passwd".into(),
            tool_call_id: "tc-1".into(),
        };
        let decision = svc.request_approval(&Context::new(), &req).await.unwrap();
        assert_eq!(decision, ApprovalDecision::AutoApprove);
    }

    #[tokio::test]
    async fn ask_approve_v1_returns_approved() {
        let svc = AskApprove;
        let req = ApprovalRequest {
            tool_name: "fs.write".into(),
            arguments: serde_json::json!({"path": "/tmp/x"}),
            risk_level: RiskLevel::Medium,
            context: "write /tmp/x".into(),
            tool_call_id: "tc-2".into(),
        };
        let decision = svc.request_approval(&Context::new(), &req).await.unwrap();
        assert_eq!(decision, ApprovalDecision::Approved, "v1 stub auto-approve");
    }

    #[tokio::test]
    async fn pending_approvals_push_pop() {
        let pending = PendingApprovals::default();
        assert_eq!(pending.len(), 0);

        let req1 = ApprovalRequest {
            tool_name: "fs.read".into(),
            arguments: serde_json::json!({}),
            risk_level: RiskLevel::Low,
            context: "read /tmp/x".into(),
            tool_call_id: "tc-3".into(),
        };
        pending.push(req1.clone());
        assert_eq!(pending.len(), 1);

        let popped = pending.pop().unwrap();
        assert_eq!(popped.tool_name, "fs.read");
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn tui_approver_pushes_pending() {
        let pending = Arc::new(PendingApprovals::default());
        let svc = TuiApprover::new(pending.clone());

        let req = ApprovalRequest {
            tool_name: "fs.delete".into(),
            arguments: serde_json::json!({}),
            risk_level: RiskLevel::High,
            context: "delete /tmp/y".into(),
            tool_call_id: "tc-4".into(),
        };
        let decision = svc.request_approval(&Context::new(), &req).await.unwrap();
        assert_eq!(decision, ApprovalDecision::Approved);
        assert_eq!(pending.len(), 1, "v1 stub: push 到 pending");
    }
}
