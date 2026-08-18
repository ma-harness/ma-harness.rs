//! Smoke test for the conformance framework.
//!
//! 目的: 验证 framework 自身能跑通 (不依赖 dsh 真实 fixture, 用合成 fixture)。
//!
//! 跑法: `cargo test -p ma_harness_conformance --test smoke`

use ma_harness_conformance::fixture::{
    ExpectedEvent, Fixture, FixtureCategory, FixtureInput, FixtureOutput,
};
use ma_harness_conformance::runner::{ConformanceRunner, RunnerStats};
use ma_harness_conformance::report::ReportWriter;
use std::collections::BTreeMap;

/// 构造一个会通过的 fixture (input event == expected event)。
fn make_passing_fixture(name: &str) -> Fixture {
    let mut payload_match = BTreeMap::new();
    payload_match.insert("tool".to_string(), serde_json::json!("bash"));

    Fixture {
        name: name.to_string(),
        category: FixtureCategory::ToolCall,
        description: Some("Synthetic passing fixture for smoke test".to_string()),
        input: FixtureInput {
            session_id: format!("session-{name}"),
            plugins: vec!["bash".to_string()],
            events: vec![ma_harness_conformance::fixture::FixtureEvent {
                event_type: "ToolCall".to_string(),
                payload: serde_json::json!({"tool": "bash", "args": {"command": "echo hi"}}),
                timestamp_ms: None,
            }],
        },
        output: FixtureOutput {
            events: vec![ExpectedEvent {
                event_type: "ToolCall".to_string(),
                payload_match,
                timestamp_ms: None,
            }],
            final_state: BTreeMap::new(),
        },
    }
}

/// 构造一个会失败的 fixture (type mismatch)。
fn make_failing_fixture(name: &str) -> Fixture {
    Fixture {
        name: name.to_string(),
        category: FixtureCategory::EventOrdering,
        description: Some("Synthetic failing fixture (type mismatch)".to_string()),
        input: FixtureInput {
            session_id: format!("session-{name}"),
            plugins: vec![],
            events: vec![ma_harness_conformance::fixture::FixtureEvent {
                event_type: "RunStart".to_string(),
                payload: serde_json::json!({}),
                timestamp_ms: None,
            }],
        },
        output: FixtureOutput {
            events: vec![ExpectedEvent {
                event_type: "RunEnd".to_string(),
                payload_match: BTreeMap::new(),
                timestamp_ms: None,
            }],
            final_state: BTreeMap::new(),
        },
    }
}

#[test]
fn framework_runs_synthetic_passing_fixture() {
    let runner = ConformanceRunner::new();
    let f = make_passing_fixture("smoke_passing");
    let r = runner.run_fixture(&f);
    assert!(r.is_pass(), "error={:?} diffs={:?}", r.error, r.compare.diffs);
    assert_eq!(r.fixture_name, "smoke_passing");
    assert_eq!(r.compare.actual_count, 1);
    assert_eq!(r.compare.expected_count, 1);
}

#[test]
fn framework_detects_failing_fixture() {
    let runner = ConformanceRunner::new();
    let f = make_failing_fixture("smoke_failing");
    let r = runner.run_fixture(&f);
    assert!(!r.is_pass(), "expected fail but got pass");
    assert_eq!(r.compare.diffs.len(), 1);
}

#[test]
fn framework_run_all_aggregates_stats() {
    let runner = ConformanceRunner::new();
    let fixtures = vec![
        make_passing_fixture("a"),
        make_passing_fixture("b"),
        make_failing_fixture("c"),
    ];
    let results = runner.run_all(&fixtures);
    let stats = RunnerStats::from_results(&results);
    assert_eq!(stats.total, 3);
    assert_eq!(stats.passed, 2);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.errored, 0);
}

