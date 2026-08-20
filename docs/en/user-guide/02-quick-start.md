# 02 — Quick Start

> **Goal**: run your first agent locally and see the event log.

[English](02-quick-start.md) | [简体中文](../../zh-CN/user-guide/02-quick-start.md)

## Prerequisites

- `mah` CLI installed (see [01-installation.md](01-installation.md))
- ~5 minutes

## What you'll do

Run a one-shot agent via `mah run`, which:
1. Creates a session
2. Sends your prompt to a model (stub by default, or real LLM via env vars)
3. Streams the response back
4. Persists events to the event log

## Step-by-step

### Step 1 — Run a stub agent

The stub model echoes your prompt. It's the fastest way to verify your
install works end-to-end:

```bash
mah run "hello, world"
```

Expected output:

```
[stub] echo: hello, world
Session: local-39af1fb0-...
Content: [stub] echo: hello, world
Tokens: prompt=10 completion=20
```

### Step 2 — Inspect the event log

Every event is persisted. The default path is `~/.ma-harness/events.db`:

```bash
# Show events for the most recent session
mah events local-39af1fb0-...
```

Expected output (4 events per agent run):

```
[RunStart]      2026-08-20T16:00:00Z  run_id=...
[ModelRequest]  2026-08-20T16:00:00Z  payload={"model":"stub","messages":1}
[ModelResponse] 2026-08-20T16:00:00Z  payload={"content":"[stub] echo: ..."}
[RunEnd]        2026-08-20T16:00:00Z  status=ok
```

### Step 3 — Use a real LLM (OpenAI)

Set your API key:

```bash
export OPENAI_API_KEY="sk-..."
```

Then pass the model spec:

```bash
mah run "tell me a joke" --model "openai:gpt-4o-mini"
```

The `openai:` prefix is required (we support `openai:` and `anthropic:`
adapters).

### Step 4 — Anthropic

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
mah run "summarize: 'Rust async is hard'" --model "anthropic:claude-3-5-sonnet-20241022"
```

### Step 5 — Use a session ID (resume)

By default, every `mah run` creates a new session. To resume:

```bash
# First run — note the session id from output
mah run "what's the weather in Tokyo?" --session "weather-app"

# Follow-up — same session, context preserved
mah run "how about in Paris?" --session "weather-app"
```

The session ID is freeform (any string). The agent uses it as a key for
context lookup.

## Verify

After running, you should see:

- 4 events in the log per `mah run` call
- Session ID printed in stdout
- A SQLite database at `~/.ma-harness/events.db`

```bash
ls -la ~/.ma-harness/
# drwxr-xr-x  .ma-harness
# -rw-r--r--  events.db    <-- session event log
# -rw-r--r--  sessions.db  <-- session metadata (P5+)
```

## What's next

You've seen the basic loop. Now:

- Add **plugins** to extend what your agent can do — see [04-plugins.md](04-plugins.md)
- **Deploy a server** so multiple clients can connect — see [03-server.md](03-server.md)
- **Validate** your agent against a known-good behavior — see [05-conformance.md](05-conformance.md)

## Troubleshooting

### "stub model" appears even when I set `OPENAI_API_KEY`

The `--model` flag is required. `mah run` defaults to `stub`:

```bash
# ❌ uses stub
mah run "hello"

# ✅ uses OpenAI
mah run --model "openai:gpt-4o-mini" "hello"
```

### "401 Unauthorized" from OpenAI

Check your API key:

```bash
echo $OPENAI_API_KEY     # Linux / macOS
$env:OPENAI_API_KEY      # PowerShell
```

If it's set but still 401, regenerate at <https://platform.openai.com/api-keys>.

### "network error" or "connection refused" on real LLM call

If you're behind a corporate firewall / proxy:

```bash
# Linux / macOS
export HTTPS_PROXY=http://your-proxy:8080

# PowerShell
$env:HTTPS_PROXY = "http://your-proxy:8080"

# Make sure NO_PROXY lets localhost through (mah start uses 127.0.0.1)
export NO_PROXY="localhost,127.0.0.1"
```

### Events db file is huge

Each `mah run` creates 4+ events. For long-running usage, vacuum
periodically:

```bash
sqlite3 ~/.ma-harness/events.db "VACUUM;"
```
