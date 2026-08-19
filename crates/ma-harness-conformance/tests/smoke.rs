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
        ExpectedEvent, Fixture, FixtureCategory, FixtureEvent, FixtureInput, FixtureOutput,
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

#[test]
fn dsh_format_loads_synthetic_fixtures() {
    use ma_harness_conformance::dsh_format::parse_dsh_jsonl;

    let content = r#"
# 模拟 dsh 真实 fixture (待 Week 12 校准)
{"name":"dsh_smoke_1","category":"agent","input":{"session_id":"s","messages":[{"role":"user","content":"hi"}]},"expected_output":{"events":[{"type":"RunStart","data":{}}],"messages":[{"role":"assistant","content":"hello"}]}}

{"name":"dsh_smoke_2","category":"tool","input":{"session_id":"s2","tools":["bash"],"events":[{"type":"ToolCall","data":{"tool":"bash"}}]},"expected_output":{"events":[{"type":"ToolResult","data":{"result":"ok"}}]}}
"#;
    let fs = parse_dsh_jsonl(content).expect("parse");
    assert_eq!(fs.len(), 2);
    assert_eq!(fs[0].name, "dsh_smoke_1");
    // P11-1.5: input.events 从 messages 派生 RunStart (前置) + UserInput
    assert_eq!(fs[0].input.events.len(), 2);
    assert_eq!(fs[0].input.events[0].event_type, "RunStart");
    assert_eq!(fs[0].input.events[1].event_type, "UserInput");
    // output.events 含 expected events + assistant message 派生
    assert!(fs[0].output.events.iter().any(|e| e.event_type == "RunStart"));
    assert!(fs[0].output.events.iter().any(|e| e.event_type == "ModelResponse"));
    // tools alias → plugins
    assert_eq!(fs[1].input.plugins, vec!["bash"]);
}

#[test]
fn dsh_format_aliases_supported() {
    use ma_harness_conformance::dsh_format::parse_dsh_jsonl;

    let content = r#"
{"name":"alias_full","input":{"session_id":"s","tools":["bash","fs"]},"expected":{"events":[{"type":"ToolCall","payload":{"tool":"bash"}}]}}
"#;
    let fs = parse_dsh_jsonl(content).expect("parse");
    assert_eq!(fs.len(), 1);
    assert_eq!(fs[0].input.plugins, vec!["bash", "fs"]);
    assert_eq!(fs[0].output.events[0].payload_match.get("tool").unwrap(), "bash");
}

#[test]
fn runner_runs_dsh_synthetic_fixtures() {
    use ma_harness_conformance::dsh_format::parse_dsh_jsonl;

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest_dir).join("fixtures/dsh_synthetic.jsonl");
    let content = std::fs::read_to_string(&path).expect("read dsh synthetic");
    let fs = parse_dsh_jsonl(&content).expect("parse dsh synthetic");
    assert!(!fs.is_empty(), "no dsh synthetic fixtures loaded");

    let runner = ConformanceRunner::new();
    let results = runner.run_all(&fs);
    let stats = ma_harness_conformance::runner::RunnerStats::from_results(&results);
    // P11-1.5: 转换层改进后, 7/7 fixture 全 pass
    // - input.messages 派生完整 ma-harness 视角 events (RunStart + UserInput/ModelResponse/...)
    // - expected_output.data 非对象走 "data" key fallback
    // - expected_output.messages assistant 派生 ModelResponse
    // - alias 兼容 (tools/expectedOutput/payload)
    assert_eq!(stats.errored, 0, "runner errors: {:?}", results.iter().filter(|r| r.error.is_some()).collect::<Vec<_>>());
    assert_eq!(stats.passed, 7, "expected 7 pass (P11-1.5 收官), got {} (failed={}, total={})", stats.passed, stats.failed, stats.total);
    assert_eq!(stats.failed, 0, "expected 0 fail, got {:?}", stats.failed);
}

/// **P11-2**: 跑 dsh 真实 acp-snapshot fixture (从 dsh 仓库转换)
/// 9 个 fixture 全 pass = dsh 跟 ma-harness 行为等价 (snapshot 视角)
#[test]
fn runner_runs_dsh_snap_converted_fixtures() {
    use ma_harness_conformance::dsh_format::parse_dsh_jsonl;

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&manifest_dir).join("fixtures/dsh-snap-converted/dsh_snap.jsonl");
    let content = std::fs::read_to_string(&path).expect("read dsh snap converted");
    let fs = parse_dsh_jsonl(&content).expect("parse dsh snap converted");
    assert_eq!(fs.len(), 9, "expected 9 dsh snap fixtures, got {}", fs.len());

    let runner = ConformanceRunner::new();
    let results = runner.run_all(&fs);
    let stats = ma_harness_conformance::runner::RunnerStats::from_results(&results);
    // P11-2: 9/9 dsh 真实 acp-snapshot fixture 全 pass
    // - suite/ 6: authored-error, blocked-log, no-model, pin-turn, plain-turn, shared-pin
    // - record-suite/ 3: rec-child, rec-pin, rec-skip
    // 转换脚本: crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap_convert.py
    assert_eq!(stats.errored, 0, "runner errors: {:?}", results.iter().filter(|r| r.error.is_some()).collect::<Vec<_>>());
    assert_eq!(stats.passed, 9, "expected 9 pass (P11-2 收官), got {} (failed={}, total={})", stats.passed, stats.failed, stats.total);
    assert_eq!(stats.failed, 0, "expected 0 fail, got {:?}", stats.failed);
}
