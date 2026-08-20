# 06 — Troubleshooting

> **Goal**: quickly diagnose and fix common issues with `mah` and your
> agents. If your problem isn't here, file an issue on
> [GitHub](https://github.com/ma-harness/ma-harness.rs/issues).

[English](06-troubleshooting.md) | [简体中文](../../zh-CN/user-guide/06-troubleshooting.md)

## Quick reference

| Symptom | First thing to check | See below |
|---|---|---|
| `mah: command not found` | `$PATH` includes `~/.cargo/bin`? | [Install](#install) |
| Stub model always used | Did you pass `--model`? | [Models](#models) |
| `401 Unauthorized` from OpenAI | API key set? valid? | [Models](#models) |
| gRPC connection refused | Server running? firewall? | [Networking](#networking) |
| gRPC through nginx fails | Is `http2` in `listen`? | [Networking](#networking) |
| Plugin not in `mah plugins` | Did you import it? | [Plugins](#plugins) |
| Events db missing | Disk full? path wrong? | [Storage](#storage) |
| High memory | Active session count? | [Storage](#storage) |
| Tests fail in CI | Network access? `RUST_TEST_THREADS=1`? | [Tests](#tests) |
| `error: linker not found` (Windows) | VS Build Tools installed? | [Build](#build) |
| Chinese mojibake in docs | PowerShell reading UTF-8 | [Misc](#misc) |

## Install

### `mah: command not found` after `cargo install`

Cargo installs to `~/.cargo/bin/` (Linux/macOS) or
`%USERPROFILE%\.cargo\bin\` (Windows). Make sure it's in your `PATH`:

```bash
# Linux / macOS
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# Windows PowerShell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
```

Verify:

```bash
which mah    # Linux / macOS
Get-Command mah  # PowerShell
```

### `error: linker 'cc' not found` (Linux)

```bash
sudo apt install build-essential   # Debian / Ubuntu
sudo dnf install gcc               # Fedora
sudo pacman -S base-devel          # Arch
```

### `error: linker not found` (Windows)

Install Visual Studio Build Tools:
<https://visualstudio.microsoft.com/visual-cpp-build-tools/>
Select "Desktop development with C++".

## Build

### `protoc not found`

We vendor protoc in `vendor/`, so this is rare. If you see it:

```bash
# macOS
brew install protobuf

# Debian / Ubuntu
sudo apt install protobuf-compiler

# Or override
PROTOC=/path/to/protoc cargo build
```

### Out of disk during build

`target/` can grow to 5-10 GB. Clean it:

```bash
cargo clean                              # remove all build artifacts
cargo clean --release                    # remove release artifacts only

# Or just the ma-harness-* crates
cargo clean -p ma-harness-cordis
```

### Slow incremental builds

Add `target-dir` to your `~/.cargo/config.toml`:

```toml
[build]
target-dir = "/path/to/shared/target"  # e.g. D:\rust_target
```

This puts all builds in one place, sharing the dependency cache.

## Models

### Stub model used despite `OPENAI_API_KEY` set

`mah run` defaults to `stub`. You must pass `--model`:

```bash
# ❌ uses stub
mah run "hello"

# ✅ uses OpenAI
mah run --model "openai:gpt-4o-mini" "hello"
```

### `401 Unauthorized` from OpenAI

1. Check the key is set:
   ```bash
   echo $OPENAI_API_KEY   # Linux / macOS
   $env:OPENAI_API_KEY    # PowerShell
   ```
2. Verify it works directly:
   ```bash
   curl -H "Authorization: Bearer $OPENAI_API_KEY" \
        https://api.openai.com/v1/models
   ```
3. If needed, regenerate at <https://platform.openai.com/api-keys>.

### `401` from Anthropic / `model not found`

Anthropic model names are date-stamped. Use the latest:

```bash
# Check available models
curl -H "x-api-key: $ANTHROPIC_API_KEY" \
     -H "anthropic-version: 2023-06-01" \
     https://api.anthropic.com/v1/models

# Use the latest (e.g. claude-3-5-sonnet-20241022)
mah run --model "anthropic:claude-3-5-sonnet-20241022" "hello"
```

### Network timeouts on real LLM

Behind a corporate proxy:

```bash
export HTTPS_PROXY=http://your-proxy:8080
export NO_PROXY="localhost,127.0.0.1"
```

## Networking

### gRPC client: "connection refused"

1. Is the server running?
   ```bash
   systemctl status mah-harness    # systemd
   curl http://localhost:50050/health
   ```

2. Is the port open?
   ```bash
   # Linux
   sudo ss -tlnp | grep 50051
   # Windows
   Get-NetTCPConnection -LocalPort 50051
   ```

3. Firewall?
   ```bash
   # Linux
   sudo ufw allow 50051/tcp
   # macOS (if firewall enabled)
   sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /usr/local/bin/mah
   ```

### gRPC through nginx: "connection reset by peer"

nginx needs `http2` directive (HTTP/2 cleartext → gRPC):

```nginx
server {
    listen 443 ssl http2;   # ← http2 is required
    # ...
}
```

### gRPC works on localhost, fails remotely

Almost always a firewall or proxy. Test:

```bash
# From another machine
nc -zv mah.example.com 50051    # should connect
grpcurl mah.example.com:50051 list  # should list services
```

If `nc` fails, it's a network/firewall issue. If `nc` succeeds but
`grpcurl` fails, it's a TLS/proxy issue.

## Plugins

### Plugin not in `mah plugins` list

You forgot to import the plugin crate:

```rust
use my_plugin as _;  // ← this triggers inventory::submit!
```

Without this import, the plugin is compiled but not registered.

### "duplicate plugin name" error

Two plugins claim the same name. Either:
- Rename one (in `impl Plugin for MyPlugin { fn name() -> &str { ... } }`)
- Disable one via a feature flag in `Cargo.toml`

### Tool not visible to LLM

The `#[dsh_tool]` macro generates the JSON schema. Without it, the method
is a regular service method, not a tool:

```rust
// ✅ This is a tool — visible to LLM
#[dsh_tool(name = "search", description = "Search docs")]
impl MyService { pub async fn search(&self, ...) -> ... { } }

// ❌ This is just a service method
impl MyService { pub async fn search(&self, ...) -> ... { } }
```

## Storage

### Events db is huge

`~/.ma-harness/events.db` is append-only. Vacuum periodically:

```bash
sqlite3 ~/.ma-harness/events.db "VACUUM;"
```

For long-term storage, archive:

```bash
sqlite3 ~/.ma-harness/events.db ".backup /backup/events-$(date +%F).db"
```

### "database is locked" errors

SQLite is configured for serializable isolation. If many `mah run` calls
fire simultaneously, you can get lock contention. Solutions:
- Use `--store-path` to a real DB, not `/tmp/`
- Reduce concurrency (use a queue)
- Upgrade to Postgres (P12+ feature)

### Memory grows unbounded

`mah start` holds all active sessions in memory. Check:

```bash
mah sessions list
# Count: 1234 sessions
```

If too many, restart periodically (systemd `RestartSec=300`).

## Tests

### Tests fail in CI but pass locally

Most common: race conditions. We test with `RUST_TEST_THREADS=1`:

```yaml
# .github/workflows/test.yml
- name: Run tests
  run: RUST_TEST_THREADS=1 cargo test --workspace --lib
```

### `linker not found` in CI

GitHub Actions ubuntu-latest has gcc. macos-latest has clang. Windows
has MSVC. If you see this, your Dockerfile / action is missing build tools.

### `protoc not found` in CI

Should work because we vendor. If not:

```bash
sudo apt-get install -y protobuf-compiler
```

## Misc

### Chinese mojibake in docs (PowerShell)

PowerShell's `Get-Content` defaults to system ANSI (GBK on Chinese
Windows), which mangles UTF-8 files. Use:

```powershell
Get-Content file.md -Encoding utf8
```

Or set the default:

```powershell
$PSDefaultParameterValues['Get-Content:Encoding'] = 'utf8'
$OutputEncoding = [System.Text.Encoding]::UTF8
```

### "address already in use" on `mah start`

Another process owns the port. Find it:

```bash
# Linux
sudo lsof -i :50051

# Windows
Get-NetTCPConnection -LocalPort 50051
```

Then either kill the process or change `mah`'s port:

```bash
mah start --grpc-port 60051 --http-port 60050
```

### "this file contains an unclosed delimiter" in Rust

A syntax error, usually a missing `{` or `}`. Run:

```bash
cargo check 2>&1 | grep -A 3 "unclosed"
# Points to the exact line
```

## Still stuck?

1. Search [GitHub issues](https://github.com/ma-harness/ma-harness.rs/issues?q=is%3Aissue)
2. Open a new issue with:
   - `mah version` output
   - `cargo --version` output
   - Full error message
   - Minimal reproduction (a one-liner command, if possible)
3. For real-time chat, see the project README for community links
