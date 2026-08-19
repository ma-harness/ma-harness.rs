# ma-harness-core

> **Note**: This crate is part of the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) workspace. Most users should depend on `ma-harness-seam` instead. `ma-harness-core` exposes the lower-level building blocks: agent loop, session event log (sqlite), and the `ModelAdapter` trait used to talk to LLM providers.

Core building blocks for the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) AI agent orchestrator.

## Features

- **AgentLoop** — orchestrate a single agent run: take user message → emit `SessionEvent`s → call model adapter → return response
- **EventLog** — durable sqlite-backed event store (`open(path)` or `open_in_memory()`)
  - `append(event)` — store event
  - `get_model_visible(session_id)` — return events for a session
  - `list_sessions()` / `count(session_id)` / `recent_events(limit)` — query helpers
- **SessionEvent** — typed event with `EventType` (SessionStart / SessionEnd / ToolCall / ModelResponse / etc.) + `Severity` + optional `payload_json`
- **ModelAdapter trait** — async `complete(&ModelRequest) -> Result<ModelResponse, AdapterError>`
  - `StubModelAdapter` — echoes the user message, useful for tests / offline mode
- **AgentRunRequest / AgentRunResponse** — typed request / response

## Quick example

```rust
use ma_harness_core::{AgentLoop, AgentRunRequest, EventLog, StubModelAdapter};
use std::sync::Arc;

let log = EventLog::open_in_memory()?;
let agent = AgentLoop::new(log, Arc::new(StubModelAdapter));

let resp = agent.run(AgentRunRequest {
    session_id: "demo".to_string(),
    user_message: "hello".to_string(),
    model: "stub".to_string(),
    temperature: 0.7,
    max_tokens: 1024,
    system_prompt: None,
}).await?;

println!("{}", resp.model_response.content);
```

## Stability

This crate is `0.1.0`. The `ModelAdapter` trait is the stable extension point for adding new LLM providers (see `ma-harness-model` for OpenAI / Anthropic reference impls). EventLog schema may evolve in 0.2 with a migration story.

## Documentation

- [API docs (docs.rs)](https://docs.rs/ma-harness-core)
- [ma-harness architecture](https://gitee.com/yifenma/ma-harness.rs)
- [Phase 2.6 SessionStore design](https://gitee.com/yifenma/ma-harness.rs/blob/main/docs/session-store-design.md)

## License

MIT OR Apache-2.0
