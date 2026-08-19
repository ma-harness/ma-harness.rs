# ma-harness-plugin-macro

> **Note**: This crate is part of the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) workspace. Plugin authors typically don't need to depend on this crate directly — `ma-harness-seam` re-exports the relevant derives via `ma_harness_seam::dsh_service`, `ma_harness_seam::dsh_listener`, etc.

Procedural macros for the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) plugin framework.

## Features

- **`#[derive(DshService)]`** — auto-implement `ma_harness_cordis::Service` for your service struct
- **`#[derive(DshListener)]`** — auto-implement `ma_harness_cordis::Listener<E>` for your listener struct
- **`#[derive(DshTool)]`** — declare a callable tool with JSON Schema args
- **`#[derive(DshCommand)]`** — declare a CLI subcommand
- **`#[derive(DshHandler)]`** — declare a typed event handler
- **`#[dsh_service_dual(name, ctor)]`** — generate both cordis + seam `Service` impls in one attribute
- **`#[dsh_plugin_dual(name, install)]`** — generate both cordis + seam `Plugin` impls
- **`#[dsh_listener_priority(priority = N)]`** — attach a priority constant to a listener struct
- **`ctx_key!(name)`** — typed-key macro with compile-time snake_case validation

## Quick example

```rust
use ma_harness_seam::{DshService, Service, Context};
use ma_harness_plugin_macro::dsh_service_dual;

#[derive(DshService)]
#[dsh_service_dual(name = "hello", ctor = "create")]
pub struct HelloService {
    name: String,
}

impl HelloService {
    pub fn create() -> Self {
        Self { name: "world".to_string() }
    }

    pub fn greet(&self) -> String {
        format!("Hello, {}!", self.name)
    }
}

// Now usable as both:
//   ctx.service::<dyn HelloService>()        // via cordis
//   ctx.service::<dyn ma_harness_seam::Service>()  // via seam (opaque)
```

## Stability

This crate is `0.1.0`. Attribute syntax may evolve as we learn what plugin authors actually need; we will bump the major version if we have to break call sites.

## Documentation

- [API docs (docs.rs)](https://docs.rs/ma-harness-plugin-macro)
- [ma-harness architecture](https://gitee.com/yifenma/ma-harness.rs)
- [Macro design doc](https://gitee.com/yifenma/ma-harness.rs/blob/main/docs/macro-design.md)

## License

MIT OR Apache-2.0
