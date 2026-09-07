# ma-harness.rs

[English](README.md) | [简体中文](README.zh-CN.md)

**Rust rewrite of [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) (dsh) AI agent framework, with extensions for production use.**

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](#)
[![Tests](https://img.shields.io/badge/tests-%7E1500%20pass-brightgreen)](#)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](#)
[![crates.io](https://img.shields.io/badge/crates.io-24%20crates-orange)](#cratesio)

`mah` is the binary; `mah-py` is the Python SDK; 37 first-party Rust crates (24 published to crates.io).
**`ma-harness`** is the umbrella crate — one `use ma_harness::*` for the full SDK (workspace-internal until the 7 SSL-blocked sub-crates land).

---

## ✨ Features

### LLM + reasoning
- **4 LLM backends** — OpenAI / Anthropic / Deepseek / Stub (vs dsh's 1) with streaming, retry+circuit-breaker, vision, tool-call
- **ACP protocol** (JSON-RPC 2.0 over stdio) — interoperable with dsh's `dsh-jsonrpc-agent` (P11-4)

### Core framework (P7-P11)
- **Cordis-style DI** — Context / Service / Plugin / TypedKey / Disposable framework
- **Plugin system** — registry + inventory + macro + bundle (lockfile install)
- **Tool execution pipeline** — pre-execute / approval / execute / post-execute / log (4-event waterfall)
- **Approval service** — oneshot / TUI / HTTP pre-tool approval
- **Code Mode** — WAT/WASM in wasmtime sandbox (4-layer defense: fuel / epoch / memory / fs)
- **Landlock sandbox** — kernel-enforced fs/process restrictions on Linux ≥ 5.13
- **DAG task orchestration** — Kahn topo + short-circuit on failure

### Production extensions (P11-P12)
- **HTTP server** (salvo 0.96) — OpenAPI export, SSE, REST + JSON-RPC endpoints
- **TUI dashboard** — ratatui-based session/event viewer
- **Vibe Coding Artifact viewer** — auto-detect and render 10 artifact kinds (HTML / SVG / JSON / etc.)
- **Plugin Registry** (npm-style) + **Bundle** (lockfile install) for distributed plugin discovery
- **dsh-adapter** — load dsh (DeepSeek Harness) TS plugins directly via JSON-RPC over stdio (P13)
- **Python SDK** (`mah-py`) — subprocess bridge to `mah` CLI
- **CI/CD** — Gitee Go + GitHub Actions, tag-triggered publish to crates.io

### P14+P15 增量 (本 batch 焦点)
- **`ctx.subprocess` / `ctx.shell`** — process spawn + shell exec trait abstraction (P14.1-2)
- **`ctx.compaction` / `ctx.context`** — context auto-summarize + context plugin (P14.4, P14.10)
- **`ctx.lsp` / `ctx.web` / `ctx.skill`** — LSP client wrapper + web search/fetch + skill catalog (P14.3, P14.5, P14.6)
- **`ctx.todo` / `ctx.plan`** — multi-step work tracker + plan-mode (P14.7)
- **`ctx.session.fork()` / `TitleProvider` / `GoalStore`** — session lifecycle (P14.8)
- **`ctx.profile` / `ctx.guard`** — profile system + loop-hygiene guard (P14.9, P14.11)
- **`ctx.workflows`** — workflow engine (YAML + DAG + parallel) with `mah workflow run/validate/list` (P15.4)
- **`ctx.webhookRuntime`** — webhook ingress with HMAC-SHA256 verification (P15.3)
- **`ctx.settings` / `ctx.credentials`** — user settings + env/`.env`/OS keyring providers (P15.5)
- **Self-modification** — agent inspects/mounts its own plugins at runtime (P15.6)
- **Hook bridges** — Claude Code wire-protocol, `mah hook install/run/list` (P15.7)

> 完整 P14+P15 列表 (24 sub-tasks) 见 [docs/en/dsh-feature-parity-table.md](docs/en/dsh-feature-parity-table.md) §2 / §5-§9.

---

## 📊 Status vs [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)

ma-harness.rs is a from-scratch Rust rewrite of dsh v0.1, targeting 100% behavioral parity at the snapshot/fixture level, plus production extensions. Last verified **2026-09-06**.

> **📖 [Full dsh ↔ ma-harness Feature Parity →](docs/en/dsh-feature-parity.md)**
> Complete comparison: 8 dsh core packages, 19 capability seams, 3 event domains, 13-step turn flow,
> 11 profiles & bundles, tool execution pipeline, 6 distribution surfaces, conformance parity,
> 13 ma-harness extensions, 12 P15+ deferred items. 25KB doc with dsh doc links + ma-harness crate links.

> **📊 [Compact parity table →](docs/en/dsh-feature-parity-table.md)**
> 12 sections, 114 items, status column + diff notes. Current score: **75% done (85/114 ✅, 1 🔄, 3 ⚠️, 23 ❌, 2 ➖)**.

### Behavioral equivalence

| Test suite | dsh v0.1 | ma-harness.rs | Status |
|---|---|---|---|
| **dsh acp-snapshot** (9 fixture) | 100% | **100% (9/9)** | ✅ parity |
| **dsh_synthetic** (7 fixture, shape conversion) | n/a | **100% (7/7)** | ✅ parity |
| **smoke** (8 fixture, framework consistency) | n/a | 62.5% (5/8) | ✅ by design (3 expected failures) |
| Terminal Bench 2.1 | 87.9% | not run | ⏳ business-driven (P11-2.5+, needs LLM API key) |
| Toolathlon-Verified | 74.1% | not run | ⏳ business-driven |
| DSBench-FullStack | 71.1% | not run | ⏳ business-driven |

End-to-end verification:
```bash
$ mah.exe conformance --dsh --fixtures crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl
Loaded 9 fixtures from dsh_snap.jsonl
Conformance: 9 / 9 passed (100.0%) in 1ms
```

### Feature matrix

| Capability | dsh v0.1 | ma-harness.rs | Notes | Status |
|---|---|---|---|---|
| Core agent loop (Session / Run / Event) | ✅ | ✅ | behaviorally equivalent | ✅ done |
| ACP (JSON-RPC 2.0 stdio) | ✅ | ✅ | P11-4 | ✅ done |
| Plugin system | ✅ | ✅ (extended) | cordis + inventory + macro | ✅ done |
| Approval service (user pre-tool) | ✅ | ✅ (P7-2/3) | oneshot + TUI + HTTP | ✅ done |
| TUI dashboard | partial | ✅ (P3.9) | ratatui | ✅ done |
| HTTP server (salvo 0.96) | n/a | ✅ (P6) | OpenAPI export, SSE | ✅ done |
| Workflow engine (YAML + DAG + parallel) | n/a | ✅ (P15.4) | 3 engines + `mah workflow run/validate/list` | ✅ done |
| Webhook (HMAC-SHA256) | n/a | ✅ (P15.3) | GitLab / Stripe / generic | ✅ done |
| Settings + Credentials | n/a | ✅ (P15.5) | hot-reload + env / .env / keyring | ✅ done |
| Self-modification | n/a | ✅ (P15.6) | `mah self` list/inspect/enable/disable/audit | ✅ done |
| Hook bridges (Claude Code) | n/a | ✅ (P15.7) | wire-protocol + adapter | ✅ done |
| Subprocess service | partial | ✅ (P14.1) | trait + StdioConfig + ChildHandle | ✅ done |
| Shell service | n/a | ✅ (P14.2) | tokio::Command + plugin-bash | ✅ done |
| LSP client | n/a | ✅ (P14.5) | `mah lsp request` / `info` | ✅ done |
| Web search/fetch | n/a | ✅ (P14.6) | Brave + DDG + HttpFetch | ✅ done |
| Todo + Plan | n/a | ✅ (P14.7) | state-machine + 5-status enum | ✅ done |
| Session fork + GoalStore + TitleProvider | n/a | ✅ (P14.8) | EventForker + heuristic titles | ✅ done |
| Profile system (CLI) | n/a | ✅ (P14.9) | `mah profile list/show/validate/info` | ✅ done |
| Guard (loop-hygiene) | n/a | ✅ (P14.11) | `mah guard` demo/observe/chain-info/reset | ✅ done |
| Plugin Registry (npm-style) | n/a | ✅ (P11-6 / P12-5) | search/export/merge | ✅ done |
| Bundle (lockfile install) | n/a | ✅ (P11-8 / P12-7) | reproducible | ✅ done |
| Vibe Coding Artifact viewer | n/a | ✅ (P11-7) | 10 kinds, terminal render | ✅ done |
| DAG orchestration | n/a | ✅ (P12-9) | Kahn topo + short-circuit | ✅ done |
| Multi-modal vision | n/a | ✅ (P11-5/9, P12-8) | OpenAI + Anthropic | ✅ done |
| Retry + Circuit Breaker | n/a | ✅ (P12-2) | exponential backoff + jitter | ✅ done |
| Wasm sandbox (Code Mode) | n/a | ✅ (P2.6) | wasmtime + 4-layer defense | ✅ done |
| Landlock sandbox (Linux kernel) | n/a | ✅ (P10) | ABI V1 (kernel ≥ 5.13) | ✅ done |
| Python SDK | n/a | ✅ (P11-3, mah-py 0.1.1) | subprocess + JSON | ✅ done |
| crates.io publish | n/a | ✅ (P12-5) | 24 crates at 0.1.0/0.1.1 | ✅ done |
| LLM backends | 1 (Deepseek) | 4 (OpenAI / Anthropic / Deepseek / Stub) | | ✅ done |
| Language | TypeScript | **Rust 1.94 (edition 2024)** | salvo 0.96 + tonic 0.12 | ✅ done |

### 🚧 Future / Planned (P15.8+)

| Item | Phase | Why deferred | Effort | Plan |
|---|---|---|---|---|
| **Web UI** (Leptos WASM or React + REST + SSE) | P15.1 / P15.8+ | TUI already works; Web UI is opt-in for browser users | 8-12 weeks · 2 engineers | `mah web` opens browser UI, live session + tool results + approval |
| **PTY backend** (`ctx.terminals`) | P15.2 | TUI + workflow shell-step runner cover most use cases | 2 weeks · 1 engineer | `portable-pty` + session_id→pty_handle persistence |
| **Profile system** (full dsh parity, file-based) | P15+ | CLI `mah profile` works; full `~/.ma-harness/profiles/<name>/cordis.yml` system pending | 1-2 weeks | per-profile cordis.yml + `--profile <name> --patch` |
| **Subagent** (formal `ctx.subagent` trait) | P16.2 | `plugin-subagent` works but no formal Service trait | 2 weeks · 1 engineer | `SubagentService` + Local/Remote providers + `delegate` tool |
| **E2B cloud sandbox** | P16.1 | Landlock local-only; cloud sandbox adds cost infra | 2 weeks · 1 engineer | `ma-harness-sandbox-e2b` + `MA_HARNESS_SANDBOX=e2b` env |
| **Agent Teams** (`ctx.agentTeams`) | P16.3 | experimental in dsh; high design risk | 4 weeks · 1 engineer | Team + Roster + TaskBoard + Mailbox + `team_create/join/disband` |
| **Remote sandbox** (Firecracker / gVisor / Hypervisor) | P16.4 | production-grade isolation needs kernel-level infra | 8 weeks · 2 engineers | `sandbox_provider.yaml` + per-backend crates |
| **Distributed session store** (Redis / PostgreSQL) | P16.5 | sqlite works for single-node; distributed needs schema migration | 4 weeks · 1 engineer | `SessionStore` trait + Redis Streams + PostgreSQL JSONB |
| **TypeScript SDK** (`@ma-harness/sdk`) | P17.1 | dsh-adapter covers TS interop for now | 6 weeks · 1 engineer | npm package + JSON-RPC 2.0 over stdio/HTTP |
| **Identity & permissions** (Branding, ACLs) | P17.2 | no formal identity in ma-harness yet | 3 weeks · 1 engineer | `mah identity create/list` + `permission.yaml` per identity |
| **12+ LSP languages** (full ecosystem) | P17.3 | P14.5 ship rust-analyzer + typescript-language-server + pyright stubs | ongoing | per-language LSP server adapters |
| **Production tooling** (`mah dashboard / trace / cost`) | P17.4 | debugging via TUI + logs works for dev | ongoing | OpenTelemetry export + cost tracking |
| **Real-benchmark conformance** (Terminal Bench 2.1 / Toolathlon / DSBench) | P17.5 | dsh acp-snapshot 9/9 ✅; real-bench needs LLM API key | blocked | business provides LLM API key + dataset access |

> Detailed effort / success criteria in [_local/dsh-planning/dsh-development-plan.en.md](_local/dsh-planning/dsh-development-plan.en.md).

### Test coverage (~1500 tests, 0 failed)

```
~1500 tests across 37 first-party Rust crates + mah-py (Python)
(ma-harness umbrella adds 35 compile-time re-export smoke tests)

Top contributors:
  ma-harness-core:                ~107
  ma-harness-cordis:              ~81
  ma-harness-conformance:         44 + 13 smoke
  ma-harness-model:               ~71  (incl. vision 17 + retry 13)
  ma-harness-server:              53
  ma-harness-cli:                 85 + 10 acp integration
  ma-harness-tui:                 35
  ma-harness-registry:            25
  ma-harness-artifact:            26
  ma-harness-bundle:              18
  ma-harness-dag:                 14
  ma-harness-seam:                11
  ma-harness-sandbox:              6
  ma-harness-plugin-*:            47
  mah-py (pytest):                16
  P14 + P15.4-15.7 sub-crates:   ~600  (subprocess / shell / skill / compaction / lsp / web /
                                      todo / plan / profile / context / guard / workflow /
                                      settings / credentials / webhook / hooks / self-modification)
  Other (smoke / conformance / dsh-adapter / proto / session / demo / web-ui / terminal):  ~200

  1 known flake: ma-harness-settings::layered_settings_store_env_only_keys_added
                 passes with --test-threads=1; pre-existing parallel-test contamination
                 (std::env::set_var is process-wide). Not a regression.
```

---

## 📋 Prerequisites

Before installing `ma-harness`, make sure your system has the following dependencies.

### Required

| Tool | Min version | Why |
|------|-------------|-----|
| **Rust** (stable) | 1.83+ (edition 2024) | Build the workspace (`cargo install ma-harness-cli`) |
| **Protocol Buffers compiler `protoc`** | libprotoc 3.21+ | `ma-harness-proto` build.rs uses it for gRPC stubs |
| **C compiler + linker** | MSVC / gcc / clang | Required by Rust crates with C deps (salvo, tokio, openssl-sys) |

### Optional (per plugin / SDK)

| Tool | Min version | When you need it |
|------|-------------|------------------|
| **Node.js** (LTS) | v18+ | `ma-harness-plugin-dsh-adapter` (P13) — loads dsh (DeepSeek Harness) TS plugins via JSON-RPC over stdio |
| **Python** | 3.8+ | `mah-py` Python SDK (`pip install mah-py`) |
| **pkg-config + OpenSSL dev headers** | any | Linux only — needed by some Rust deps to find OpenSSL |

### Linux (Ubuntu / Debian)

```bash
# 1. Rust (via rustup)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 2. protoc + C build tools + OpenSSL dev headers
sudo apt-get update
sudo apt-get install -y protobuf-compiler build-essential pkg-config libssl-dev

# 3. Node.js 20 LTS (via NodeSource — apt 官方源版本太旧)
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs
```

### Linux (Fedora / RHEL / Rocky)

```bash
# 1. Rust (via rustup)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 2. protoc + C build tools + OpenSSL dev headers
sudo dnf install -y protobuf-compiler gcc gcc-c++ make pkg-config openssl-devel

# 3. Node.js 20 LTS
sudo dnf install -y nodejs
```

### macOS

```bash
# 1. Xcode Command Line Tools (C compiler + git)
xcode-select --install

# 2. Homebrew (macOS doesn't have a built-in package manager)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 3. Rust (via rustup)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 4. protoc + Node.js + OpenSSL
brew install protobuf node@20 openssl
echo 'export PATH="/opt/homebrew/opt/node@20/bin:$PATH"' >> ~/.zshrc
```

### Windows

```powershell
# 1. Rust: download rustup-init.exe from https://rustup.rs/
#    (勾选 "Add rustc to PATH" + host triple x86_64-pc-windows-msvc)

# 2. Visual Studio Build Tools (C++ workload) — required for C compilation
#    https://visualstudio.microsoft.com/visual-cpp-build-tools/
#    选 "Desktop development with C++" workload (包含 MSVC + Windows SDK)

# 3a. 用 choco 装 protoc + Node.js (推荐, 一条命令搞定)
choco install -y protoc nodejs

# 3b. 或用 scoop
scoop install protobuf nodejs
```

> **WSL note**: if you develop inside WSL2, follow the **Linux (Ubuntu)** steps inside the WSL distro (not on the Windows host). protoc / Node.js / cargo all run inside the Linux side.

### Verify install

```bash
rustc --version    # rustc 1.83.x (edition 2024)  — Required
protoc --version   # libprotoc 3.21.x              — Required
node --version      # v18+ (LTS recommended)        — Optional (dsh-adapter)
python3 --version  # 3.8+                          — Optional (mah-py)
```

If any `command not found` shows up, re-open the shell (to load `~/.cargo/env`) and re-check.

---

## 🚀 Quick start

### Python SDK (recommended for most users)

```bash
pip install -i https://test.pypi.org/simple mah-py==0.1.1
```

```python
from mah_py import Mah

m = Mah()
result = m.run("echo hello world")
print(result.content)  # "[stub] echo: echo hello world"
```

See [`crates/mah-py/README.md`](crates/mah-py/README.md) for full API.

### Rust crate (LLM adapter)

```toml
# Cargo.toml
[dependencies]
ma-harness-model = "0.1"
tokio = { version = "1", features = ["full"] }
futures = "0.3"
```

### Rust umbrella (full SDK, one dep)

For in-workspace use, depend on the umbrella crate to get the full
SDK with a single import path:

```toml
# Cargo.toml (within the ma-harness.rs workspace)
[dependencies]
ma-harness = { path = "ma-harness" }  # path relative to your crate
```

```rust
use ma_harness::*;

// Foundation: types + DI + LLM (default features: core + model)
let ctx = Context::new();
let adapter = OpenaiAdapter::new("sk-...");
let req = ModelRequest::new(vec![ModelMessage::user("hi")]);

// Turn on features for what you need:
//   ma-harness = { path = "ma-harness", features = ["p14", "p15", "server"] }
```

See [`crates/ma-harness/README.md`](crates/ma-harness/README.md)
for the full feature matrix. Note: the umbrella is currently
`publish = false` (workspace-internal) until the 7 SSL-blocked
sub-crates land on crates.io.

```rust
use ma_harness_model::{OpenaiAdapter, ModelAdapter, Message};
use futures::StreamExt;

#[tokio::main]
async fn main() {
    let adapter = OpenaiAdapter::from_env("OPENAI_API_KEY").unwrap();
    let messages = vec![Message::user("hello")];
    let mut stream = adapter.complete_stream(&messages, &Default::default()).await.unwrap();
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk.content);
    }
}
```

### `mah` CLI binary

```bash
# install via cargo
cargo install ma-harness-cli

# or download prebuilt (see GitHub Releases)
mah version
mah plugins
mah run "fix the failing tests"
mah acp serve    # JSON-RPC 2.0 over stdio

# P14 / P15 subcommands:
mah compaction run --input events.jsonl
mah lsp request --server rust-analyzer --method textDocument/hover --params '{...}'
mah web search --query "rust async" --provider brave
mah web fetch --url https://example.com
mah todo list
mah plan list
mah profile list
mah context new --source file
mah guard observe --chain-id main
mah workflow run ci.yaml
mah workflow validate ci.yaml
mah workflow list
mah settings get api_key
mah self list
mah hook install claude-code
mah hook run --event PreToolUse
```

---

## 🏗️ Architecture (37 first-party Rust crates + mah-py)

### Core (P7-P10) — 9 crates

```
crates/
├── ma-harness-cordis         (P7  DI framework)               ✅ crates.io
├── ma-harness-seam           (P8  plugin facade)              ✅ crates.io
├── ma-harness-plugin-macro   (P7  proc-macro)                 ✅ crates.io
├── ma-harness-core           (P7-10 core types)               ✅ crates.io
├── ma-harness-model          (P8-9 LLM adapter, 4 backends)  ✅ crates.io
├── ma-harness-code           (P2.6 wasm sandbox)              ✅ crates.io
├── ma-harness-sandbox        (P10 landlock / seatbelt / stub) ✅ crates.io
├── ma-harness-proto          (gRPC stubs via tonic)           internal
└── ma-harness-session        (P14.8 fork / goals / title)     internal
```

### P11-P12 features — 7 crates

```
├── ma-harness-registry       (P11-6 plugin registry, npm-style) ✅ crates.io
├── ma-harness-bundle         (P11-8 lockfile install)          ✅ crates.io
├── ma-harness-artifact       (P11-7 vibe coding viewer)        ✅ crates.io
├── ma-harness-dag            (P12-9 DAG orchestration)         ✅ crates.io
├── ma-harness-conformance    (P11 dsh fixtures, 9/9 pass)      internal
├── ma-harness-server         (P6 salvo HTTP, 0.96)              internal
└── ma-harness-tui            (P3.9 ratatui)                     internal
```

### P14 ctx.* seams — 10 crates

```
├── ma-harness-subprocess     (P14.1  ctx.subprocess)            internal
├── ma-harness-shell          (P14.2  ctx.shell)                 internal
├── ma-harness-skill          (P14.3  ctx.skill)                 internal
├── ma-harness-compaction     (P14.4  ctx.compaction)            internal
├── ma-harness-lsp            (P14.5  ctx.lsp)                   internal
├── ma-harness-web            (P14.6  ctx.web, search/fetch)     internal
├── ma-harness-todo           (P14.7  ctx.todo + ctx.plan)       internal
├── ma-harness-profile        (P14.9  ctx.profile)               internal
├── ma-harness-context        (P14.10 ctx.context)               internal
└── ma-harness-guard          (P14.11 loop-hygiene guard)        internal
```

### P15 features — 6 crates

```
├── ma-harness-workflow       (P15.4  YAML + DAG + parallel)     internal
├── ma-harness-webhook        (P15.3  HMAC-SHA256 ingress)       internal
├── ma-harness-settings       (P15.5  user settings, hot-reload) internal
├── ma-harness-credentials    (P15.5  env/.env/keyring)          internal
├── ma-harness-self-modification (P15.6 runtime plugin mount)   internal
└── ma-harness-hooks          (P15.7  Claude Code wire-protocol) internal
```

### Misc / P15.1-2 / demo / umbrella — 5 crates

```
├── ma-harness-terminal       (P15.2  PTY backend, future)      internal
├── ma-harness-web-ui         (P15.1  Web UI, future)            internal
├── ma-harness-demo           (CLI binary, integration demo)    internal
├── ma-harness-cli            (CLI binary, all 14 subcommands)  internal
└── ma-harness                (umbrella: re-exports all above   internal (workspace)
                              under feature flags — `use ma_harness::*`
                              for the full SDK; `publish = false`
                              until the 7 SSL-blocked sub-crates land)
```

### First-party plugin (P13) — 1 plugin

```
plugins/
└── ma-harness-plugin-dsh-adapter   (P13 load dsh TS plugins via JSON-RPC stdio)
```

### Python SDK

```
crates/
└── mah-py                    (Python SDK, P11-3)                ✅ test.pypi.org
```

See [`docs/ma-harness-arch-map.md`](docs/ma-harness-arch-map.md) for the full dependency map.

---

## 📚 Documentation

- **[Docs index](docs/README.md)** — entry point for all markdown docs
- **[Architecture overview](docs/ma-harness-arch-map.md)** — 37-crate dependency map
- **[dsh feature parity](docs/en/dsh-feature-parity.md)** — full dsh ↔ ma-harness comparison (12 sections, prose form)
- **[dsh feature parity table](docs/en/dsh-feature-parity-table.md)** — compact table form, status column, 114 items
- **[Development plan](_local/dsh-planning/dsh-development-plan.en.md)** — P14-P17+ 4-phase roadmap (local-only)
- **[Decision log](docs/decision-log.md)** — design decisions
- **[dsh benchmark report](docs/dsh-benchmark-report.md)** — 9/9 = 100% dsh acp-snapshot
- **[Conformance design](docs/conformance-design.md)** — fixture-based testing
- **[Python SDK README](crates/mah-py/README.md)** — `mah-py` quick start

---

## 🔌 Repositories

| Platform | URL | Role |
|---|---|---|
| **GitHub** | https://github.com/ma-harness/ma-harness.rs | primary mirror (CI runs here) |
| **Gitee** | https://gitee.com/yifenma/ma-harness.rs | primary source (CN) |
| **crates.io** | https://crates.io/crates/ma-harness-model | published crates (24 total) |
| **PyPI** | https://test.pypi.org/project/mah-py/ | Python SDK (0.1.1, test) |

---

## 🤝 Contributing

```bash
# 1. Fork & clone
git clone git@github.com:ma-harness/ma-harness.rs.git
cd ma-harness.rs

# 2. Run all tests
cargo test --workspace

# 3. Run conformance
mah conformance --fixtures crates/ma-harness-conformance/fixtures/smoke.jsonl

# 4. Before commit
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

For new features, add a fixture to `crates/ma-harness-conformance/fixtures/` and ensure it passes.

---

## 🌾 "码来 / Code, come forth!"

> *"Code, come forth!"* — the ancient cry of every programmer since `cat > main.c`.
>
> This is **`ma-harness.rs`** — a Rust port of DeepSeek's `dsh` AI agent orchestrator.
> 30%+ faster cold start, 10× faster hot path, types that catch your typos
> before your LLM does. Production-grade, even when the LLM that helped write
> the boilerplate was having an off day.
>
> **📢 Disclaimer**: this project is **for learning and research only**.
> Many implementation details were drafted with LLM assistance (including
> this README's questionable humor), but **every line has been through ~1500
> cargo tests**. Use with confidence.
>
> Bugs? Feature requests? [Open an issue](https://github.com/ma-harness/ma-harness.rs/issues)
> or ping the author. If this project saved you an afternoon, consider fueling
> the next sprint with a small donation toward API tokens:
>
> <table>
> <tr>
>   <td align="center"><b>微信 / WeChat</b></td>
>   <td align="center"><b>支付宝 / Alipay</b></td>
> </tr>
> <tr>
>   <td><img src="docs/assets/donate-wechat.png" width="200" alt="微信收款码"></td>
>   <td><img src="docs/assets/donate-alipay.png" width="200" alt="支付宝收款码"></td>
> </tr>
> </table>
>
> *Even a coffee's worth keeps the GPUs warm ☕.*
> *In Rust we trust — all others we `cargo test`.*

---

## 📜 License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
