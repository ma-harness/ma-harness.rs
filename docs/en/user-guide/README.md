# ma-harness User Guide

> **For end users of ma-harness.rs** — application developers, integrators,
> and platform engineers who run agents in production.
>
> Looking for **design docs**, **weekly reports**, or **decision history**?
> See [docs/en/README.md](../README.md) for the full index.

[English](README.md) | [简体中文](../../zh-CN/user-guide/README.md)

## What's here

This guide walks you through running ma-harness in real situations, one
step at a time. Each page is self-contained; you can read them in order,
or jump to the page that matches your immediate need.

| # | Page | When to read |
|---|---|---|
| 01 | [Installation](01-installation.md) | First time setup — install `mah` CLI, get Python SDK, verify versions |
| 02 | [Quick start](02-quick-start.md) | First 5 minutes — run your first agent locally, understand the loop |
| 03 | [Server deployment](03-server.md) | Going to production — `mah start` with gRPC + HTTP, reverse proxy, HTTPS, auth |
| 04 | [Plugins](04-plugins.md) | Extending — install first-party plugins, write your own, publish to GH Pages registry |
| 05 | [Conformance](05-conformance.md) | Validating — `mah conformance` against your fixtures, comparing to dsh |
| 06 | [Troubleshooting](06-troubleshooting.md) | When things break — common errors, debugging steps, FAQ |

## 30-second overview

**ma-harness.rs** is a Rust port of [DeepSeek's dsh](https://github.com/deepseek-ai/dsh)
AI agent orchestrator. You write plugins, configure typed keys, and the
`mah` CLI runs them through a SessionEvent log, optionally streaming back
to a real LLM (OpenAI, Anthropic, or stub).

```bash
# 1. Install
cargo install --path crates/ma-harness-cli

# 2. Run a one-shot agent
mah run "summarize the README"

# 3. Or run a server (gRPC + HTTP)
mah start --grpc-port 50051 --http-port 50050
```

## Reading order

- **New to ma-harness?** Read `01-installation` → `02-quick-start` → `04-plugins`
- **Deploying to production?** Read `01-installation` → `03-server`
- **Migrating from dsh?** Read `02-quick-start` → `05-conformance`
- **Writing plugins?** Read `04-plugins`
- **Trouble?** Read `06-troubleshooting`

## How to use this guide

Each page follows the same structure:

1. **What you'll do** — one-sentence goal
2. **Prerequisites** — what you need before starting
3. **Step-by-step** — exact commands, with explanations
4. **Verify** — how to confirm it worked
5. **Troubleshooting** — common errors specific to that step
6. **Next** — links to related pages

## Conventions

- All commands assume a Unix-like shell (bash / zsh). Windows PowerShell
  equivalents are noted where they differ.
- Paths use POSIX form (`./foo/bar`). On Windows, swap forward slashes
  for backslashes.
- Code blocks with `bash` can be copy-pasted directly. Code blocks
  with `text` show expected output.
- Environment variables are `UPPER_SNAKE_CASE`.
- File paths in square brackets are placeholders: replace `[your-token]`
  with your actual value, brackets included.

## Where to next

- For **internal architecture and design rationale**, see
  [docs/en/ma-harness-arch-map.md](../ma-harness-arch-map.md)
- For **plugin schema reference**, see
  [docs/en/plugin-schema-v1.md](../plugin-schema-v1.md)
- For **decision history** (why things are the way they are), see
  [docs/zh-CN/decision-log.md](../../zh-CN/decision-log.md)
- For **development plan** (what's done, what's next), see
  [docs/en/development-plan.md](../development-plan.md)
