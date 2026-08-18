# ma-harness.rs — Tech Stack (PoC 期间冻结)

> 锁定时间: 2026-08-18
> 冻结期: 12 周 PoC (即 ~2026-11-10 之前)
> 升级走 ADR 单独评审,bug fix 例外

---

## 1. 运行时

| Crate | 版本 | 用途 | 备注 |
|---|---|---|---|
| `tokio` | 1.x (latest 1.40) | async runtime | 全栈 async/await |
| `futures` | 0.3 | 异步工具 | stream / sink |
| `async-trait` | 0.1 | async fn in trait | 跟 tonic 0.12 配合 |

---

## 2. 通信 (Protobuf 单协议)

| Crate | 版本 | 用途 |
|---|---|---|
| `tonic` | 0.12 | gRPC server + client |
| `prost` | 0.13 | protobuf codec |
| `prost-types` | 0.13 | well-known types (Timestamp/Duration) |
| `tonic-build` | 0.12 | build script codegen |
| `tonic-reflection` | 0.12 | gRPC reflection (debug 用) |

> **不用**: `tower` 直接组合 (用 tonic 自带的 Layer);`async-channel` (用 tokio::sync);`crossbeam` (用 tokio)。

---

## 3. HTTP (仅 server side)

| Crate | 版本 | 用途 |
|---|---|---|
| `axum` | 0.7 | HTTP server (跟 tonic 共享 tokio) |
| `tower` | 0.5 | middleware |
| `tower-http` | 0.6 | trace / cors / compression |
| `hyper` | 1.x | (axum 传递依赖,不直接用) |

> **server 端**: axum 接 HTTP/1.1 + WebSocket (供将来 browser dashboard 用)
> **client 端**: reqwest 0.12

---

## 4. 序列化

| Crate | 版本 | 用途 |
|---|---|---|
| `serde` | 1.x | 序列化框架 |
| `serde_json` | 1.x | JSON (插件配置 / 日志外部格式) |
| `serde_yaml` | 0.9 | YAML (plugin.toml) |
| `schemars` | 0.8 | JSON Schema 生成 (给 plugin.toml 校验) |

---

## 5. 错误处理

| Crate | 版本 | 用途 |
|---|---|---|
| `thiserror` | 1.x | 库错误 (struct 化) |
| `anyhow` | 1.x | 应用错误 (dyn Error) |
| `eyre` | ❌ 不引入 | 跟 anyhow 二选一,我们选 anyhow |

---

## 6. 可观测

| Crate | 版本 | 用途 |
|---|---|---|
| `tracing` | 0.1 | 结构化日志 |
| `tracing-subscriber` | 0.3 | subscriber + filter |
| `tracing-futures` | ❌ 不引入 | 用 `tracing::Instrument` |
| `opentelemetry` | 0.24 | (Phase 2) |
| `prometheus` | 0.13 | (Phase 2) |

PoC 期间只用 tracing,指标走 tracing span,不开 metrics endpoint。

---

## 7. 存储

| Crate | 版本 | 用途 |
|---|---|---|
| `rusqlite` | 0.32 | append-only 日志 (sync API,简单) |
| `r2d2` | ❌ 不引入 | 单连接足够 |
| `sled` | ❌ 不引入 | 复杂度不值 |

> **不用** `sqlx` (异步 ORM),日志是纯 append + 范围读,rusqlite 同步 API 包在 `spawn_blocking` 就够。
> **不用** `redb` / `lmdb` / `rocksdb`,PoC 阶段 SQLite 足够。

---

## 8. Sandbox (Linux)

| Crate | 版本 | 用途 |
|---|---|---|
| `landlock` | 0.4 | Linux 5.13+ filesystem 沙箱 |
| `capsicum` | ❌ 不引入 | FreeBSD 专属,跳过 |
| `nix` | 0.29 | POSIX syscall 封装 (landlock 用) |

> macOS sandbox: Phase 1 用 `std::process::Command` + `Command::arg("--sandbox")` 转发给系统 `sandbox-exec`(占位,不深度集成)
> Windows sandbox: Phase 1 **不做**,直接 `#[cfg(windows)]` panic with "Phase 2",因为 Windows sandbox API 跟 landlock 差异太大,PoC 期间不分心。

---

## 9. CLI

| Crate | 版本 | 用途 |
|---|---|---|
| `clap` | 4.x | 命令行解析 (derive feature) |
| `indicatif` | 0.17 | 进度条 (用户跑 benchmark 用) |
| `console` | 0.15 | 终端样式 |

---

## 10. 测试

| Crate | 版本 | 用途 |
|---|---|---|
| `proptest` | 1.x | property-based testing |
| `mockall` | 0.13 | mock (跟 trait 配合) |
| `insta` | 1.x | snapshot test (尤其 JSONL 日志格式) |
| `criterion` | 0.5 | benchmark |
| `tokio-test` | 0.4 | tokio 测试工具 |
| `pretty_assertions` | 1.x | assert 输出友好 |
| `wiremock` | 0.6 | HTTP mock (web 插件测试) |

---

## 11. 内部锁 / 并发原语

| Crate | 版本 | 用途 |
|---|---|---|
| `dashmap` | 6 | 并发 HashMap (plugin registry) |
| `parking_lot` | 0.12 | 比 std::sync 快 / 不 poison |
| `arc-swap` | 1.7 | atomic Arc swap (config 热更新) |

---

## 12. 不引入的清单 (避免诱惑)

| 类别 | 不引入 | 理由 |
|---|---|---|
| JS 引擎 | `wasmtime` / `deno_core` / `boa_engine` / `rquickjs` | Phase 2,见 code-mode-deferred.md |
| 异步运行时 | `async-std` / `smol` | tokio 统一 |
| ORM | `diesel` / `sea-orm` / `sqlx` | 日志是 append-only,rusqlite 够 |
| 分布式协调 | `etcd-client` / `consul` | 单机跑分,Phase 2 |
| TLS | `rustls` / `openssl` | PoC 跑 gRPC plaintext,本地只 |
| 配置 | `config` / `figment` | 自己写一个 50 行的,够用 |
| 日志宏 | `log` | 用 `tracing` |
| HTTP 客户端 | `hyper` 直接用 / `ureq` | reqwest 统一 |

---

## 13. 升级策略

- **patch 升级** (例 1.40.1 → 1.40.2): 直接 `cargo update`
- **minor 升级** (例 1.40 → 1.41): 等 1 周,跑过 `cargo test` 后升级
- **major 升级** (例 0.12 → 0.13): 写 ADR,跑 benchmark 对比,决策后才升

## 14. 安全公告源

- RustSec Advisory Database: https://rustsec.org/
- `cargo-audit` 接入 CI(Phase 2)

---

## 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-18 | 初版,PoC 期间冻结 |
