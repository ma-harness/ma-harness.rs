# ma-harness.rs

> **Rust 重写的 AI agent 编排 harness**,设计参考 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)。
> 12 周 PoC 进度 92% (Week 1-11 完成, Week 12 收尾)。

---

## 这是什么

`ma-harness.rs` 是一个**独立的 Rust 实现**,跟 dsh 行为对齐、跑分对比,但**不是 dsh 的官方 Rust 端口**。

主要设计:

| 维度 | 选择 |
|---|---|
| 元框架 | Cordis-rs (typed key + plugin + listener + scope + fork) |
| 协议 | Protobuf 单协议 (Prost + tonic) |
| 日志 | append-only SessionEvent (rusqlite, model-visible 不变量) |
| 公开 API | `ma_harness_seam` (5 trait + 5 proc-macro re-export) |
| 6 first-party plugin | bash / fs / web / subagent / skill / cordis |
| 内部 crate | `ma_harness_cordis` / `ma_harness_core` (API 频繁变) |

## 快速开始

```bash
# 编译 (网络通后)
cargo build --workspace

# 跑全部测试 (~167 个)
cargo test --workspace

# 跑全部 bench (~18 个)
cargo bench --workspace

# 装 `mah` CLI
cargo install --path crates/ma_harness_cli
mah --help
```

## 仓库结构

```
ma-harness.rs/
├── AGENTS.md                       ← AI agent / 新成员入口
├── README.md                       ← 你正在看
├── CHANGELOG.md                    ← 变更记录
├── LICENSE-MIT                     ← MIT License
├── LICENSE-APACHE                  ← Apache 2.0 License
├── Cargo.toml                      ← workspace 根 (16 member)
├── .gitattributes                  ← 跨平台 LF 规范化
├── .github/workflows/ci.yml        ← GitHub Actions CI
├── .gitee/workflows/ci.yml         ← Gitee Go CI
├── docs/                           ← 决策档案 + 设计稿 + 周报
│   ├── decision-log.md             ← 11 项关键决策 (宪法层)
│   ├── ma-harness-arch-map.md      ← 跟 dsh 的 12 节机制映射
│   ├── macro-design.md             ← 5 个 proc-macro 规范
│   ├── plugin-schema-v1.md         ← plugin.toml + JSON Schema
│   ├── tech-stack.md               ← 14 节 crate 冻结 + "不引入"清单
│   ├── code-mode-deferred.md       ← Code Mode Phase 2 推迟
│   ├── conformance-design.md       ← Week 10 conformance 框架设计
│   ├── benchmark-design.md         ← Week 10 benchmark 设计
│   ├── conformance-report-week11.md ← Week 11 conformance 报告 (TBD)
│   ├── benchmark-report-week11.md  ← Week 11 benchmark 报告 (TBD)
│   └── weekly/                     ← 周报 (000-007)
├── proto/ma_harness/v1/            ← 3 个 .proto (agent / session / event)
├── crates/                         ← 8 个内部 + 公开 crate
│   ├── ma_harness_cordis/          ← 内部元框架 (7 文件, ~2700 行)
│   ├── ma_harness_core/            ← 内部核心 (4 文件, ~1500 行)
│   ├── ma_harness_seam/            ← 公开抽象层 (5 trait + PluginRegistry)
│   ├── ma_harness_proto/           ← Prost/tonic codegen
│   ├── ma_harness_server/          ← gRPC service 实现 + axum /health
│   ├── ma_harness_cli/             ← `mah` 二进制 (7 子命令)
│   ├── ma_harness_plugin_macro/    ← 5 proc-macro + ctx_key!
│   ├── ma_harness_demo/            ← 端到端 12 步 demo
│   └── ma_harness_conformance/     ← Conformance test framework (Week 10)
└── plugins/                        ← 6 first-party 插件
    ├── ma_harness_plugin_hello/    ← (Week 1 hello-world 教学用)
    ├── ma_harness_plugin_bash/     ← subprocess + timeout
    ├── ma_harness_plugin_fs/       ← read/write/list + 路径白名单
    ├── ma_harness_plugin_web/      ← reqwest + URL 白名单 + timeout
    ├── ma_harness_plugin_subagent/ ← fork ctx 跑子 agent
    ├── ma_harness_plugin_skill/    ← load .skill/ 目录
    └── ma_harness_plugin_cordis/   ← ctx 反射
```

