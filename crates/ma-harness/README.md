# ma-harness (umbrella crate)

> **One `use ma_harness::*` for the full ma-harness SDK.**

This crate re-exports the public API of every first-party ma-harness
sub-crate under feature flags. It's a **convenience** layer on top of
the 36 first-party crates — use it when you want the whole SDK with
one dependency, or skip it and depend on the individual crates when
you want to pick exactly the pieces you need.

---

## Status

- **publish = false** (workspace-internal) as of 2026-09-07. The
  umbrella depends on 16 sub-crates that are on crates.io *plus* 11
  P14 / 6 P15 sub-crates that are workspace-internal. The 7
  SSL-blocked crates (compaction / lsp / web / todo / session /
  profile / context / guard / skill / etc.) gate publication.
- Re-enable `publish = true` in `Cargo.toml` once those 7 land.
- **In the meantime**, the umbrella is fully usable inside the
  `ma-harness.rs` workspace — `cargo build` / `cargo test` /
  `cargo run -p ma-harness-cli` all work via path deps.

## Quick start (in-workspace)

Add to your crate's `Cargo.toml`:

```toml
[dependencies]
ma-harness = { path = "../ma-harness" }  # or relative to your crate
```

```rust
use ma_harness::*;

// Cordis DI
let ctx = Context::new();

// LLM (default features: core + model)
let adapter = OpenaiAdapter::new("sk-...");
let req = ModelRequest::new(vec![ModelMessage::user("hi")]);
```

## Quick start (external, once publish = true)

```toml
[dependencies]
ma-harness = "0.1"
# or pick features:
# ma-harness = { version = "0.1", default-features = false, features = ["core", "server", "plugin"] }
```

## Feature matrix

| Feature     | Re-exports                                             | Notes                       |
|-------------|--------------------------------------------------------|-----------------------------|
| `core`      | `ma-harness-cordis` + `ma-harness-core` + `ma-harness-seam` + `ma-harness-plugin-macro` | Foundation (types, DI, plugin facade, proc-macro) |
| `model`     | `ma-harness-model`                                     | OpenAI / Anthropic adapters + retry + vision. Depends on `core`. |
| `plugin`    | `ma-harness-registry`                                  | Plugin registry (npm-style). Depends on `core`. |
| `bundle`    | `ma-harness-bundle`                                    | Lockfile install. Depends on `plugin`. |
| `sandbox`   | `ma-harness-sandbox`                                   | Landlock / Seatbelt / Stub enforcer. |
| `code`      | `ma-harness-code`                                      | Wasmtime Code Mode. |
| `dag`       | `ma-harness-dag`                                       | DAG orchestration. |
| `artifact`  | `ma-harness-artifact`                                  | Vibe coding artifact viewer. |
| `server`    | `ma-harness-server`                                    | Salvo HTTP server. |
| `tui`       | `ma-harness-tui`                                       | Ratatui TUI dashboard. |
| `p14`       | 11 P14 sub-crates (subprocess, shell, skill, compaction, lsp, web, todo, session, profile, context, guard) | P14 ctx.\* seams in one feature. |
| `p15`       | 6 P15 sub-crates (workflow, webhook, settings, credentials, hooks, self-modification) | P15 features in one feature. |
| `web-ui`    | `ma-harness-web-ui`                                    | P15.1 future work. |
| `pty`       | `ma-harness-terminal`                                  | P15.2 future work. |
| `full`      | All of the above                                       | Equivalent to the full workspace SDK. |

**Default features**: `core` + `model`. Matches the most common
library use case: types and an LLM adapter.

## Tests

```bash
cargo test -p ma-harness                       # default: core + model
cargo test -p ma-harness --all-features        # full
cargo test -p ma-harness --no-default-features # metadata only
```

35 smoke tests verify the feature-gated re-export surface compiles
and resolves to real types under each feature combination. The
underlying functionality is exercised by each sub-crate's own
test suite.

## Design notes

- The umbrella crate is **additive** — every existing first-party
  crate continues to be published independently. Users who prefer
  granular dependencies keep their current setup.
- Re-exports use `pub use` (not `pub mod`), so the umbrella
  crate's own version is what shows in semver-compatible
  resolution. Sub-crates stay version-locked via the workspace
  `version.workspace = true`.
- Trait re-exports use the bare trait name (e.g. `ShellService`,
  `TodoStore`); library users should write `dyn ShellService`
  when needed.
- Static items like `ma-harness-compaction::COMPACTION_STRATEGY`
  (a `CtxKey`) are re-exported; pass `&` to take a reference.

## Layout

```
crates/ma-harness/
├── Cargo.toml              # feature-gated optional deps
├── src/lib.rs              # feature-gated pub use re-exports
└── tests/smoke.rs          # 35 compile-time type-existence tests
```

## License

Dual-licensed under MIT / Apache-2.0, same as the rest of
`ma-harness.rs`.
