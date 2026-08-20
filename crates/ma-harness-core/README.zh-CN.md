# ma-harness-core (中文 / 简体中文)

[English](README.md) | [简体中文](README.zh-CN.md)

> **备注**: 本 crate 是 [ma-harness](https://gitee.com/yifenma/ma-harness.rs) workspace 的一部分. 大多数用户应该依赖 `ma-harness-seam`. `ma-harness-core` 暴露更底层的构建块: agent loop, session event log (sqlite), 以及跟 LLM 提供商对话的 `ModelAdapter` trait.

为 [ma-harness](https://gitee.com/yifenma/ma-harness.rs) AI agent 编排器提供的核心构建块.

## 特性

- **AgentLoop** — 编排单个 agent run: 接收用户消息 → emit `SessionEvent` → 调 model adapter → 返回响应
- **EventLog** — 持久化 sqlite 后端的事件存储 (`open(path)` 或 `open_in_memory()`)
  - `append(event)` — 存储事件
  - `get_model_visible(session_id)` — 返回 session 的事件
  - `list_sessions()` / `count(session_id)` / `recent_events(limit)` — 查询 helper
- **SessionEvent** — 类型化 event, 含 `EventType` (SessionStart / SessionEnd / ToolCall / ModelResponse / 等) + `Severity` + 可选 `payload_json`
- **ModelAdapter trait** — async `complete(&ModelRequest) -> Result<ModelResponse, AdapterError>`
  - `StubModelAdapter` — 回显用户消息, 用于测试 / 离线模式
- **AgentRunRequest / AgentRunResponse** — 类型化 request / response

## 快速示例

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

## 稳定性

本 crate 当前 `0.1.0`. `ModelAdapter` trait 是新增 LLM 提供商的稳定扩展点 (OpenAI / Anthropic 参考实现见 `ma-harness-model`). EventLog schema 可能在 0.2 演进, 配套迁移说明.

## 文档

- [API docs (docs.rs)](https://docs.rs/ma-harness-core)
- [ma-harness 架构](https://gitee.com/yifenma/ma-harness.rs)
- [Phase 2.6 SessionStore 设计](https://gitee.com/yifenma/ma-harness.rs/blob/main/docs/session-store-design.md)

## 许可证

MIT OR Apache-2.0