#[test]
fn framework_writes_markdown_report_to_tempfile() {
    let runner = ConformanceRunner::new();
    let fixtures = vec![
        make_passing_fixture("x"),
        make_failing_fixture("y"),
    ];
    let results = runner.run_all(&fixtures);
    let summary = runner.build_summary(&results);
    let report = ReportWriter::build(&results, summary);

    let tmp = tempfile::tempdir().expect("tempdir");
    let md_path = tmp.path().join("report.md");
    ReportWriter::write_markdown(&report, &md_path).expect("write md");
    let json_path = tmp.path().join("report.json");
    ReportWriter::write_json(&report, &json_path).expect("write json");

    // 验证文件存在 + 包含关键字
    let md = std::fs::read_to_string(&md_path).expect("read md");
    assert!(md.contains("# Conformance Report"));
    assert!(md.contains("x"));
    assert!(md.contains("y"));

    let json = std::fs::read_to_string(&json_path).expect("read json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert!(parsed["summary"]["total"].as_u64().unwrap() == 2);
    assert!(parsed["summary"]["passed"].as_u64().unwrap() == 1);
}

#[test]
fn fixture_loader_skips_empty_and_comment_lines() {
    use ma_harness_conformance::fixture::FixtureLoader;

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("smoke.jsonl");
    let content = r#"
# 注释行, 应该被跳过
{"name":"a","category":"tool_call","input":{"session_id":"s","events":[]},"output":{"events":[]}}

{"name":"b","category":"tool_call","input":{"session_id":"s","events":[]},"output":{"events":[]}}
"#;
    std::fs::write(&path, content).expect("write");

    let fixtures = FixtureLoader::from_jsonl(&path).expect("load");
    assert_eq!(fixtures.len(), 2);
    assert_eq!(fixtures[0].name, "a");
    assert_eq!(fixtures[1].name, "b");
}

#[test]
fn fixture_loader_handles_optional_fields() {
    use ma_harness_conformance::fixture::FixtureLoader;

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("minimal.jsonl");
    let content = r#"{"name":"minimal","category":"agent_run","input":{"session_id":"s","events":[]},"output":{"events":[]}}"#;
    std::fs::write(&path, content).expect("write");

    let fixtures = FixtureLoader::from_jsonl(&path).expect("load");
    assert_eq!(fixtures.len(), 1);
    assert!(fixtures[0].description.is_none());
    assert!(fixtures[0].input.plugins.is_empty());
}

#[test]
fn framework_collects_passing_and_failing_results_separately() {
    let runner = ConformanceRunner::new();
    let fixtures = vec![
        make_passing_fixture("p1"),
        make_passing_fixture("p2"),
        make_passing_fixture("p3"),
        make_failing_fixture("f1"),
        make_failing_fixture("f2"),
    ];
    let results = runner.run_all(&fixtures);
    let passed: Vec<_> = results.iter().filter(|r| r.is_pass()).collect();
    let failed: Vec<_> = results.iter().filter(|r| !r.is_pass()).collect();
    assert_eq!(passed.len(), 3);
    assert_eq!(failed.len(), 2);
}

#[test]
fn framework_loads_synthetic_fixtures_from_jsonl() {
    use ma_harness_conformance::fixture::FixtureLoader;

    // 找到 crates/ma_harness_conformance/fixtures/smoke.jsonl
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest_dir).join("fixtures/smoke.jsonl");
    let fixtures = FixtureLoader::from_jsonl(&path).expect("load smoke fixtures");
    assert!(fixtures.len() >= 4, "expected >= 4 fixtures, got {}", fixtures.len());

    let runner = ConformanceRunner::new();
    let results = runner.run_all(&fixtures);
    let stats = ma_harness_conformance::runner::RunnerStats::from_results(&results);
    // smoke.jsonl 故意有 1 个 fail (extra event), 其余 pass
    assert!(stats.passed >= 3, "expected >= 3 pass, got {}", stats.passed);
    assert!(stats.failed >= 1, "expected >= 1 fail, got {}", stats.failed);
    assert_eq!(stats.errored, 0);
}

#[test]
fn framework_event_log_preserves_order_across_4_events() {
    use ma_harness_conformance::fixture::{
        ExpectedEvent, Fixture, FixtureCategory, FixtureInput, FixtureOutput,
    };
    use std::collections::BTreeMap;

    // 构造 4 事件 fixture, 验证 EventLog 真落库
    let make_event = |ty: &str| FixtureEvent {
        event_type: ty.to_string(),
        payload: serde_json::json!({}),
        timestamp_ms: None,
    };
    let make_expected = |ty: &str| ExpectedEvent {
        event_type: ty.to_string(),
        payload_match: BTreeMap::new(),
        timestamp_ms: None,
    };

    let f = Fixture {
        name: "order_test".to_string(),
        category: FixtureCategory::AgentRun,
        description: None,
        input: FixtureInput {
            session_id: "order-test".to_string(),
            plugins: vec![],
            events: vec![
                make_event("RunStart"),
                make_event("ToolCall"),
                make_event("ToolResult"),
                make_event("RunEnd"),
            ],
        },
        output: FixtureOutput {
            events: vec![
                make_expected("RunStart"),
                make_expected("ToolCall"),
                make_expected("ToolResult"),
                make_expected("RunEnd"),
            ],
            final_state: BTreeMap::new(),
        },
    };

    let runner = ConformanceRunner::new();
    let r = runner.run_fixture(&f);
    assert!(r.is_pass(), "error={:?} diffs={:?}", r.error, r.compare.diffs);
    assert_eq!(r.actual_events.len(), 4);
    // 顺序保持 (EventLog 1-based seq 单调递增, query 读回按 seq 排序)
    let types: Vec<&str> = r.actual_events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(types, vec!["RunStart", "ToolCall", "ToolResult", "RunEnd"]);
}
