# ma-harness-cordis

> **Note**: This crate is part of the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) workspace. Most users should depend on `ma-harness-seam` instead, which is the public-facing facade. `ma-harness-cordis` exposes the raw DI container, typed key storage, and plugin/listener/disposable framework used internally by ma-harness.


[English](README.md) | [简体中文](README.zh-CN.md)


> **Note**: This crate is part of the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) workspace. Most users should depend on `ma-harness-seam` instead, which is the public-facing facade. `ma-harness-cordis` exposes the raw DI container, typed key storage, and plugin/listener/disposable framework used internally by ma-harness.

Cordis-style dependency injection container for the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) AI agent orchestrator.

## Features

- **Context** — DI container holding typed-key storage, services, plugins, listeners, disposables
- **Typed keys** — compile-time snake_case validated `CtxKey<T>` for safe cross-plugin state
- **Service registry** — `inject` / `service` strong-typed, no string keys
- **Plugin loader** — install / uninstall with dependency tracking
- **Listener system** — sync + async listeners, priority-ordered dispatch
- **Disposable scopes** — sync + async, LIFO release
- **Deferred event queue** — re-entrant `emit` doesn't panic (Phase 2.7)

## Quick example

```rust
use ma_harness_cordis::{Context, CtxKey};

// 1. Define a typed key (compile-time snake_case check)
static COUNTER: CtxKey<u32> = ma_harness_cordis::ctx_key!("counter");

// 2. Create a context
let ctx = Context::new();
ctx.set(COUNTER, 42_u32);

// 3. Read back
assert_eq!(ctx.get(COUNTER), Some(42));
```

## Stability

This crate is currently `0.1.0`. API surface is **internal to ma-harness** and may change between minor versions. Plugin authors should use [`ma-harness-seam`](https://crates.io/crates/ma-harness-seam) for a stable surface.

## Documentation

- [API docs (docs.rs)](https://docs.rs/ma-harness-cordis)
- [ma-harness architecture](https://gitee.com/yifenma/ma-harness.rs)
- [Macro design doc](https://gitee.com/yifenma/ma-harness.rs/blob/main/docs/macro-design.md)

## License

MIT OR Apache-2.0
