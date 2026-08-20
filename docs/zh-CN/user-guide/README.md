# ma-harness 使用手册

> **给 ma-harness.rs 终端用户** — 应用程序开发者、集成商、平台工程师。
>
> 找 **设计文档**、**周报** 或 **决策历史**?
> 看 [docs/zh-CN/README.md](../README.md) 总索引。

[English](README.md) | [简体中文](README.md)

## 这里有什么

本手册**一步步**教你在真实场景下使用 ma-harness。每页独立,你可以顺序读,
或者跳到跟当前需求匹配的页面。

| # | 页面 | 何时读 |
|---|---|---|
| 01 | [安装](01-installation.md) | 首次安装 — 装 `mah` CLI + Python SDK + 验版本 |
| 02 | [快速开始](02-quick-start.md) | 前 5 分钟 — 本地跑第一个 agent,理解循环 |
| 03 | [服务部署](03-server.md) | 上生产 — `mah start` 配 gRPC + HTTP,反代 + HTTPS |
| 04 | [插件](04-plugins.md) | 扩展 — 装 first-party 插件,自己写,publish 到 GH Pages registry |
| 05 | [Conformance](05-conformance.md) | 验证 — `mah conformance` 跑你的 fixture,跟 dsh 对比 |
| 06 | [Troubleshooting](06-troubleshooting.md) | 出问题时 — 常见错误,debug 步骤,FAQ |

## 30 秒概览

**ma-harness.rs** 是 [DeepSeek 的 dsh](https://github.com/deepseek-ai/dsh) AI agent
orchestrator 的 Rust 重写。你写插件、配 typed key,`mah` CLI 把它们跑在
SessionEvent log 里,可选用真 LLM 流回 (OpenAI / Anthropic / stub)。

```bash
# 1. 装
cargo install --path crates/ma-harness-cli

# 2. 跑一个 agent
mah run "总结 README"

# 3. 或者起 server
mah start --grpc-port 50051 --http-port 50050
```

## 阅读顺序

- **新用 ma-harness?** 读 `01-installation` → `02-quick-start` → `04-plugins`
- **部署到生产?** 读 `01-installation` → `03-server`
- **从 dsh 迁移?** 读 `02-quick-start` → `05-conformance`
- **写插件?** 读 `04-plugins`
- **出问题了?** 读 `06-troubleshooting`

## 怎么用本手册

每页结构一样:

1. **要做什么** — 一句话目标
2. **前置条件** — 需要的准备
3. **步骤** — 精确命令 + 解释
4. **验证** — 怎么确认成功
5. **Troubleshooting** — 该步骤常见错误
6. **下一步** — 相关页链接

## 约定

- 所有命令假设 Unix-like shell (bash / zsh)。Windows PowerShell 不同地方会注明。
- 路径用 POSIX 格式 (`./foo/bar`)。Windows 换成反斜杠。
- 标 `bash` 的代码块可以直接 copy-paste。标 `text` 的代码块显示期望输出。
- 环境变量用 `UPPER_SNAKE_CASE`。
- 方括号里是占位符: 把 `[your-token]` 换成你的实际值,包括方括号。

## 下一步去哪

- **内部架构跟设计理念**: [docs/zh-CN/ma-harness-arch-map.md](../ma-harness-arch-map.md)
- **插件 schema 参考**: [docs/zh-CN/plugin-schema-v1.md](../plugin-schema-v1.md)
- **决策历史** (为什么这样设计): [docs/zh-CN/decision-log.md](../decision-log.md)
- **开发计划** (已完成 / 待办): [docs/zh-CN/development-plan.md](../development-plan.md)
