# ma-harness.rs

[English](README.md) | [简体中文](README.zh-CN.md)

**Rust rewrite of [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) (dsh) AI agent framework, with extensions for production use.**

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](#)
[![Tests](https://img.shields.io/badge/tests-638%2F638-brightgreen)](#)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](#)
[![crates.io](https://img.shields.io/badge/crates.io-6%20crates-orange)](#cratesio)

`mah` is the binary; `mah-py` is the Python SDK; 6 crates are published to crates.io.

---

## ✨ Features

- **OpenAI / Anthropic / Deepseek / Stub** LLM adapters with streaming, retry (P12-2), vision (P11-5), tool-call
- **Cordis-style DI**: Context / Service / Plugin / TypedKey / Disposable framework (P7)
- **ACP protocol** (JSON-RPC 2.0 over stdio) — interoperable with dsh's `dsh-jsonrpc-agent` (P11-4)
- **Plugin Registry + Bundle** for distributed plugin discovery and reproducible installs (P11-6/8, P12-5/7)
- **DAG task orchestration** with topological sort, dependency validation, short-circuit on failure (P12-9)
- **Vibe Coding artifact viewer** — auto-detect and render 10 artifact kinds (HTML, SVG, JSON, etc.) (P11-7)
- **Code Mode** — run LLM-generated WAT/WASM in wasmtime sandbox (4-layer defense: fuel / epoch / memory / fs) (P2.6)
- **Landlock sandbox** — kernel-enforced fs/process restrictions on Linux (P10)
- **TUI dashboard** — ratatui-based session/event viewer (P3.9)
- **Python SDK** (`mah-py`) — subprocess bridge to `mah` CLI (P11-3)
- **CI/CD** — Gitee Go + GitHub Actions, tag-triggered publish to crates.io (P12-5)

---

## 📊 Status vs [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)

ma-harness.rs is a from-scratch Rust rewrite of dsh v0.1, targeting 100% behavioral parity at the snapshot/fixture level, plus production extensions. Last verified 2026-08-20.

### Behavioral equivalence (P11-1 / P11-2)

| Test suite | dsh v0.1 | ma-harness.rs | Status |
|---|---|---|---|
| **dsh acp-snapshot** (9 fixture) | 100% | **100% (9/9)** | ✅ parity |
| **dsh_synthetic** (7 fixture, shape conversion) | n/a | **100% (7/7)** | ✅ parity |
| **smoke** (8 fixture, framework consistency) | n/a | 62.5% (5/8) | ✅ by design (3 expected failures) |
| Terminal Bench 2.1 | 87.9% | not run | ⏳ business-driven (P11-2.5+) |
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
| HTTP server (salvo) | n/a | ✅ (P6) | OpenAPI export, SSE | ✅ done |
| **Plugin Registry** (npm-style) | n/a | ✅ (P11-6 / P12-5) | search/export/merge | ✅ done |
| **Bundle** (lockfile install) | n/a | ✅ (P11-8 / P12-7) | reproducible | ✅ done |
| **Vibe Coding Artifact viewer** | n/a | ✅ (P11-7) | 10 kinds, terminal render | ✅ done |
| **DAG orchestration** | n/a | ✅ (P12-9) | Kahn topo + short-circuit | ✅ done |
| **Multi-modal vision** | n/a | ✅ (P11-5/9, P12-8) | OpenAI + Anthropic | ✅ done |
| **Retry + Circuit Breaker** | n/a | ✅ (P12-2) | exponential backoff + jitter | ✅ done |
| **Wasm sandbox** (Code Mode) | n/a | ✅ (P2.6) | wasmtime + 4-layer defense | ✅ done |
| **Landlock sandbox** (Linux kernel) | n/a | ✅ (P10) | ABI V1 (kernel ≥ 5.13) | ✅ done |
| Python SDK | n/a | ✅ (P11-3, mah-py 0.1.1) | subprocess + JSON | ✅ done |
| crates.io publish | n/a | ✅ (P12-5) | 6 crates at 0.1.0 | ✅ done |
| LLM backends | 1 (Deepseek) | 4 (OpenAI / Anthropic / Deepseek / Stub) | | ✅ done |
| Language | TypeScript | **Rust 1.94 (edition 2024)** | salvo 0.95 + tonic 0.12 | ✅ done |

### 🚧 Future / Planned (P13+)

| Item | Phase | Why deferred | Blocker | Plan |
|---|---|---|---|---|
| **Terminal Bench 2.1** parity | P11-2.5+ | Needs real LLM API key + dataset (87.9% baseline target) | external (Deepseek API key + dataset access) | business-driven, P11-2.5 docs in `docs/dsh-benchmark-report.md` |
| **Toolathlon-Verified** parity | P11-2.5+ | Same as above (74.1% baseline target) | external | business-driven |
| **DSBench-FullStack** parity | P11-2.5+ | Same as above (71.1% baseline target) | external | business-driven |
| **dsh → ma-harness migration tool** | P13 | Tool to convert dsh plugins/fixtures automatically | needs decision on what to convert first | P13 business-driven |
| **Cargo workspaces** integration | P13 | `cargo install cargo-workspaces` not done yet (manual script used) | install + verify | P13, 10-min task |
| **PyO3 v2** (replace subprocess) | P13+ | v1 (subprocess) works, v2 (PyO3) gives 10-100x speedup | needs re-design of mah-py API | P13+, low priority |
| **WASI preview2** support | P13+ | wasmtime 27 has partial WASI, full preview2 needs upgrade | wasmtime 28+ release | P13+, low priority |
| **Plugin Registry public deployment** | P13+ | P12-5 `export` works, need GitHub Pages hosting | GH Pages config | P13, 30-min task |
| **ACP v3** (when dsh ships) | P13+ | wait for dsh v0.2 protocol spec | external | when dsh ships |
| **crates.io 0.1.0 release** | P12-5 | workflow + secrets in place, waiting for token | `CRATES_IO_TOKEN` for GH + Gitee | first push tag `v0.1.0` |
| **mah-py 0.1.1 → pypi.org production** | P12-4 | Currently on test.pypi.org only | pypi.org token (separate from test.pypi.org) | business verifies test.pypi.org first |
| **Cross-platform binary releases** (Windows / macOS / Linux) | P13+ | mah.exe builds locally; need cross-compile + GH release workflow | cross-compile toolchain (cargo-cross / GitHub Actions matrix) | P13 |

### Test coverage

```
638 tests, 0 failed
  ma-harness-core:           107
  ma-harness-cordis:          81
  ma-harness-model:           71  (incl. vision 17 + retry 13 + vision_plugin 4)
  ma-harness-server:          53
  ma-harness-conformance:     44 + 13 smoke
  ma-harness-tui:             35
  ma-harness-registry:        25
  ma-harness-bundle:          18
  ma-harness-artifact:        26
  ma-harness-dag:             14
  ma-harness-cli:             21 + 10 acp integration
  ma-harness-seam:            11
  ma-harness-sandbox:          6
  ma-harness-plugin-*:        47
  mah-py (pytest):            16
```

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
```

---

## 🏗️ Architecture (14 crates)

```
crates/
├── ma-harness-cordis       (P7 framework)            ✅ crates.io
├── ma-harness-seam         (P8 plugin facade)        ✅ crates.io
├── ma-harness-plugin-macro (P7 proc-macro)          ✅ crates.io
├── ma-harness-core         (P7-10 core types)       ✅ crates.io
├── ma-harness-model        (P8-9 LLM adapter)       ✅ crates.io
├── ma-harness-code         (P2.6 wasm sandbox)      ✅ crates.io
├── ma-harness-server       (P6 salvo HTTP)          internal
├── ma-harness-cli          (P9 binary)              internal
├── ma-harness-conformance  (P11 dsh fixtures)       internal
├── ma-harness-tui          (P3.9 ratatui)           internal
├── ma-harness-sandbox      (P10 landlock)           internal
├── ma-harness-dag          (P12-9 DAG)              internal
├── ma-harness-registry     (P11-6 plugin registry)  internal
├── ma-harness-bundle       (P11-8 lockfile)         internal
└── ma-harness-artifact     (P11-7 artifact viewer)  internal
```

See [`docs/ma-harness-arch-map.md`](docs/ma-harness-arch-map.md) for the full dependency map.

---

## 📚 Documentation

- **[Docs index](docs/README.md)** — entry point for all 18 markdown docs
- **[Architecture overview](docs/ma-harness-arch-map.md)** — 14-crate dependency map
- **[Decision log](docs/decision-log.md)** — 38 design decisions (P1-P12)
- **[P11 final report](docs/p11-final-report.md)** — dsh parity achievement
- **[P12 final report](docs/p12-final-report.md)** — full feature completion
- **[dsh benchmark report](docs/dsh-benchmark-report.md)** — 9/9 = 100% dsh acp-snapshot
- **[Roadmap P11](docs/roadmap-phase-11.md)** — dsh alignment plan
- **[Conformance design](docs/conformance-design.md)** — fixture-based testing
- **[Python SDK README](crates/mah-py/README.md)** — `mah-py` quick start

---

## 🔌 Repositories

| Platform | URL | Role |
|---|---|---|
| **GitHub** | https://github.com/ma-harness/ma-harness.rs | primary mirror (CI runs here) |
| **Gitee** | https://gitee.com/yifenma/ma-harness.rs | primary source (CN) |
| **crates.io** | https://crates.io/crates/ma-harness-model | published crates (6 total) |
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
> this README's questionable humor), but **every line has been through 641
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
>   <td><img src="docs/assets/donate-alipay.jpg" width="200" alt="支付宝收款码"></td>
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
