//! Core bench: ma_harness_core agent loop + event log 性能 baseline.
//!
//! 跑法: `cargo bench -p ma_harness_core --bench agent`
//!
//! 设计: 见 `docs/benchmark-design.md` § 3.2.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ma_harness_core::agent::{AgentLoop, AgentRunRequest, ModelAdapter, ModelRequest, StubModelAdapter};
use ma_harness_core::event::{EventType, SessionEvent};
use ma_harness_core::log::EventLog;
use std::sync::Arc;
use tempfile::TempDir;

// ============================================================================
// Bench: EventLog append
// ============================================================================

fn bench_event_log_append(c: &mut Criterion) {
    let tmp = TempDir::new().expect("tempdir");
    let log = EventLog::open(tmp.path().join("bench.db")).expect("open log");
    c.bench_function("event_log_append_single", |b| {
        b.iter(|| {
            let event = SessionEvent::new("bench-session", EventType::RunStart)
                .with_payload(&serde_json::json!({"bench": true}))
                .unwrap();
            let _ = black_box(log.append(event));
        });
    });
}

fn bench_event_log_append_1000(c: &mut Criterion) {
    let tmp = TempDir::new().expect("tempdir");
    let log = EventLog::open(tmp.path().join("bench-1k.db")).expect("open log");
    c.bench_function("event_log_append_1000", |b| {
        b.iter(|| {
            for i in 0..1000 {
                let event = SessionEvent::new("bench-session", EventType::RunStart)
                    .with_payload(&serde_json::json!({"i": i}))
                    .unwrap();
                let _ = log.append(event);
            }
        });
    });
}

// ============================================================================
// Bench: AgentLoop 1 step
// ============================================================================

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build runtime")
}

fn bench_agent_loop_1_step(c: &mut Criterion) {
    let runtime = rt();
    let tmp = TempDir::new().expect("tempdir");
    let log = EventLog::open(tmp.path().join("agent.db")).expect("open log");
    let adapter: Arc<dyn ma_harness_core::agent::ModelAdapter> = Arc::new(StubModelAdapter);
    let agent = Arc::new(AgentLoop::new(log.clone(), adapter));
    let req = AgentRunRequest {
        session_id: "bench-session".to_string(),
        user_message: "hello".to_string(),
        model: "stub".to_string(),
        temperature: 0.7,
        max_tokens: 100,
        system_prompt: None,
    };

    c.bench_function("agent_loop_1_step", |b| {
        b.to_async(&runtime).iter(|| {
            let req = req.clone();
            let agent = agent.clone();
            async move {
                let _ = black_box(agent.run(req).await);
            }
        });
    });
}

fn bench_stub_model_complete(c: &mut Criterion) {
    let runtime = rt();
    let adapter = StubModelAdapter;
    let req = ModelRequest {
        model: "stub".to_string(),
        messages: vec![ma_harness_core::agent::ModelMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        }],
        temperature: 0.7,
        max_tokens: 100,
        system_prompt: None,
    };

    c.bench_function("stub_model_complete", |b| {
        // StubModelAdapter: Copy (line 117 agent.rs), 闭包多次执行 OK
        b.to_async(&runtime).iter(|| {
            let adapter = adapter;
            let req = req.clone();
            async move {
                let _ = black_box(adapter.complete(&req).await);
            }
        });
    });
}

// ============================================================================
// Group + Main
// ============================================================================

criterion_group!(
    name = core_benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(3));
    targets =
        bench_event_log_append,
        bench_event_log_append_1000,
        bench_agent_loop_1_step,
        bench_stub_model_complete,
);

criterion_main!(core_benches);
