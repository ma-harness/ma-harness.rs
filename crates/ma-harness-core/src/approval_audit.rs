//! 审批审计 log helper (P7-2.6 / Day 101)
//!
//! 业务方在 ToolRegistry::invoke pre-execute hook (P7-2.3) 调, 写 EventLog:
//! - `ApprovalRequest` event (who/when/decision 字段)
//! - `ApprovalDecision` event (decision: Approved / Denied / AutoApprove, reason)

use ma_harness_cordis::{ApprovalDecision, ApprovalRequest, Context, RiskLevel};
use serde_json::json;

use crate::{
    event::{EventType, SessionEvent},
    log::EventLog,
};

/// 写 ApprovalRequest event 到 EventLog
pub fn log_approval_request(log: &EventLog, ctx: &Context, req: &ApprovalRequest) -> i64 {
    let session_id = ctx_session_id(ctx);
    let event = SessionEvent::new(session_id, EventType::ApprovalRequest)
        .with_payload(&json!({
            "tool_call_id": req.tool_call_id,
            "tool_name": req.tool_name,
            "arguments": req.arguments,
            "risk_level": format!("{:?}", req.risk_level),
            "context": req.context,
        }))
        .expect("approval request payload is serializable");
    log.append(event)
}

/// 写 ApprovalDecision event 到 EventLog
pub fn log_approval_decision(
    log: &EventLog,
    ctx: &Context,
    req: &ApprovalRequest,
    decision: &ApprovalDecision,
) -> i64 {
    let session_id = ctx_session_id(ctx);
    let (decision_str, reason) = match decision {
        ApprovalDecision::Approved => ("Approved".to_string(), String::new()),
        ApprovalDecision::AutoApprove => ("AutoApprove".to_string(), String::new()),
        ApprovalDecision::Denied { reason } => ("Denied".to_string(), reason.clone()),
    };
    let event = SessionEvent::new(session_id, EventType::ApprovalDecision)
        .with_payload(&json!({
            "tool_call_id": req.tool_call_id,
            "tool_name": req.tool_name,
            "risk_level": format!("{:?}", req.risk_level),
            "decision": decision_str,
            "reason": reason,
        }))
        .expect("approval decision payload is serializable");
    log.append(event)
}

/// 从 ctx 拿 session_id (P7-2.6 简化: 用 `mah_session_id` typed key)
fn ctx_session_id(ctx: &Context) -> String {
    use ma_harness_cordis::CtxKey;
    // 业务方 ctx 应该设了 "session_id" typed key
    // v1 简化: 没有就返 placeholder
    static SESSION_ID: CtxKey<String> = CtxKey::new_unchecked("session_id");
    ctx.get(SESSION_ID).unwrap_or_else(|| "default".to_string())
}

/// 便捷: 一次性写 request + decision (e.g. auto-approve 立即记录)
pub fn log_approval_pair(
    log: &EventLog,
    ctx: &Context,
    req: &ApprovalRequest,
    decision: &ApprovalDecision,
) -> (i64, i64) {
    let req_seq = log_approval_request(log, ctx, req);
    let dec_seq = log_approval_decision(log, ctx, req, decision);
    (req_seq, dec_seq)
}

/// RiskLevel → 中文标签 (审计 log UI 显示用)
pub fn risk_level_label(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Low => "低风险 (只读)",
        RiskLevel::Medium => "中风险 (写工作区)",
        RiskLevel::High => "高风险 (删/系统)",
        RiskLevel::Critical => "严重风险 (配置/安全)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_cordis::RiskLevel;
    use serde_json::Value;

    #[test]
    fn log_approval_request_writes_event() {
        let log = EventLog::open_in_memory().unwrap();
        let ctx = Context::new();
        let req = ApprovalRequest {
            tool_name: "fs.delete".into(),
            arguments: json!({"path": "/tmp/x"}),
            risk_level: RiskLevel::High,
            context: "delete /tmp/x".into(),
            tool_call_id: "tc-1".into(),
        };
        let seq = log_approval_request(&log, &ctx, &req);
        assert!(seq > 0, "should write event");

        // 验能从 event log 读回
        let page = log
            .query(&crate::log::EventQuery {
                session_id: "default".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.events.len(), 1);
        assert_eq!(
            page.events[0].event.event_type,
            crate::event::EventType::ApprovalRequest
        );
        // 验 payload 含 tool_call_id
        let payload: Value =
            serde_json::from_str(&page.events[0].event.payload_json.clone().unwrap()).unwrap();
        assert_eq!(payload["tool_call_id"], "tc-1");
        assert_eq!(payload["tool_name"], "fs.delete");
    }

    #[test]
    fn log_approval_decision_writes_event() {
        let log = EventLog::open_in_memory().unwrap();
        let ctx = Context::new();
        let req = ApprovalRequest {
            tool_name: "fs.delete".into(),
            arguments: json!({}),
            risk_level: RiskLevel::High,
            context: "delete /tmp/x".into(),
            tool_call_id: "tc-2".into(),
        };
        let dec = ApprovalDecision::Denied {
            reason: "user declined".into(),
        };
        let seq = log_approval_decision(&log, &ctx, &req, &dec);
        assert!(seq > 0);
    }

    #[test]
    fn log_approval_pair_writes_both() {
        let log = EventLog::open_in_memory().unwrap();
        let ctx = Context::new();
        let req = ApprovalRequest {
            tool_name: "fs.read".into(),
            arguments: json!({}),
            risk_level: RiskLevel::Low,
            context: "read /tmp/x".into(),
            tool_call_id: "tc-3".into(),
        };
        let (req_seq, dec_seq) =
            log_approval_pair(&log, &ctx, &req, &ApprovalDecision::AutoApprove);
        assert!(req_seq > 0);
        assert!(dec_seq > req_seq, "decision seq > request seq");
    }

    #[test]
    fn risk_level_label_chinese() {
        assert_eq!(risk_level_label(RiskLevel::Low), "低风险 (只读)");
        assert_eq!(risk_level_label(RiskLevel::Medium), "中风险 (写工作区)");
        assert_eq!(risk_level_label(RiskLevel::High), "高风险 (删/系统)");
        assert_eq!(
            risk_level_label(RiskLevel::Critical),
            "严重风险 (配置/安全)"
        );
    }
}
