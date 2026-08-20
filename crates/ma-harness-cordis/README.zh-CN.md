# ma-harness-cordis (中文 / 简体中文)

[English](README.md) | [简体中文](README.zh-CN.md)

> **备注**: 本 crate 是 [ma-harness](https://gitee.com/yifenma/ma-harness.rs) workspace 的一部分. 大多数用户应该依赖 `ma-harness-seam`, 它是面向公共的 facade. `ma-harness-cordis` 暴露 ma-harness 内部使用的原始 DI 容器, 类型化 key 存储, 以及 plugin / listener / disposable 框架.

为 [ma-harness](https://gitee.com/yifenma/ma-harness.rs) AI agent 编排器提供的 Cordis 风格依赖注入容器.

## 特性

- **Context** — DI 容器, 持有类型化 key 存储, services, plugins, listeners, disposables
- **类型化 keys** — 编译期 snake_case 校验的 `CtxKey<T>`, 保证跨插件 state 安全
- **Service 注册表** — `inject` / `service` 强类型, 不用字符串 key
- **Plugin loader** — install / uninstall, 带依赖追踪
- **Listener 系统** — sync + async listener, 按 priority 顺序派发
- **Disposable 作用域** — sync + async, LIFO 释放
- **延迟事件队列** — 重新进入的 `emit` 不 panic (Phase 2.7)

## 快速示例

```rust
use ma_harness_cordis::{Context, CtxKey};

// 1. 定义类型化 key (编译期 snake_case 检查)
static COUNTER: CtxKey<u32> = ma_harness_cordis::ctx_key!("counter");

// 2. 创建 context
let ctx = Context::new();
ctx.set(COUNTER, 42_u32);

// 3. 读回
assert_eq!(ctx.get(COUNTER), Some(42));
```

## 稳定性

本 crate 当前 `0.1.0`. API 表面是 **ma-harness 内部使用**, minor 版本之间可能变更. 插件作者应使用 [`ma-harness-seam`](https://crates.io/crates/ma-harness-seam) 获得稳定的 API 表面.

## 文档

- [API docs (docs.rs)](https://docs.rs/ma-harness-cordis)
- [ma-harness 架构](https://gitee.com/yifenma/ma-harness.rs)
- [Macro 设计文档](https://gitee.com/yifenma/ma-harness.rs/blob/main/docs/macro-design.md)

## 许可证

MIT OR Apache-2.0
