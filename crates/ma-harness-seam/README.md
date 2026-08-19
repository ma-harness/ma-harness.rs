# ma-harness-seam

> **The recommended public API for plugin authors.** Other `ma-harness-*` crates (cordis, plugin-macro, core) are internal building blocks; plugin code should depend only on this crate to be insulated from internal refactors.

Stable plugin API facade for the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) AI agent orchestrator.

## Features

- **Trait-isolated surface** — `Service`, `Plugin`, `Listener`, `AsyncListener`, `Disposable`, `AsyncDisposable`, `Tool`, `Command`, `Handler`. All `#[non_exhaustive]` so we can add methods without breaking your code.
- **Re-exported derives** — `DshService`, `DshListener`, `DshTool`, `DshCommand`, `DshHandler` (from `ma-harness-plugin-macro`)
- **Re-exported macros** — `ctx_key!`, `dsh_service_dual!`, `dsh_plugin_dual!`, `dsh_listener_priority!`
- **PluginLoader** — `list()` / `load_by_name(ctx, name)` / `load_all(ctx)` (topological order via Kahn)
- **PluginRegistry** — typed plugin registration with name → `Plugin` factory
- **PluginEntry / PluginManifest** — `inventory::submit!` registration from any crate in the binary
- **Helper re-exports** — `Context`, `CtxKey`, `EventType`, `ModelAdapter`, etc. so plugins only need one `use`

## Quick example

```rust
use ma_harness_seam::{Context, DshService, DshListener, Plugin, Service, dsh_service_dual, dsh_listener};
use ma_harness_core::EventType;

// 1. Define a service
#[derive(DshService)]
#[dsh_service_dual(name = "greet", ctor = "create")]
pub struct GreetService { name: String }

impl GreetService {
    pub fn create() -> Self { Self { name: "world".into() } }
    pub fn greet(&self) -> String { format!("Hello, {}!", self.name) }
}

// 2. Define a listener
#[derive(DshListener)]
pub struct CountSessions;

// 3. Run
let ctx = Context::new();
let svc = GreetService::create();
ctx.emit(ma_harness_core::SessionEvent::new("demo", EventType::SessionStart));
```

## Plugin loading (inventory)

```rust
use ma_harness_seam::{PluginLoader, inventory};

// In your plugin crate:
inventory::submit!(ma_harness_seam::PluginEntry::new("greet", || Box::new(GreetService::create())));
inventory::submit!(ma_harness_seam::PluginManifest::new("greet", &[]));  // no deps

// In your app:
let ctx = Context::new();
let names = PluginLoader::list();           // ["greet", ...]
PluginLoader::load_by_name(&ctx, "greet")?;  // install
PluginLoader::load_all(&ctx)?;               // topological order
```

## Stability

This crate is `0.1.0`. The trait surface is `#[non_exhaustive]` — adding methods is allowed without a major version bump. Removing / renaming methods requires `0.2.0`.

## Documentation

- [API docs (docs.rs)](https://docs.rs/ma-harness-seam)
- [ma-harness architecture](https://gitee.com/yifenma/ma-harness.rs)
- [Plugin authoring guide](https://gitee.com/yifenma/ma-harness.rs/blob/main/docs/plugin-authoring.md)

## License

MIT OR Apache-2.0
