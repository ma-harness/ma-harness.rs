//! Approval flow 集成测试 (P7-2.7 / Day 101)
//!
//! 5 scenarios 走 `ToolRegistry::invoke` + `Context.approval()`:
//! 1. **auto-approve** — Never policy 不问, 工具正常执行
//! 2. **ask-yes** — Ask policy + service 返 Approved, 工具正常执行
//! 3. **ask-no** — Ask policy + service 返 Denied, 工具返 Err
//! 4. **service-error** — Ask policy + service 返 Err, 工具返 Err
//! 5. **whitelist-skip** — Whitelist policy 命中白名单 → auto-approve 跳过 service

use std::sync::Arc;

use async_trait::async_trait;
use ma_harness_core::{EventLog, ToolRegistry, ToolSchema};
use ma_harness_cordis::{
    ApprovalDecision, ApprovalPolicy, ApprovalRequest, ApprovalService, BoxedError, Context, RiskLevel,
};
use serde_json::{json, Value};

/// 集成测试用 error type (P7-2.7)
#[derive(Debug)]
struct ApprovalTestError(String);
impl std::fmt::Display for ApprovalTestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ApprovalTestError {}

/// 记录每次 request_approval 调用 (用于断言 service 是否被调过)
#[derive(Default)]
struct MockService {
    calls: parking_lot::Mutex<Vec<ApprovalRequest>>,
    decision: parking_lot::Mutex<Option<ApprovalDecision>>,
    fail_with: parking_lot::Mutex<Option<String>>,
}

impl MockService {
    fn always(decision: ApprovalDecision) -> Self {
        Self {
            calls: parking_lot::Mutex::new(Vec::new()),
            decision: parking_lot::Mutex::new(Some(decision)),
            fail_with: parking_lot::Mutex::new(None),
        }
    }

