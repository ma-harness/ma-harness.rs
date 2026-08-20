# ma-harness-model (中文 / 简体中文)

[English](README.md) | [简体中文](README.zh-CN.md)

[ma-harness.rs](https://github.com/ma-harness/ma-harness.rs) 的 LLM model 适配器: OpenAI, Anthropic, Deepseek, Stub.

[ma-harness.rs](https://github.com/ma-harness/ma-harness.rs) monorepo 的一部分 (也镜像在 <https://gitee.com/yifenma/ma-harness.rs>).

## 包含什么

| Backend | 流式 | 重试 | 视觉 | 工具调用 |
|---|---|---|---|---|
| `OpenaiAdapter` (含 Deepseek) | ✅ | ✅ (P12-2) | ✅ (P11-5) | ✅ |
| `AnthropicAdapter` | ✅ | ✅ (P12-2) | ✅ (P11-5) | ✅ |
| `StubAdapter` (离线测试) | ✅ | n/a | n/a | n/a |

`ModelAdapter` trait — async 流式 token, 返回 `RunResult` 含 `prompt_tokens` / `completion_tokens`.

## 快速开始

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

## 端点

- **OpenAI**: `https://api.openai.com/v1/chat/completions`
- **Deepseek**: `https://api.deepseek.com/v1/chat/completions` (OpenAI-compatible)
- **Anthropic**: `https://api.anthropic.com/v1/messages`
- **Stub**: in-process, 返回 `[stub] <prompt>` 回显

## Re-exports

- `OpenaiAdapter`, `AnthropicAdapter`, `StubAdapter`
- `ModelAdapter` trait
- `Message`, `Role` (P11-5 multimodal)
- `ImageAttachment`, `vision_tool::describe_image` (P11-5/9)
- `retry::{RetryPolicy, retry_with_backoff, is_retryable}` (P12-2)
- `vision_plugin::VisionTool` (P12-8, 用于 `ToolRegistry` 集成)

## 相关 crate (同 workspace)

- [`ma-harness-cordis`](https://crates.io/crates/ma-harness-cordis) — DI / Service / Plugin
- [`ma-harness-core`](https://crates.io/crates/ma-harness-core) — EventLog / agent loop / ModelAdapter trait
- [`ma-harness-seam`](https://crates.io/crates/ma-harness-seam) — 公开 plugin API facade

## 许可证

MIT OR Apache-2.0
