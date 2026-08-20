//! HTTP /v1/metrics Prometheus endpoint (P10-7 / Day 101)
//!
//! 暴露 Prometheus 格式 metrics 给 ops / monitoring 平台抓取.
//! 业务方 (Grafana / Prometheus / Datadog) 配 scrape config:
//! ```yaml
//! - job_name: ma-harness
//!   scrape_interval: 15s
//!   static_configs:
//!     - targets: ['mah-server:8080']
//! ```
//!
//! ## 当前 metrics
//!
//! 简化 v1: 全局计数器 (process 启动累计, 不分 session/model)
//! - ma_harness_sessions_total (sessions 累计创建数)
//! - ma_harness_runs_total (runs 累计)
//! - ma_harness_model_requests_total
//! - ma_harness_model_responses_total
//! - ma_harness_tool_calls_total
//! - ma_harness_tool_errors_total
//! - ma_harness_approvals_total{decision="approved|denied|auto"}
//! - ma_harness_uptime_seconds (process 启动秒数)
//!
//! ## v2 计划
//!
//! - per-model / per-session labels
//! - histogram 类型 (latency, token 分布)
//! - gauge (active sessions, pending approvals)
//! - OpenTelemetry exporter 集成

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use parking_lot::Mutex;

/// 启动时间 (用于 uptime 算)
static START_TIME: Mutex<Option<Instant>> = Mutex::new(None);

/// 全局 metrics 计数器 (P10-7)
#[derive(Default)]
pub struct Metrics {
    pub sessions_total: AtomicU64,
    pub runs_total: AtomicU64,
    pub model_requests_total: AtomicU64,
    pub model_responses_total: AtomicU64,
    pub tool_calls_total: AtomicU64,
    pub tool_errors_total: AtomicU64,
    pub approvals_approved: AtomicU64,
    pub approvals_denied: AtomicU64,
    pub approvals_auto: AtomicU64,
    pub http_requests_total: AtomicU64,
    pub http_errors_total: AtomicU64,
}

impl std::fmt::Debug for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metrics").finish()
    }
}

impl Metrics {
    /// 构造 (内部用, 业务方拿全局)
    pub const fn new() -> Self {
        Self {
            sessions_total: AtomicU64::new(0),
            runs_total: AtomicU64::new(0),
            model_requests_total: AtomicU64::new(0),
            model_responses_total: AtomicU64::new(0),
            tool_calls_total: AtomicU64::new(0),
            tool_errors_total: AtomicU64::new(0),
            approvals_approved: AtomicU64::new(0),
            approvals_denied: AtomicU64::new(0),
            approvals_auto: AtomicU64::new(0),
            http_requests_total: AtomicU64::new(0),
            http_errors_total: AtomicU64::new(0),
        }
    }