## `mah` CLI

7 个子命令:

```bash
mah start                   # 起 server (tonic gRPC 50051 + axum HTTP 50050)
mah run "echo hi"           # 本地跑一次 agent (StubModel)
mah plugins                 # 列已装载 plugin
mah events <session_id>     # 查 session 事件
mah conformance \           # 跑 conformance fixture, 出报告
  --fixtures fixtures/smoke.jsonl \
  --output target/
mah conformance \           # dsh 风格 fixture
  --fixtures fixtures/dsh/ --dsh \
  --output target/
mah bench                   # benchmark 提示 (真跑用 cargo bench)
mah version
```

## 文档导航

按"我需要了解什么"分:

| 我需要... | 看这个 |
|---|---|
| 仓库定位 + 决策 | [`AGENTS.md`](./AGENTS.md) + [`docs/decision-log.md`](./docs/decision-log.md) |
| 跟 dsh 怎么对应 | [`docs/ma-harness-arch-map.md`](./docs/ma-harness-arch-map.md) |
| 写 proc-macro | [`docs/macro-design.md`](./docs/macro-design.md) |
| 写 plugin | [`docs/plugin-schema-v1.md`](./docs/plugin-schema-v1.md) |
| 跑 conformance | [`docs/conformance-design.md`](./docs/conformance-design.md) |
| 跑 benchmark | [`docs/benchmark-design.md`](./docs/benchmark-design.md) |
| 跟踪进度 | [`docs/weekly/`](./docs/weekly/) (000-007) |
| 加新 crate | [`docs/tech-stack.md`](./docs/tech-stack.md) § "不引入"清单 |
| 用 ma-harness API | [`crates/ma_harness_seam/src/lib.rs`](./crates/ma_harness_seam/src/lib.rs) |
| 变更记录 | [`CHANGELOG.md`](./CHANGELOG.md) |
| 许可证 | [`LICENSE-MIT`](./LICENSE-MIT) + [`LICENSE-APACHE`](./LICENSE-APACHE) |

## 关键数字 (12 周 PoC 终点)

| 指标 | 数值 |
|---|---|
| 累计 commit | 41+ (持续增长) |
| Workspace member | 16 (9 crates/ + 7 plugins/) |
| 累计代码 | ~14000 行 |
| 累计测试 | ~167 (mental-verified, 网络通后跑) |
| 累计 bench | 18 (cordis 10 + core 4 + seam 4) |
| 设计文档 | 9 份 |
| 周报 | 7 份 (Day 0 / Week 1-2 / 3-4 / 5-6 / 7-9 / 10 / 11) |

## Phase 2 路线图

不在 12 周 PoC scope, Week 12 收尾后启动:

- [ ] macro 增强 (#[dsh_service(cordis, seam)] 自动派生两套)
- [ ] Sandbox 强化 (landlock / Seatbelt syscall)
- [ ] 持久化 (SessionServiceImpl 内存换 rusqlite)
- [ ] Code Mode (wasmtime / deno_core)
- [ ] 多 model adapter (OpenAI / Anthropic)
- [ ] 真 plugin 动态装载 (conformance runner 现在用 placeholder ctx)
- [ ] HTTP/HTTPS inbound (除 tonic gRPC)
- [ ] 持久化 session + 重启恢复

## 网络阻塞

本机代理 `127.0.0.1:7890` 不能代理 HTTPS, **130+ 文件未 `cargo check` 验证**。
所有代码 mental-compile only,等代理恢复或换网络环境后跑:

```bash
cargo check --workspace
cargo test --workspace
cargo bench --workspace
```

预计 16 crate 编译 + 167 测试 + 18 bench 跑通需要 2-3 分钟。

## License

MIT OR Apache-2.0 (跟 workspace 锁定一致)

- [`LICENSE-MIT`](./LICENSE-MIT) — MIT License
- [`LICENSE-APACHE`](./LICENSE-APACHE) — Apache License 2.0

## 仓库地址

`git@gitee.com:yifenma/ma-harness.rs.git`
