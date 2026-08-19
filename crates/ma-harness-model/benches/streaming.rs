//! Streaming benchmark — 测 P5-6/P6-2/P6-3 streaming infra 性能 baseline.
//!
//! 跑法: `cargo bench -p ma-harness-model --bench streaming`
//!
//! ## Bench 列表
//!
//! 1. `parse_sse_data_line_throughput` — OpenAI `data: {json}` 行 parse 速度
//! 2. `parse_sse_event_anthropic_throughput` — Anthropic event-based 解析速度
//! 3. `stub_model_complete_stream_throughput` — StubModelAdapter streaming
//! 4. `openai_complete_stream_e2e_tokens` — 端到端 wiremock + token 产出 latency
//!
//! ## 业务方使用
//!
//! - 比较 stub / OpenAI / Anthropic 三家 streaming 路径 overhead
//! - streaming 优化前后对比 (e.g. 减少 buffer copy, 优化 JSON parse)
//! - CI 跑 perf regression check (留待 P6-4 follow-up)
//!
//! 设计: 见 `docs/benchmark-design.md` (P6-4 增补 § 3.3)

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use ma_harness_core::agent::{ModelAdapter, ModelMessage, ModelRequest, StubModelAdapter};
use ma_harness_model::{AnthropicAdapter, OpenaiAdapter};
use std::sync::OnceLock;

// ============================================================================
// Bench 1: parse_sse_data_line (OpenAI)
// ============================================================================

fn bench_parse_sse_data_line(c: &mut Criterion) {
    // 业务方场景: OpenAI 实际 streaming, 每个 data: 行含 choices[0].delta.content
    let line = r#"data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4o-mini","choices":[{"index":0,"delta":{"content":"Hello world"},"finish_reason":null}]}"#;

    c.bench_function("parse_sse_data_line", |b| {
        b.iter(|| {
            let _ = black_box(OpenaiAdapter::parse_sse_data_line(line));
        });
    });
}

// ============================================================================
// Bench 2: parse_sse_event (Anthropic)
// ============================================================================

fn bench_parse_sse_event_anthropic(c: &mut Criterion) {
    // 业务方场景: Anthropic content_block_delta event
    let event_type = "content_block_delta";
    let data_line = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello world from Anthropic streaming response"}}"#;

    c.bench_function("parse_sse_event_anthropic", |b| {
        b.iter(|| {
            let _ = black_box(AnthropicAdapter::parse_sse_event(event_type, data_line));
        });
    });
}

// ============================================================================
// Bench 3: StubModelAdapter::complete_stream
// ============================================================================

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build runtime")
}

fn sample_request() -> ModelRequest {
    ModelRequest {
        model: "stub".to_string(),
        messages: vec![ModelMessage {
            role: "user".to_string(),
            // 长 prompt 让 stub 拆出更多 word
            content: "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega".to_string(),
        }],
        temperature: 0.7,
        max_tokens: 100,
        system_prompt: None,
    }
}

// criterion async iter 要求 'static future, req 走 OnceLock 拿 &'static
fn static_request() -> &'static ModelRequest {
    static REQ: OnceLock<ModelRequest> = OnceLock::new();
    REQ.get_or_init(sample_request)
}

fn bench_stub_complete_stream(c: &mut Criterion) {
    let runtime = rt();
    let adapter = StubModelAdapter;
    let req = static_request();

    c.bench_function("stub_complete_stream", |b| {
        b.to_async(&runtime).iter(|| {
            use futures::StreamExt;
            let mut stream = adapter.complete_stream(req);
            async move {
                let mut count = 0usize;
                while let Some(token) = stream.next().await {
                    count += token.len();
                }
                black_box(count);
            }
        });
    });
}

// ============================================================================
// Bench 4: OpenAI end-to-end with wiremock (latency)
// ============================================================================

fn bench_openai_complete_stream_e2e(c: &mut Criterion) {
    let runtime = rt();
    let req = static_request();
    let body = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n\
data: [DONE]\n\n";

    c.bench_function("openai_complete_stream_e2e", |b| {
        b.to_async(&runtime).iter(|| {
            use futures::StreamExt;
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};
            // 每次 iter 启新 MockServer (MockServer 不 Send 不可 share)
            async move {
                let mock_server = MockServer::start().await;
                Mock::given(method("POST"))
                    .and(path("/v1/chat/completions"))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .insert_header("content-type", "text/event-stream")
                            .set_body_string(body),
                    )
                    .mount(&mock_server)
                    .await;
                let adapter = OpenaiAdapter::new("sk-test").with_endpoint(format!(
                    "{}/v1/chat/completions",
                    mock_server.uri()
                ));
                let mut stream = adapter.complete_stream(req);
                let mut total_len = 0usize;
                while let Some(token) = stream.next().await {
                    total_len += token.len();
                }
                black_box(total_len);
            }
        });
    });
}

// ============================================================================
// Throughput: parse_sse_data_line 跑 1000 轮 (测总 throughput)
// ============================================================================

fn bench_parse_sse_data_line_throughput(c: &mut Criterion) {
    let line = r#"data: {"choices":[{"index":0,"delta":{"content":"Hello world this is a typical streaming response chunk"}}]}"#;

    let mut group = c.benchmark_group("parse_sse_throughput");
    group.throughput(Throughput::Elements(1));
    group.bench_function("per_line", |b| {
        b.iter(|| {
            let _ = black_box(OpenaiAdapter::parse_sse_data_line(line));
        });
    });
    group.finish();
}

// ============================================================================
// Group + Main
// ============================================================================

criterion_group!(
    name = streaming_benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(3));
    targets =
        bench_parse_sse_data_line,
        bench_parse_sse_event_anthropic,
        bench_stub_complete_stream,
        bench_openai_complete_stream_e2e,
        bench_parse_sse_data_line_throughput,
);

criterion_main!(streaming_benches);
