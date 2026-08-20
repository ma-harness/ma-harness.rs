# 04 — Plugins

> **Goal**: install a first-party plugin, write your own, and publish to
> the public registry.
> Plugin source code is 100% cross-platform Rust — the OS-specific parts
> are the **build environment** and **path conventions** (covered in
> [Build env & paths per OS](#build-env-paths-per-os) below).

[English](04-plugins.md) | [简体中文](../../zh-CN/user-guide/04-plugins.md)

## Pick your OS (for the build env & paths section)

| OS | Jump to |
|---|---|
| **Linux** | [Linux build env & paths](#linux) |
| **macOS** | [macOS build env & paths](#macos) |
| **Windows** | [Windows build env & paths](#windows) |

If you already followed [01-installation.md](01-installation.md) and have
`cargo build` working, you can skip ahead to [Step 1](#step-1-install-a-first-party-plugin-one-time).

## Prerequisites (all OSes)

- `mah` CLI installed (see [01-installation.md](01-installation.md))
- A working Rust toolchain (`cargo build` works) — see
  [Build env & paths per OS](#build-env-paths-per-os) if you haven't done this yet
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

## Step 1 — Install a first-party plugin (one-time)

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

## Step 2 — Configure typed keys in your agent code

```rust
ctx.set(MAX_RUNTIME_MS, 30_000)        // 30 second timeout
    .set(READ_ALLOW_LIST, vec!["/tmp".to_string(), "/home/me/docs".to_string()])
    .set(EGRESS_ALLOW_LIST, vec!["https://api.example.com".to_string()])
    .set(MAX_DEPTH, 3);
```

The plugin reads these from the context on every call — change them at
runtime, no restart needed.

## Step 3 — Call a tool from your agent

```rust
let bash = ctx.service::<BashService>().await?;
let result = bash.run_command(ctx, "ls -la /tmp").await?;
println!("{}", result);
```

---

## Build env & paths per OS

The plugin **source code** is identical across OSes. The differences are:

1. **Build toolchain** — `cargo`, linker, native deps (already covered in
   [01-installation.md](01-installation.md); this section is a quick reminder)
2. **Path conventions** — the registry file location, home dir, slash vs backslash

### Linux

#### Build env

```bash
# Reuse what you installed for `mah` itself:
rustc --version
cargo --version
# You also need a C linker (build-essential / dnf / pacman — see 01)
```

#### Path conventions

| What | Linux path |
|---|---|
| User home | `$HOME` (~ `/home/<you>`) |
| Plugin registry | `~/.ma-harness/registry.json` |
| Plugin source layout | `~/projects/my-plugin/...` (your choice) |
| `Cargo.toml` workspace | `~/projects/ma-harness.rs/` |
| Native deps for build | installed via `apt` / `dnf` / `pacman` (e.g. `libssl-dev`) |

#### Cross-compile a plugin for another OS

```bash
# From Linux, build a Windows .exe of your plugin (for testing):
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu -p my-plugin
# Binary at target/x86_64-pc-windows-gnu/release/<name>.exe
```

> Linux → Windows: install `mingw-w64` (`sudo apt install mingw-w64`) for the
> cross-linker. The other direction (Windows → Linux) is harder; use WSL.

### macOS

#### Build env

```bash
xcode-select -p   # /Library/Developer/CommandLineTools — should be set
rustc --version
cargo --version
```

If `xcode-select` is missing, run `xcode-select --install` (see 01).

#### Path conventions

| What | macOS path |
|---|---|
| User home | `$HOME` (~ `/Users/<you>`) |
| Plugin registry | `~/.ma-harness/registry.json` |
| Plugin source layout | `~/Developer/my-plugin/...` (Xcode convention) or wherever |
| `Cargo.toml` workspace | `~/Developer/ma-harness.rs/` |
| Native deps for build | `brew install pkg-config openssl` |

> Apple Silicon vs Intel: `~/.ma-harness/registry.json` is the same
> `$HOME` path, but Homebrew installs to `/opt/homebrew` vs `/usr/local`.
> If your plugin needs OpenSSL on Apple Silicon:
> `export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl/lib/pkgconfig"`

#### Cross-compile a plugin for another OS

```bash
# macOS → Linux (rare, but works for static binaries):
rustup target add x86_64-unknown-linux-musl
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-unknown-linux-musl -p my-plugin
```

> Cross-compile is rarely needed for plugins — most teams build on each
> target OS. Use this only for CI matrix or static-binary distribution.

### Windows

#### Build env

```powershell
rustc --version
cargo --version
# You also need MSVC linker (Visual Studio Build Tools) — see 01
```

If you get `error: linker 'link.exe' not found`, open
**"x64 Native Tools Command Prompt for VS"** and run from there.

#### Path conventions

| What | Windows path |
|---|---|
| User home | `$env:USERPROFILE` (~ `C:\Users\<you>`) |
| Plugin registry | `$env:USERPROFILE\.ma-harness\registry.json` |
| Plugin source layout | `C:\Users\<you>\source\my-plugin\...` (VS convention) |
| `Cargo.toml` workspace | `C:\Users\<you>\source\ma-harness.rs\` |
| Native deps for build | installed via `vcpkg` or vcpkg/Conan |

> **Use forward slashes in `Cargo.toml`** (`path = "../../crates/..."`).
> Cargo normalizes on Windows but mixing `/` and `\` is error-prone.

#### Cross-compile a plugin for another OS

```powershell
# Windows → Linux (common for CI):
rustup target add x86_64-unknown-linux-gnu
# Then use a Linux container / WSL / a Linux CI runner. Cross from Windows
# without a Unix toolchain is not practical for most use cases.
```

> Plugin development on Windows is most comfortable inside **WSL** —
> follow the [Linux](#linux) section there.

---

## Step 4 — Write your own plugin

The example below is cross-platform. The crate will compile on any OS that
has a working Rust toolchain.

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

Build and check (works on all 3 OSes):

```bash
cargo build -p my-plugin
cargo test -p my-plugin        # if you have tests
```

## Step 5 — Run your agent with the new plugin

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

## Step 6 — Publish to the registry

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

Publish locally (writes to the registry file — see path table for your OS):

```bash
mah plugin publish plugin.toml
# Linux / macOS:  creates/updates ~/.ma-harness/registry.json
# Windows:        creates/updates %USERPROFILE%\.ma-harness\registry.json
```

Export for GitHub Pages:

```bash
# All OSes (relative path from repo root)
mah registry export --output docs/registry/registry.json
```

Commit + push (Linux / macOS / Git Bash on Windows):

```bash
git add docs/registry/registry.json
git commit -m "feat(registry): publish my-plugin @ 0.1.0"
git push origin main
```

Or in Windows PowerShell:

```powershell
git add docs/registry/registry.json
git commit -m "feat(registry): publish my-plugin @ 0.1.0"
git push origin main
```

The `registry-pages.yml` workflow (if enabled) deploys to
<https://ma-harness.github.io/ma-harness.rs/registry/> within ~2 minutes.

## Step 7 — Install from registry (other users)

Once published, anyone can:

```bash
# List available plugins (uses default registry path for your OS)
mah registry list

# Install
mah plugin install my-plugin@0.1.0
```

To use a custom registry path:

```bash
# Linux / macOS
mah plugin install my-plugin@0.1.0 --registry /path/to/registry.json

# Windows PowerShell
mah plugin install my-plugin@0.1.0 --registry C:\path\to\registry.json
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
# On the published GH Pages site (any OS):
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

### "registry file not found" on `mah plugin publish`

The default registry path doesn't exist yet. Either:

- Run `mah plugin publish` (creates it on first publish), or
- Pass `--registry` explicitly:
  - Linux/macOS: `--registry ~/.ma-harness/registry.json`
  - Windows: `--registry $env:USERPROFILE\.ma-harness\registry.json`

### Windows: `path = "..\..\crates\..."` doesn't work in `Cargo.toml`

Cargo only accepts forward slashes. Use:

```toml
ma-harness-cordis = { path = "../../crates/ma-harness-cordis" }
```

### Linux: `error: linker 'cc' not found` while building a plugin

Same as install — install the C toolchain:

```bash
sudo apt install -y build-essential     # Debian / Ubuntu
sudo dnf install -y gcc gcc-c++ make    # Fedora
sudo pacman -S --needed base-devel      # Arch
```

### macOS: OpenSSL not found when building a plugin

```bash
brew install openssl
# Apple Silicon:
export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl/lib/pkgconfig"
# Intel:
export PKG_CONFIG_PATH="$(brew --prefix openssl)/lib/pkgconfig"
```

### Windows: `link.exe not found` while building a plugin

Open **"x64 Native Tools Command Prompt for VS"** (or run `vcvarsall.bat x64` in
your current PowerShell) so MSVC is on `PATH`, then re-run `cargo build`.

If you don't have VS Build Tools installed, follow the
[Windows install steps](01-installation.md#windows).
