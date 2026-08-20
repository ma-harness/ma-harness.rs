# ma-harness-model

LLM model adapters for [ma-harness.rs](https://github.com/ma-harness/ma-harness.rs): OpenAI, Anthropic, Deepseek, Stub.

Part of the [ma-harness.rs](https://github.com/ma-harness/ma-harness.rs) monorepo (also mirrored at <https://gitee.com/yifenma/ma-harness.rs>).

## What's here

| Backend | Streaming | Retry | Vision | Tool-call |
|---|---|---|---|---|
| `OpenaiAdapter` (incl. Deepseek) | ✅ | ✅ (P12-2) | ✅ (P11-5) | ✅ |
| `AnthropicAdapter` | ✅ | ✅ (P12-2) | ✅ (P11-5) | ✅ |
| `StubAdapter` (offline test) | ✅ | n/a | n/a | n/a |

`ModelAdapter` trait — async stream tokens, returns `RunResult` with `prompt_tokens` / `completion_tokens`.

## Quick start

```toml
# Cargo.toml
[dependencies]
ma-harness-model = "0.1"
tokio = { version = "1", features = ["full"] }
futures = "0.3"
```

```rust
use ma_harness_model::{ModelRegistry, OpenaiAdapter, ModelAdapter, Message};
use futures::StreamExt;

#[tokio::main]
async fn main() {
    let adapter = OpenaiAdapter::from_env("OPENAI_API_KEY").expect("OPENAI_API_KEY set");
    let messages = vec![Message::user("hello world")];
    let mut stream = adapter.complete_stream(&messages, &Default::default()).await.unwrap();
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk.content);
    }
}
```

## Endpoints

- **OpenAI**: `https://api.openai.com/v1/chat/completions`
- **Deepseek**: `https://api.deepseek.com/v1/chat/completions` (OpenAI-compatible)
- **Anthropic**: `https://api.anthropic.com/v1/messages`
- **Stub**: in-process, returns `[stub] <prompt>` echoed back

## Re-exports

- `OpenaiAdapter`, `AnthropicAdapter`, `StubAdapter`
- `ModelAdapter` trait
- `Message`, `Role` (P11-5 multimodal)
- `ImageAttachment`, `vision_tool::describe_image` (P11-5/9)
- `retry::{RetryPolicy, retry_with_backoff, is_retryable}` (P12-2)
- `vision_plugin::VisionTool` (P12-8, for `ToolRegistry` integration)

## Related crates (same workspace)

- [`ma-harness-cordis`](https://crates.io/crates/ma-harness-cordis) — DI / Service / Plugin
- [`ma-harness-core`](https://crates.io/crates/ma-harness-core) — EventLog / agent loop / ModelAdapter trait
- [`ma-harness-seam`](https://crates.io/crates/ma-harness-seam) — public plugin API facade

## License

MIT OR Apache-2.0
