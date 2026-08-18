# ma-harness Protobuf 定义

> 锁定版本: `ma_harness.v1`
> 任何 breaking change 走 `v2` package,不破坏 v1 兼容。

## 文件清单

| 文件 | 用途 | message 数量 | service 数量 |
|---|---|---|---|
| `agent.proto` | agent loop / model 调用 / tool_call 消息 | 11 | 1 (`AgentService`) |
| `session.proto` | 会话元信息 / CRUD | 9 | 1 (`SessionService`) |
| `event.proto` | append-only SessionEvent 日志 | 5 | 1 (`EventService`) |

**Phase 1 共 25 个 message + 3 个 service**。Phase 2 扩 `plugin.proto` / `sandbox.proto` / `model.proto`。

## 编码约定 (强制)

- **package**: `ma_harness.v1` (semver-versioned,锁)
- **字段名**: 全 snake_case
- **枚举值**: `MESSAGE_NAME_ENUM_VALUE_UPPER_SNAKE` (例 `AGENT_STATE_THINKING`)
- **0 值**: 必须是 `_UNSPECIFIED` 哨兵, 业务禁止用 0 表示"无"
- **时间**: 用 `google.protobuf.Timestamp`,**不**用 int64 millis
- **ID**: 全 UUID v4 字符串
- **JSON payload**: 业务字段进 `payload_json` (string), 避免加 nested message 改 schema

## codegen 集成

Week 2 起 `crates/ma_harness_proto/build.rs` 用 `tonic-build` 把上面 3 个 .proto 编成 Rust 代码:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/ma_harness/v1/agent.proto",
                "proto/ma_harness/v1/session.proto",
                "proto/ma_harness/v1/event.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
```

生成代码落 `target/` 不进 git。

## lint / 校验

- 用 `buf` 或 `protolock` 锁 wire 兼容 (Week 4 加)
- 用 `protoc-gen-validate` 加字段级校验 (Phase 2)

## 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-18 | 初版, agent / session / event 3 个 .proto, 25 message + 3 service |