    /// 业务方 (各 handler) 调
    pub fn inc_sessions(&self) {
        self.sessions_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_runs(&self) {
        self.runs_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_model_requests(&self) {
        self.model_requests_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_model_responses(&self) {
        self.model_responses_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_tool_calls(&self) {
        self.tool_calls_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_tool_errors(&self) {
        self.tool_errors_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_approval(&self, decision: &str) {
        match decision {
            "approved" => {
                self.approvals_approved.fetch_add(1, Ordering::Relaxed);
            }
            "denied" => {
                self.approvals_denied.fetch_add(1, Ordering::Relaxed);
            }
            "auto" | "auto_approve" => {
                self.approvals_auto.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
    pub fn inc_http_requests(&self) {
        self.http_requests_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_http_errors(&self) {
        self.http_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 渲染 Prometheus 格式 metrics text
    ///
    /// 格式: 每 metric `# HELP` + `# TYPE` + value 行
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str("# HELP ma_harness_uptime_seconds Process uptime in seconds\n");
        s.push_str("# TYPE ma_harness_uptime_seconds gauge\n");
        s.push_str(&format!("ma_harness_uptime_seconds {}\n", uptime_seconds()));

        // 简化 v1: 用 macro 减少重复
        macro_rules! counter {
            ($name:literal, $field:ident) => {{
                s.push_str(concat!("# HELP ", $name, "\n"));
                s.push_str(concat!("# TYPE ", $name, " counter\n"));
                s.push_str(&format!(
                    "{} {}\n",
                    $name,
                    self.$field.load(Ordering::Relaxed)
                ));
            }};
        }
        counter!("ma_harness_sessions_total", sessions_total);
        counter!("ma_harness_runs_total", runs_total);
        counter!("ma_harness_model_requests_total", model_requests_total);
        counter!("ma_harness_model_responses_total", model_responses_total);
        counter!("ma_harness_tool_calls_total", tool_calls_total);
        counter!("ma_harness_tool_errors_total", tool_errors_total);
        counter!("ma_harness_http_requests_total", http_requests_total);
        counter!("ma_harness_http_errors_total", http_errors_total);

        // approval metrics 带 label
        s.push_str("# HELP ma_harness_approvals_total Total approval decisions\n");
        s.push_str("# TYPE ma_harness_approvals_total counter\n");
        s.push_str(&format!(
            "ma_harness_approvals_total{{decision=\"approved\"}} {}\n",
            self.approvals_approved.load(Ordering::Relaxed)
        ));
        s.push_str(&format!(
            "ma_harness_approvals_total{{decision=\"denied\"}} {}\n",
            self.approvals_denied.load(Ordering::Relaxed)
        ));
        s.push_str(&format!(
            "ma_harness_approvals_total{{decision=\"auto\"}} {}\n",
            self.approvals_auto.load(Ordering::Relaxed)
        ));

        s
    }
}

/// 拿 process 启动秒数 (P10-7)
pub fn uptime_seconds() -> u64 {
    let mut guard = START_TIME.lock();
    if guard.is_none() {
        *guard = Some(Instant::now());
    }
    guard.unwrap().elapsed().as_secs()
}

/// 全局 metrics (P10-7)
pub static METRICS: Metrics = Metrics::new();

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    fn fresh() -> Metrics {
        Metrics::new()
    }

    #[test]
    fn render_includes_uptime() {
        let m = fresh();
        let out = m.render();
        assert!(out.contains("# HELP ma_harness_uptime_seconds"));
        assert!(out.contains("# TYPE ma_harness_uptime_seconds gauge"));
        assert!(out.contains("ma_harness_uptime_seconds"));
    }

    #[test]
    fn render_includes_all_counters_zero() {
        let m = fresh();
        let out = m.render();
        for name in &[
            "ma_harness_sessions_total",
            "ma_harness_runs_total",
            "ma_harness_model_requests_total",
            "ma_harness_model_responses_total",
            "ma_harness_tool_calls_total",
            "ma_harness_tool_errors_total",
            "ma_harness_http_requests_total",
            "ma_harness_http_errors_total",
        ] {
            assert!(out.contains(name), "missing {}", name);
            assert!(out.contains(&format!("{name} 0")), "{name} not 0");
        }
    }

    #[test]
    fn render_includes_approvals_with_labels() {
        let m = fresh();
        let out = m.render();
        assert!(out.contains("ma_harness_approvals_total{decision=\"approved\"} 0"));
        assert!(out.contains("ma_harness_approvals_total{decision=\"denied\"} 0"));
        assert!(out.contains("ma_harness_approvals_total{decision=\"auto\"} 0"));
    }

    #[test]
    fn inc_counters_increments() {
        let m = fresh();
        m.inc_sessions();
        m.inc_sessions();
        m.inc_runs();
        m.inc_tool_calls();
        m.inc_tool_errors();
        m.inc_tool_errors();
        m.inc_tool_errors();
        m.inc_approval("approved");
        m.inc_approval("approved");
        m.inc_approval("denied");
        m.inc_approval("auto");
        m.inc_approval("auto_approve");

        let out = m.render();
        assert!(out.contains("ma_harness_sessions_total 2"));
        assert!(out.contains("ma_harness_runs_total 1"));
        assert!(out.contains("ma_harness_tool_calls_total 1"));
        assert!(out.contains("ma_harness_tool_errors_total 3"));
        assert!(out.contains("ma_harness_approvals_total{decision=\"approved\"} 2"));
        assert!(out.contains("ma_harness_approvals_total{decision=\"denied\"} 1"));
        assert!(out.contains("ma_harness_approvals_total{decision=\"auto\"} 2"));
    }

    #[test]
    fn uptime_increases() {
        let _ = uptime_seconds();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let u = uptime_seconds();
        assert!(u < 10, "uptime 应 < 10 秒, got {}", u);
    }

    #[test]
    fn inc_approval_unknown_does_nothing() {
        let m = fresh();
        m.inc_approval("unknown_decision");
        let out = m.render();
        // 3 决策都是 0
        assert!(out.contains("ma_harness_approvals_total{decision=\"approved\"} 0"));
        assert!(out.contains("ma_harness_approvals_total{decision=\"denied\"} 0"));
        assert!(out.contains("ma_harness_approvals_total{decision=\"auto\"} 0"));
    }
}