    fn always_fail(msg: &str) -> Self {
        Self {
            calls: parking_lot::Mutex::new(Vec::new()),
            decision: parking_lot::Mutex::new(None),
            fail_with: parking_lot::Mutex::new(Some(msg.to_string())),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().len()
    }
}

#[async_trait]
impl ApprovalService for MockService {
    async fn request_approval(
        &self,
        _ctx: &Context,
        request: &ApprovalRequest,
    ) -> Result<ApprovalDecision, BoxedError> {
        self.calls.lock().push(request.clone());
        if let Some(msg) = self.fail_with.lock().as_ref() {
            return Err(BoxedError::new(ApprovalTestError(msg.clone())));
        }
        Ok(self
            .decision
            .lock()
            .clone()
            .unwrap_or(ApprovalDecision::Approved))
    }
}

/// 注册一个 fake "echo" 工具, 返 `{"echoed": args}`.
fn register_echo(reg: &ToolRegistry) {
    reg.register(
        ToolSchema {
            name: "echo".into(),
            description: "echo args".into(),
            parameters: json!({"type": "object"}),
        },
        Arc::new(|args, _ctx| {
            Box::pin(async move { Ok(json!({ "echoed": args })) })
        }),
    );
}

#[tokio::test]
async fn auto_approve_never_policy() {
    // Never policy 永远不审批, service 应该**不被**调用
    let reg = ToolRegistry::new();
    register_echo(&reg);
    let svc = Arc::new(MockService::always(ApprovalDecision::Denied {
        reason: "should not be called".into(),
    }));
    let ctx = Context::new();
    ctx.install_approval(svc.clone(), ApprovalPolicy::Never);

    let result = reg
        .invoke("echo", json!({"msg": "hi"}), ctx)
        .await
        .expect("auto-approve should pass");

    assert_eq!(result["echoed"]["msg"], "hi");
    assert_eq!(
        svc.call_count(),
        0,
        "Never policy 不该 call service"
    );
}

#[tokio::test]
async fn ask_policy_service_approves() {
    // Ask policy + service 返 Approved → invoke 成功
    let reg = ToolRegistry::new();
    register_echo(&reg);
    let svc = Arc::new(MockService::always(ApprovalDecision::Approved));
    let ctx = Context::new();
    ctx.install_approval(svc.clone(), ApprovalPolicy::Ask);

    let result = reg
        .invoke("echo", json!({"msg": "ask-yes"}), ctx)
        .await
        .expect("ask-yes should pass");

    assert_eq!(result["echoed"]["msg"], "ask-yes");
    assert_eq!(svc.call_count(), 1, "Ask policy 调 1 次 service");
}

#[tokio::test]
async fn ask_policy_service_denies() {
    // Ask policy + service 返 Denied → invoke 返 Err 含 "approval denied"
    let reg = ToolRegistry::new();
    register_echo(&reg);
    let svc = Arc::new(MockService::always(ApprovalDecision::Denied {
        reason: "user said no".into(),
    }));
    let ctx = Context::new();
    ctx.install_approval(svc.clone(), ApprovalPolicy::Ask);

    let err = reg
        .invoke("echo", json!({}), ctx)
        .await
        .expect_err("ask-no should fail");
    let msg = err.to_string();
    assert!(msg.contains("approval denied"), "got: {msg}");
    assert!(msg.contains("user said no"), "got: {msg}");
    assert_eq!(svc.call_count(), 1);
}

#[tokio::test]
async fn ask_policy_service_errors() {
    // Ask policy + service 返 Err → invoke 返 Err 含 "approval service error"
    let reg = ToolRegistry::new();
    register_echo(&reg);
    let svc = Arc::new(MockService::always_fail("network timeout"));
    let ctx = Context::new();
    ctx.install_approval(svc.clone(), ApprovalPolicy::Ask);

    let err = reg
        .invoke("echo", json!({}), ctx)
        .await
        .expect_err("service error should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("approval service error"),
        "got: {msg}"
    );
    assert!(msg.contains("network timeout"), "got: {msg}");
    assert_eq!(svc.call_count(), 1);
}

#[tokio::test]
async fn whitelist_policy_skips_listed_tool() {
    // Whitelist 命中 → auto-approve (AutoApprove decision), service 不被调
    let reg = ToolRegistry::new();
    register_echo(&reg);
    let svc = Arc::new(MockService::always(ApprovalDecision::Denied {
        reason: "should not be called".into(),
    }));
    let ctx = Context::new();
    ctx.install_approval(
        svc.clone(),
        ApprovalPolicy::Whitelist {
            tools: vec!["echo".into()],
        },
    );

    let result = reg
        .invoke("echo", json!({"x": 1}), ctx)
        .await
        .expect("whitelist should auto-approve");

    assert_eq!(result["echoed"]["x"], 1);
    assert_eq!(
        svc.call_count(),
        0,
        "白名单命中不该 call service"
    );
}

#[tokio::test]
async fn whitelist_policy_asks_unlisted_tool() {
    // Whitelist 没命中 → 走 Ask → service 返 Approved
    let reg = ToolRegistry::new();
    register_echo(&reg);
    let svc = Arc::new(MockService::always(ApprovalDecision::Approved));
    let ctx = Context::new();
    ctx.install_approval(
        svc.clone(),
        ApprovalPolicy::Whitelist {
            tools: vec!["other.tool".into()],
        },
    );

    let result = reg
        .invoke("echo", json!({}), ctx)
        .await
        .expect("ask unlisted should pass via Approved");
    assert_eq!(result["echoed"], json!({}));
    assert_eq!(svc.call_count(), 1);
}

#[tokio::test]
async fn no_approval_installed_passes_through() {
    // 没装 approval → backward-compat, auto-approve
    let reg = ToolRegistry::new();
    register_echo(&reg);
    let ctx = Context::new();
    assert!(ctx.approval().is_none());

    let result = reg
        .invoke("echo", json!({"no": "approval"}), ctx)
        .await
        .expect("no approval should pass");
    assert_eq!(result["echoed"]["no"], "approval");
}

#[tokio::test]
async fn approval_audit_writes_pair_to_eventlog() {
    // 走完整 audit log: 装 approval, 调 invoke, 验 log 有 request + decision
    use ma_harness_core::approval_audit::log_approval_pair;
    use ma_harness_cordis::CtxKey;

    let reg = ToolRegistry::new();
    register_echo(&reg);
    let svc = Arc::new(MockService::always(ApprovalDecision::Approved));
    let ctx = Context::new();

    // 设 session_id typed key (P7-2.6 helper 拿)
    static SESSION_ID: CtxKey<String> = CtxKey::new_unchecked("session_id");
    ctx.set(SESSION_ID, "test-session".to_string());

    ctx.install_approval(svc.clone(), ApprovalPolicy::Ask);

    let log = EventLog::open_in_memory().unwrap();

    // 业务方 audit pattern: invoke 前构造 request + decision 走 audit, 再调 invoke
    let req = ApprovalRequest {
        tool_name: "echo".into(),
        arguments: json!({"audit": true}),
        risk_level: RiskLevel::Low,
        context: "test audit".into(),
        tool_call_id: "tc-audit".into(),
    };
    let dec = ApprovalDecision::Approved;
    let (req_seq, dec_seq) = log_approval_pair(&log, &ctx, &req, &dec);
    assert!(req_seq > 0);
    assert!(dec_seq > req_seq);

    // 调真实 invoke (会再走一次 approval service, log 不重复写)
    let result = reg.invoke("echo", json!({}), ctx).await.unwrap();
    assert_eq!(result["echoed"], json!({}));

    // 验 event log 至少 1 个 ApprovalRequest + 1 个 ApprovalDecision
    let page = log
        .query(&ma_harness_core::log::EventQuery {
            session_id: "test-session".into(),
            ..Default::default()
        })
        .unwrap();
    let mut have_request = false;
    let mut have_decision = false;
    for ev in &page.events {
        match ev.event.event_type {
            ma_harness_core::event::EventType::ApprovalRequest => have_request = true,
            ma_harness_core::event::EventType::ApprovalDecision => have_decision = true,
            _ => {}
        }
    }
    assert!(have_request, "应写 ApprovalRequest event");
    assert!(have_decision, "应写 ApprovalDecision event");
}
