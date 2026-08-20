# 02 — 快速开始

> **目标**: 本地跑你的第一个 agent,看 event log。

[English](02-quick-start.md) | [简体中文](02-quick-start.md)

## 前置条件

- 装好 `mah` CLI (见 [01-installation.md](01-installation.md))
- ~5 分钟

## 你要做什么

通过 `mah run` 跑一个一次性 agent:
1. 建 session
2. 发送 prompt 到模型 (默认 stub,或通过环境变量走真 LLM)
3. 流回响应
4. 事件持久化到 event log

## 步骤

### 第 1 步 — 跑 stub agent

Stub 模型会回显你的 prompt。这是验证端到端最快的方式:

```bash
mah run "hello, world"
```

期望输出:

```
[stub] echo: hello, world
Session: local-39af1fb0-...
Content: [stub] echo: hello, world
Tokens: prompt=10 completion=20
```

### 第 2 步 — 看 event log

每个事件都持久化。默认路径 `~/.ma-harness/events.db`:

```bash
# 看最近 session 的事件
mah events local-39af1fb0-...
```

期望输出 (每次 agent run 4 个事件):

```
[RunStart]      2026-08-20T16:00:00Z  run_id=...
[ModelRequest]  2026-08-20T16:00:00Z  payload={"model":"stub","messages":1}
[ModelResponse] 2026-08-20T16:00:00Z  payload={"content":"[stub] echo: ..."}
[RunEnd]        2026-08-20T16:00:00Z  status=ok
```

### 第 3 步 — 用真 LLM (OpenAI)

设 API key:

```bash
export OPENAI_API_KEY="sk-..."
```

然后传 model spec:

```bash
mah run "讲个笑话" --model "openai:gpt-4o-mini"
```

`openai:` 前缀必填 (支持 `openai:` 跟 `anthropic:` adapter)。

### 第 4 步 — Anthropic

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
mah run "总结: 'Rust async 真难'" --model "anthropic:claude-3-5-sonnet-20241022"
```

### 第 5 步 — 用 session id (续接)

默认每次 `mah run` 新建 session。续接:

```bash
# 第一次 — 记下 stdout 的 session id
mah run "东京天气?" --session "weather-app"

# 续接 — 同 session,上下文保留
mah run "巴黎呢?" --session "weather-app"
```

session id 任意字符串。agent 用它作上下文查找的 key。

## 验证

跑完后应该看到:

- 每次 `mah run` 4 个事件
- stdout 打印 session ID
- SQLite 数据库在 `~/.ma-harness/events.db`

```bash
ls -la ~/.ma-harness/
# drwxr-xr-x  .ma-harness
# -rw-r--r--  events.db    <-- session event log
# -rw-r--r--  sessions.db  <-- session metadata (P5+)
```

## 下一步

你看到了基本循环。现在:

- 加 **插件** 扩展 agent 能力 — 见 [04-plugins.md](04-plugins.md)
- **部署 server** 让多 client 连 — 见 [03-server.md](03-server.md)
- **验证** agent 跟已知行为 — 见 [05-conformance.md](05-conformance.md)

## Troubleshooting

### 设了 `OPENAI_API_KEY` 但用 stub

`--model` 必填。`mah run` 默认 stub:

```bash
# ❌ 用 stub
mah run "hello"

# ✅ 用 OpenAI
mah run --model "openai:gpt-4o-mini" "hello"
```

### OpenAI 返 `401 Unauthorized`

检查 API key:

```bash
echo $OPENAI_API_KEY     # Linux / macOS
$env:OPENAI_API_KEY      # PowerShell
```

如果设了还是 401,去 <https://platform.openai.com/api-keys> 重新生成。

### 真 LLM 报 "network error" 或 "connection refused"

在公司防火墙 / 代理后面:

```bash
# Linux / macOS
export HTTPS_PROXY=http://your-proxy:8080

# PowerShell
$env:HTTPS_PROXY = "http://your-proxy:8080"

# NO_PROXY 让 localhost 通过 (mah start 用 127.0.0.1)
export NO_PROXY="localhost,127.0.0.1"
```

### Events db 文件巨大

每次 `mah run` 4+ 个事件。长期跑定期 vacuum:

```bash
sqlite3 ~/.ma-harness/events.db "VACUUM;"
```
