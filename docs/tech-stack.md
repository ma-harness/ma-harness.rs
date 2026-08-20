# ma-harness.rs — Tech Stack (frozen during PoC)

[English](tech-stack.md) | [简体中文](tech-stack.zh-CN.md)

> Locked: 2026-08-18
> Freeze period: 12-week PoC (i.e. until ~2026-11-10)
> Upgrades require a separate ADR review; bug fixes are exceptions.

---

## 1. Runtime

| Crate          | Version            | Purpose                | Notes                       |
|----------------|--------------------|------------------------|-----------------------------|
| `tokio`        | 1.x (latest 1.40)  | async runtime          | Full async/await stack      |
| `futures`      | 0.3                | async utilities        | stream / sink               |
| `async-trait`  | 0.1                | async fn in trait      | Cooperates with tonic 0.12  |

---

## 2. Communication (Protobuf single protocol)

| Crate              | Version | Purpose                            |
|--------------------|---------|------------------------------------|
| `tonic`            | 0.12    | gRPC server + client               |
| `prost`            | 0.13    | protobuf codec                     |
| `prost-types`      | 0.13    | well-known types (Timestamp/Duration) |
| `tonic-build`      | 0.12    | build script codegen               |
| `tonic-reflection` | 0.12    | gRPC reflection (debug)            |

