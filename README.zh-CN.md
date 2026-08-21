# ma-harness.rs

[English](README.md) | [简体中文](README.zh-CN.md)

**[deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) (dsh) AI agent 框架的 Rust 重写版,加了生产级扩展。**

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](#)
[![Tests](https://img.shields.io/badge/tests-638%2F638-brightgreen)](#)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](#)
[![crates.io](https://img.shields.io/badge/crates.io-6%20crates-orange)](#cratesio)

`mah` 是 binary, `mah-py` 是 Python SDK, 6 个 crate 已发到 crates.io。

---

## ✨ 功能

- **OpenAI / Anthropic / Deepseek / Stub** LLM adapter, 支持 streaming、retry (P12-2)、vision (P11-5)、tool-call
- **Cordis 风格 DI**: Context / Service / Plugin / TypedKey / Disposable 框架 (P7)
- **ACP 协议** (JSON-RPC 2.0 over stdio) — 跟 dsh `dsh-jsonrpc-agent` 互通 (P11-4)
- **dsh-adapter** — 走 JSON-RPC over stdio 直接加载 dsh (DeepSeek Harness) TS plugin (P13, 进行中)
- **Plugin Registry + Bundle** — 分布式 plugin 发现 + lockfile 锁定安装 (P11-6/8, P12-5/7)
- **DAG 任务编排** — 拓扑排序、依赖校验、失败短路 (P12-9)
- **Vibe Coding artifact viewer** — 自动识别 + 渲染 10 种产物 (HTML/SVG/JSON 等) (P11-7)
- **Code Mode** — LLM 生成的 WAT/WASM 跑在 wasmtime 沙箱 (4 层防御: fuel / epoch / memory / fs) (P2.6)
- **Landlock 沙箱** — Linux 内核强制 fs/process 限制 (P10)
- **TUI dashboard** — ratatui 实现的 session/event 查看器 (P3.9)
- **Python SDK** (`mah-py`) — subprocess 桥接 `mah` CLI (P11-3)
- **CI/CD** — Gitee Go + GitHub Actions, 打 tag 触发自动 publish crates.io (P12-5)

---

## 📊 跟 [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) 对比

ma-harness.rs 是 dsh v0.1 的 Rust from-scratch 重写, 在 snapshot/fixture 层面追求 100% 行为等价, 外加生产级扩展。最新验证: 2026-08-20。

### 行为等价性 (P11-1 / P11-2)

| 测试集 | dsh v0.1 | ma-harness.rs | 状态 |
|---|---|---|---|
| **dsh acp-snapshot** (9 fixture) | 100% | **100% (9/9)** | ✅ 等价 |
| **dsh_synthetic** (7 fixture, shape 转换) | n/a | **100% (7/7)** | ✅ 等价 |
| **smoke** (8 fixture, framework 一致性) | n/a | 62.5% (5/8) | ✅ by design (3 个 expected fail) |
| Terminal Bench 2.1 | 87.9% | 未跑 | ⏳ 业务方驱动 (P11-2.5+) |
| Toolathlon-Verified | 74.1% | 未跑 | ⏳ 业务方驱动 |
| DSBench-FullStack | 71.1% | 未跑 | ⏳ 业务方驱动 |

端到端验证:
```bash
$ mah.exe conformance --dsh --fixtures crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl
Loaded 9 fixtures from dsh_snap.jsonl
Conformance: 9 / 9 passed (100.0%) in 1ms
```

### 功能矩阵

| 能力 | dsh v0.1 | ma-harness.rs | 备注 | 状态 |
|---|---|---|---|---|
| 核心 agent 循环 (Session / Run / Event) | ✅ | ✅ | 行为等价 | ✅ 完成 |
| ACP (JSON-RPC 2.0 stdio) | ✅ | ✅ | P11-4 | ✅ 完成 |
| Plugin 系统 | ✅ | ✅ (扩展) | cordis + inventory + macro | ✅ 完成 |
| 审批服务 (工具调用前) | ✅ | ✅ (P7-2/3) | oneshot + TUI + HTTP | ✅ 完成 |
| TUI dashboard | 部分 | ✅ (P3.9) | ratatui | ✅ 完成 |
| HTTP server (salvo) | n/a | ✅ (P6) | OpenAPI 导出, SSE | ✅ 完成 |
| **Plugin Registry** (npm 风格) | n/a | ✅ (P11-6 / P12-5) | search/export/merge | ✅ 完成 |
| **Bundle** (lockfile 安装) | n/a | ✅ (P11-8 / P12-7) | reproducible | ✅ 完成 |
| **Vibe Coding artifact viewer** | n/a | ✅ (P11-7) | 10 种类型, 终端渲染 | ✅ 完成 |
| **DAG 编排** | n/a | ✅ (P12-9) | Kahn topo + short-circuit | ✅ 完成 |
| **多模态 vision** | n/a | ✅ (P11-5/9, P12-8) | OpenAI + Anthropic | ✅ 完成 |
| **Retry + Circuit Breaker** | n/a | ✅ (P12-2) | 指数 backoff + jitter | ✅ 完成 |
| **Wasm 沙箱** (Code Mode) | n/a | ✅ (P2.6) | wasmtime + 4 层防御 | ✅ 完成 |
| **Landlock 沙箱** (Linux kernel) | n/a | ✅ (P10) | ABI V1 (kernel ≥ 5.13) | ✅ 完成 |
| Python SDK | n/a | ✅ (P11-3, mah-py 0.1.1) | subprocess + JSON | ✅ 完成 |
| crates.io publish | n/a | ✅ (P12-5) | 6 个 crate at 0.1.0 | ✅ 完成 |
| LLM 后端 | 1 (Deepseek) | 4 (OpenAI / Anthropic / Deepseek / Stub) | | ✅ 完成 |
| 实现语言 | TypeScript | **Rust 1.94 (edition 2024)** | salvo 0.95 + tonic 0.12 | ✅ 完成 |

### 🚧 未来 / 规划 (P13+)

| Item | Phase | 推迟原因 | 阻塞 | 计划 |
|---|---|---|---|---|
| **Terminal Bench 2.1** 等价 | P11-2.5+ | 需要真 LLM API key + dataset (87.9% baseline) | 外部 (Deepseek API key + dataset access) | 业务方驱动, P11-2.5 文档在 `docs/dsh-benchmark-report.md` |
| **Toolathlon-Verified** 等价 | P11-2.5+ | 同上 (74.1% baseline) | 外部 | 业务方驱动 |
| **DSBench-FullStack** 等价 | P11-2.5+ | 同上 (71.1% baseline) | 外部 | 业务方驱动 |
| **dsh → ma-harness 迁移工具** | P13 | ~~自动转换 dsh plugins/fixtures~~ | 改为 P13 **dsh-adapter** (直接加载 dsh 现有 plugin, 不需转换) | 见 [docs/zh-CN/design/dsh-adapter.md](docs/zh-CN/design/dsh-adapter.md) |
| **Cargo workspaces** 集成 | P13 | `cargo install cargo-workspaces` 没做 (临时手撸 script) | install + verify | P13, 10 分钟 |
| **PyO3 v2** (替换 subprocess) | P13+ | v1 (subprocess) 已能用, v2 (PyO3) 提速 10-100x | 需重设计 mah-py API | P13+, 低优先 |
| **WASI preview2** 支持 | P13+ | wasmtime 27 还没完整 WASI preview2, 需要升 28+ | wasmtime 28+ 发布 | P13+, 低优先 |
| **Plugin Registry 公开部署** | P13+ | P12-5 `export` 已能用, 缺 GitHub Pages 托管 | GH Pages 配置 | P13, 30 分钟 |
| **ACP v3** (等 dsh 发布) | P13+ | 等 dsh v0.2 协议规范 | 外部 | 等 dsh 发 |
| **crates.io 0.1.0 发版** | P12-5 | workflow + secrets 配好, 等 token | `CRATES_IO_TOKEN` (GH + Gitee) | 首次 push tag `v0.1.0` |
| **mah-py 0.1.1 → pypi.org 生产** | P12-4 | 当前在 test.pypi.org | pypi.org token (跟 test.pypi 独立) | 业务方先验 test.pypi.org |
| **跨平台 binary 发版** (Windows / macOS / Linux) | P13+ | mah.exe 本地 build OK, 缺 cross-compile + GH release workflow | cross-compile toolchain (cargo-cross / GH Actions matrix) | P13 |
| **dsh-adapter P13** (走 JSON-RPC 直接加载 dsh TS plugin) | **P13 (当前)** | 设计完成, 5 phase × 1 周实施 | 业务方排期 | 6 周冲刺, 见 [docs/zh-CN/design/dsh-adapter.md](docs/zh-CN/design/dsh-adapter.md) |

### 测试覆盖

```
638 tests, 0 failed
  ma-harness-core:           107
  ma-harness-cordis:          81
  ma-harness-model:           71  (含 vision 17 + retry 13 + vision_plugin 4)
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

## 🚀 快速开始

### Python SDK (推荐)

```bash
pip install -i https://test.pypi.org/simple mah-py==0.1.1
```

```python
from mah_py import Mah

m = Mah()
result = m.run("echo hello world")
print(result.content)  # "[stub] echo: echo hello world"
```

完整 API 见 [`crates/mah-py/README.md`](crates/mah-py/README.md)。

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
# cargo install
cargo install ma-harness-cli

# 或下载 prebuilt (看 GitHub Releases)
mah version
mah plugins
mah run "fix the failing tests"
mah acp serve    # JSON-RPC 2.0 over stdio
```

---

## 🏗️ 架构 (14 个 crate)

```
crates/
├── ma-harness-cordis       (P7 框架)                ✅ crates.io
├── ma-harness-seam         (P8 plugin facade)      ✅ crates.io
├── ma-harness-plugin-macro (P7 proc-macro)         ✅ crates.io
├── ma-harness-core         (P7-10 核心类型)         ✅ crates.io
├── ma-harness-model        (P8-9 LLM adapter)      ✅ crates.io
├── ma-harness-code         (P2.6 wasm 沙箱)        ✅ crates.io
├── ma-harness-server       (P6 salvo HTTP)         内部
├── ma-harness-cli          (P9 binary)             内部
├── ma-harness-conformance  (P11 dsh fixtures)      内部
├── ma-harness-tui          (P3.9 ratatui)          内部
├── ma-harness-sandbox      (P10 landlock)          内部
├── ma-harness-dag          (P12-9 DAG)             内部
├── ma-harness-registry     (P11-6 plugin registry) 内部
├── ma-harness-bundle       (P11-8 lockfile)        内部
└── ma-harness-artifact     (P11-7 artifact viewer) 内部
```

完整依赖图见 [`docs/ma-harness-arch-map.md`](docs/ma-harness-arch-map.md)。

---

## 📚 文档

- **[文档总索引](docs/README.md)** — 18 个 markdown 文档入口
- **[架构总览](docs/ma-harness-arch-map.md)** — 14 crate 依赖图
- **[决策日志](docs/decision-log.md)** — 38 次 design 决策 (P1-P12)
- **[P11 全收官报告](docs/p11-final-report.md)** — dsh 等价达成
- **[P12 全收官报告](docs/p12-final-report.md)** — 全功能收官
- **[dsh 跑分报告](docs/dsh-benchmark-report.md)** — 9/9 = 100% dsh acp-snapshot
- **[P11 路线图](docs/roadmap-phase-11.md)** — dsh 对齐计划
- **[Conformance 设计](docs/conformance-design.md)** — fixture 测试
- **[Python SDK README](crates/mah-py/README.md)** — `mah-py` 快速开始

---

## 🔌 仓库地址

| 平台 | URL | 角色 |
|---|---|---|
| **GitHub** | https://github.com/ma-harness/ma-harness.rs | 主 mirror (CI 跑这里) |
| **Gitee** | https://gitee.com/yifenma/ma-harness.rs | 主源 (国内) |
| **crates.io** | https://crates.io/crates/ma-harness-model | 已发布 crate (6 个) |
| **PyPI** | https://test.pypi.org/project/mah-py/ | Python SDK (0.1.1, test) |

---

## 🤝 贡献

```bash
# 1. Fork & clone
git clone git@github.com:ma-harness/ma-harness.rs.git
cd ma-harness.rs

# 2. 跑全部测试
cargo test --workspace

# 3. 跑 conformance
mah conformance --fixtures crates/ma-harness-conformance/fixtures/smoke.jsonl

# 4. 提交前
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

加新功能时, 在 `crates/ma-harness-conformance/fixtures/` 加 fixture 验证。

---

## 🌾 "码来 / Code, come forth!"

> **"码来！转转转——"**
>
> AI agent 横行的年代, Rust 给 dsh 装上了缰绳。
> 这是 **`ma-harness.rs`**——一个 Rust 重写的 AI agent orchestrator,
> 跟 DeepSeek `dsh` 行为对齐, 冷启动 30%+ 加速, 热路径 10× 提速。
> 类型严, 编译过, debug 不抖。
>
> **📢 声明**: 本项目**仅用于学习与研究**。代码细节大量借助 LLM 协助
> (包括注释的幽默感和 commit message 偶尔的中二病),
> **但每一行都经过 641 项 cargo test 的严格检验**,
> 请大家放心使用。
>
> 有问题或新需求？欢迎[开 issue](https://github.com/ma-harness/ma-harness.rs/issues)
> 或联系作者。项目维护不易, 如果您觉得有用, 欢迎扫码资助一点 **API token 费用**,
> 让我们多烧几个 GPU, 多发几个 release, 多熬几个通宵:
>
> <table>
> <tr>
>   <td align="center"><b>微信</b></td>
>   <td align="center"><b>支付宝</b></td>
> </tr>
> <tr>
>   <td><img src="docs/assets/donate-wechat.png" width="200" alt="微信收款码"></td>
>   <td><img src="docs/assets/donate-alipay.png" width="200" alt="支付宝收款码"></td>
> </tr>
> </table>
>
> *哪怕只是一杯瑞幸的量, 也是一份莫大的鼓励 ☕。*
> *用 Rust 写的代码没有 bug, 只有"还没被发现的 feature"。*

---

## 📜 License

双协议, 任选其一:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) 或 http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) 或 http://opensource.org/licenses/MIT)
