# 04 — Plugins

> **Goal**: install a first-party plugin, write your own, and publish to
> the public registry.

[English](04-plugins.md) | [简体中文](../../zh-CN/user-guide/04-plugins.md)

## Prerequisites

- `mah` CLI installed (see [01-installation.md](01-installation.md))
- A plugin you want to use, or a Rust workspace to write one
- ~30 minutes for writing a plugin from scratch

## What is a plugin?

A plugin is a Rust crate that implements one or more of these traits
(defined in `ma-harness-seam`):

| Trait | Purpose |
|---|---|
| `Service` | Long-lived state (database connection, cache, etc.) |
| `Plugin` | Lifecycle hook (install / uninstall) |
| `Listener` | React to events |
| `Disposable` | Cleanup on scope exit |
| `Tool` | LLM-callable function (with name, description, JSON schema) |

Each plugin also declares typed keys in the context for its config.

## Step-by-step

### Step 1 — Install a first-party plugin (one-time)

The 6 first-party plugins are bundled with `mah`:

| Plugin | What it does | Typed keys |
|---|---|---|
| `bash` | Run shell commands | `MAX_RUNTIME_MS` |
| `fs` | Read/write/list files (sandboxed) | `READ_ALLOW_LIST`, `WRITE_ALLOW_LIST` |
| `web` | HTTP GET/POST (URL whitelist) | `EGRESS_ALLOW_LIST`, `TIMEOUT_MS` |
| `subagent` | Spawn a child agent | `MAX_DEPTH` |
| `skill` | Load `.skill/` files | `SKILLS_DIR` |
| `cordis` | Reflect on context (meta) | `INSPECT_DEPTH` |

To activate, just import them in your agent code:

```rust
use ma_harness_plugin_hello as _;  // auto-registers via inventory
use ma_harness_plugin_bash as _;
use ma_harness_plugin_fs as _;
use ma_harness_plugin_web as _;
use ma_harness_plugin_subagent as _;
use ma_harness_plugin_skill as _;
use ma_harness_plugin_cordis as _;
```

Or list them at runtime:

```bash
mah plugins
# Expected:
# Registered plugins (7 total):
#   - hello
#   - bash
#   - fs
#   - web
#   - subagent
#   - skill
#   - cordis
```

### Step 2 — Configure typed keys in your agent code

```rust
ctx.set(MAX_RUNTIME_MS, 30_000)        // 30 second timeout
    .set(READ_ALLOW_LIST, vec!["/tmp".to_string(), "/home/me/docs".to_string()])
    .set(EGRESS_ALLOW_LIST, vec!["https://api.example.com".to_string()])
    .set(MAX_DEPTH, 3);
```

The plugin reads these from the context on every call — change them at
runtime, no restart needed.

### Step 3 — Call a tool from your agent

```rust
let bash = ctx.service::<BashService>().await?;
let result = bash.run_command(ctx, "ls -la /tmp").await?;
println!("{}", result);
```

### Step 4 — Write your own plugin

Create a new crate:

```bash
cargo new --lib plugins/my-plugin
```

In `Cargo.toml`:

```toml
[dependencies]
ma-harness-cordis = { path = "../../crates/ma-harness-cordis" }
ma-harness-seam = { path = "../../crates/ma-harness-seam" }
ma-harness-plugin-macro = { path = "../../crates/ma-harness-plugin-macro" }
inventory = "0.3"
async-trait = "0.1"
```

In `src/lib.rs`:

```rust
use ma_harness_cordis::{Context, Service, Plugin};
use ma_harness_seam::Tool;
use ma_harness_plugin_macro::{dsh_service, dsh_tool, ctx_key};

// Typed key for config
ctx_key!(pub static MAX_ITEMS: usize = 100);

#[dsh_service]
pub struct MyService {
    // state fields
}

#[dsh_tool(
    name = "search_docs",
    description = "Search the local docs directory"
)]
impl MyService {
    pub async fn search_docs(&self, ctx: &Context, query: String) -> Result<String, String> {
        let max = *ctx.get(MAX_ITEMS).unwrap_or(&100);
        // ... implementation ...
        Ok(format!("found {} results for {}", max, query))
    }
}

pub struct MyPlugin {
    service: MyService,
}

impl Plugin for MyPlugin {
    fn name(&self) -> &str { "my-plugin" }
    fn install(&self, ctx: &Context) {
        ctx.inject(self.service.clone());
    }
}

// Auto-register on startup
inventory::submit! {
    pub fn register() -> Box<dyn Plugin> {
        Box::new(MyPlugin { service: MyService::new() })
    }
}
```

### Step 5 — Run your agent with the new plugin

In your agent binary:

```rust
use my_plugin as _;  // triggers inventory::submit!
```

Then run:

```bash
cargo run -p my-agent
mah plugins | grep my-plugin
# Expected: - my-plugin
```

### Step 6 — Publish to the registry

First, write `plugin.toml`:

```toml
[plugin]
name = "my-plugin"
version = "0.1.0"
description = "Searches local docs"
author = "your-name"
source = { type = "local", path = "../plugins/my-plugin" }
tags = ["search", "docs"]
```

Publish locally:

```bash
mah plugin publish plugin.toml
# Creates/updates ~/.ma-harness/registry.json
```

Export for GitHub Pages:

```bash
mah registry export --output docs/registry/registry.json
```

Commit + push:

```bash
git add docs/registry/registry.json
git commit -m "feat(registry): publish my-plugin @ 0.1.0"
git push origin main
```

The `registry-pages.yml` workflow (if enabled) deploys to
<https://ma-harness.github.io/ma-harness.rs/registry/> within ~2 minutes.

### Step 7 — Install from registry (other users)

Once published, anyone can:

```bash
# List available plugins
mah registry list

# Install
mah plugin install my-plugin@0.1.0
```

## Verify

After step 5:

```bash
mah plugins
# Expected: - my-plugin listed

mah run "search docs for async"
# Expected: agent uses MyService::search_docs
```

After step 6 (publish):

```bash
# On the published GH Pages site:
curl -s https://ma-harness.github.io/ma-harness.rs/registry/registry.json | jq '.plugins | keys'
# Expected: includes "my-plugin"
```

## What's next

- **Validate** your plugin's behavior with conformance tests —
  see [05-conformance.md](05-conformance.md)
- **Deploy** to production server — see [03-server.md](03-server.md)
- **Troubleshoot** common plugin issues — see [06-troubleshooting.md](06-troubleshooting.md)

## Reference

- Plugin schema: [docs/en/plugin-schema-v1.md](../plugin-schema-v1.md)
- Macro design: [docs/en/macro-design.md](../macro-design.md)
- Registry workflow: [docs/en/operations/registry-pages.md](../operations/registry-pages.md)
- hello plugin source: [../../../plugins/ma-harness-plugin-hello/](../../../plugins/ma-harness-plugin-hello/)

## Troubleshooting

### "plugin not registered" when running agent

Make sure you import the plugin crate in your binary:

```rust
use my_plugin as _;
```

The `as _` is important — it forces the crate's `inventory::submit!` to run.

### "duplicate plugin name" error

Two plugins have the same `name()`. Either rename one or guard with a
feature flag in your `Cargo.toml`:

```toml
[features]
default = []
my-plugin-b = []
```

### Tool not visible to LLM

Tool schema generation requires the `Tool` trait. Make sure your impl
uses `#[dsh_tool]`:

```rust
#[dsh_tool(name = "...", description = "...")]
impl MyService { ... }
```

Without the macro, the tool is registered as a service but doesn't show
up to the LLM.