> **Not used**: `tower` direct composition (use tonic's built-in Layer);
> `async-channel` (use `tokio::sync`); `crossbeam` (use tokio).

---

## 3. HTTP (server side only)

> 2026-08-18: axum 0.7 → salvo 0.79 (charter change, see decision-log §12)

| Crate           | Version | Purpose |
|-----------------|---------|---------|
| `salvo`         | 0.79    | HTTP server (bundles hyper 1, built-in OpenAPI export) |
| ~~`axum`~~      | ❌ no longer used | Replaced by salvo, see decision-log §12 |
| ~~`tower`~~     | ❌ no longer used | salvo has its own middleware |
| ~~`tower-http`~~| ❌ no longer used | trace / cors / compression via salvo middleware |
| ~~`hyper`~~     | (transitive) | Used internally by salvo, not direct dependency |

> **Server side**: salvo accepts HTTP/1.1 + WebSocket (for future browser dashboard)
> **Client side**: reqwest 0.12

> **Note**: As of P12 (Day 101+1), salvo has been upgraded from 0.79 → 0.93 (see
> decision-log §20), and 0.95.2 in the latest run (see decision-log §21).

---

## 4. Serialization

| Crate         | Version | Purpose                                       |
|---------------|---------|-----------------------------------------------|
| `serde`       | 1.x     | Serialization framework                       |
| `serde_json`  | 1.x     | JSON (plugin config / external log format)    |
| `serde_yaml`  | 0.9     | YAML (plugin.toml)                            |
| `schemars`    | 0.8     | JSON Schema generation (for plugin.toml validation) |

---

## 5. Error handling

| Crate         | Version | Purpose                       |
|---------------|---------|-------------------------------|
| `thiserror`   | 1.x     | Library errors (struct form)  |
| `anyhow`      | 1.x     | Application errors (dyn Error) |
| `eyre`        | ❌ not introduced | We pick anyhow, not eyre |

---

## 6. Observability

| Crate                  | Version | Purpose                          |
|------------------------|---------|----------------------------------|
| `tracing`              | 0.1     | structured logging               |
| `tracing-subscriber`   | 0.3     | subscriber + filter              |
| `tracing-futures`      | ❌ not introduced | use `tracing::Instrument` |
| `opentelemetry`        | 0.24    | (Phase 2)                        |
| `prometheus`           | 0.13    | (Phase 2)                        |

During PoC, only `tracing`; metrics go through tracing span; metrics endpoint not
exposed.

> **Note**: As of P10-7, `prometheus` 0.13 is wired in via `ma-harness-server`
> and exposes `/v1/metrics` (text format).

---

## 7. Storage

| Crate         | Version | Purpose                                |
|---------------|---------|----------------------------------------|
| `rusqlite`    | 0.32    | append-only log (sync API, simple)     |
| `r2d2`        | ❌ not introduced | single connection is enough |
| `sled`        | ❌ not introduced | complexity not worth it     |

> **Not used** `sqlx` (async ORM); logs are pure append + range read; rusqlite
> sync API wrapped in `spawn_blocking` is sufficient.
> **Not used** `redb` / `lmdb` / `rocksdb`; SQLite is enough for the PoC.

---

## 8. Sandbox (Linux)

| Crate         | Version | Purpose                                       |
|---------------|---------|-----------------------------------------------|
| `landlock`    | 0.4     | Linux 5.13+ filesystem sandbox                |
| `capsicum`    | ❌ not introduced | FreeBSD only, skip                    |
| `nix`         | 0.29    | POSIX syscall wrapper (used by landlock)      |

> macOS sandbox: Phase 1 use `std::process::Command` + `Command::arg("--sandbox")`
> forwarded to system `sandbox-exec` (placeholder, not deep integration)
> Windows sandbox: Phase 1 **not implemented**, just `#[cfg(windows)]` panic
> with "Phase 2", because the Windows sandbox API differs too much from
> landlock; not worth the distraction during PoC.

> **Note**: As of P10-1.6, `landlock` 0.4 is wired in via `ma-harness-sandbox`
> (cross-platform: Linux landlock / macOS seatbelt / other stub).

---

## 9. CLI

| Crate         | Version | Purpose                                |
|---------------|---------|----------------------------------------|
| `clap`        | 4.x     | command-line parsing (derive feature)  |
| `indicatif`   | 0.17    | progress bar (for end-user benchmark)  |
| `console`     | 0.15    | terminal styling                       |

---

## 10. Testing

| Crate              | Version | Purpose                                              |
|--------------------|---------|------------------------------------------------------|
| `proptest`         | 1.x     | property-based testing                               |
| `mockall`          | 0.13    | mock (cooperates with trait)                         |
| `insta`            | 1.x     | snapshot test (especially for JSONL log format)      |
| `criterion`       | 0.5     | benchmark                                            |
| `tokio-test`       | 0.4     | tokio testing utilities                              |
| `pretty_assertions`| 1.x     | friendly assert output                               |
| `wiremock`         | 0.6     | HTTP mock (for web plugin testing)                   |

---

## 11. Internal locks / concurrency primitives

| Crate         | Version | Purpose                                          |
|---------------|---------|--------------------------------------------------|
| `dashmap`     | 6       | concurrent HashMap (plugin registry)             |
| `parking_lot` | 0.12    | faster than std::sync, no poison                 |
| `arc-swap`    | 1.7     | atomic Arc swap (hot config reload)              |

---

## 12. Not-introduced checklist (avoid temptation)

| Category            | Not introduced                                          | Reason                                  |
|---------------------|---------------------------------------------------------|-----------------------------------------|
| JS engine           | `wasmtime` / `deno_core` / `boa_engine` / `rquickjs`    | Phase 2, see code-mode-deferred.md      |
| Async runtime       | `async-std` / `smol`                                    | tokio unified                           |
| ORM                 | `diesel` / `sea-orm` / `sqlx`                           | log is append-only, rusqlite is enough  |
| Distributed coord.  | `etcd-client` / `consul`                                | single-node benchmark, Phase 2          |
| TLS                 | `rustls` / `openssl`                                    | PoC uses gRPC plaintext, local only     |
| Configuration       | `config` / `figment`                                    | write our own 50 lines, sufficient       |
| Logging macro       | `log`                                                   | use `tracing`                           |
| HTTP client         | `hyper` direct / `ureq`                                 | reqwest unified                         |

---

## 13. Upgrade strategy

- **Patch upgrade** (e.g. 1.40.1 → 1.40.2): direct `cargo update`
- **Minor upgrade** (e.g. 1.40 → 1.41): wait 1 week, run `cargo test` first
- **Major upgrade** (e.g. 0.12 → 0.13): write ADR, run benchmark comparison, only
  then upgrade

## 14. Security advisory source

- RustSec Advisory Database: https://rustsec.org/
- `cargo-audit` integrated into CI (Phase 2)

---

## Changelog

| Date       | Change |
|------------|--------|
| 2026-08-18 | Initial version, frozen for the PoC |
| 2026-08-19 | Note salvo 0.79 → 0.93 / 0.95.2 upgrade (see decision-log §20 / §21) |
| 2026-08-20 | Add P10-1.6 landlock / P10-7 prometheus wiring notes (P10) |
