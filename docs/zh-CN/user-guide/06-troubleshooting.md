# 06 — Troubleshooting

> **目标**: 快速诊断和修常见 `mah` 跟 agent 问题。如果不在这里,
> 在 [GitHub](https://github.com/ma-harness/ma-harness.rs/issues) 提 issue。

[English](06-troubleshooting.md) | [简体中文](06-troubleshooting.md)

## 速查

| 症状 | 先检查 | 见下面 |
|---|---|---|
| `mah: command not found` | `$PATH` 含 `~/.cargo/bin`? | [Install](#install) |
| 总是用 stub model | 传 `--model` 了? | [Models](#models) |
| OpenAI 返 `401 Unauthorized` | API key 设了?有效? | [Models](#models) |
| gRPC connection refused | server 跑着?防火墙? | [Networking](#networking) |
| gRPC 过 nginx 失败 | listen 有 `http2`? | [Networking](#networking) |
| 插件不在 `mah plugins` 里 | import 了? | [Plugins](#plugins) |
| Events db 缺 | 磁盘满?路径错? | [Storage](#storage) |
| 内存高 | active session 数? | [Storage](#storage) |
| CI 跑 test 失败 | 网络?`RUST_TEST_THREADS=1`? | [Tests](#tests) |
| `error: linker not found` (Windows) | 装 VS Build Tools? | [Build](#build) |
| 文档中文乱码 | PowerShell 读 UTF-8 | [Misc](#misc) |

## Install

### `cargo install` 后 `mah: command not found`

Cargo 装在 `~/.cargo/bin/` (Linux/macOS) 或
`%USERPROFILE%\.cargo\bin\` (Windows)。检查 PATH:

```bash
# Linux / macOS
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# Windows PowerShell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
```

验证:

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

装 Visual Studio Build Tools:
<https://visualstudio.microsoft.com/visual-cpp-build-tools/>
选 "Desktop development with C++"。

## Build

### `protoc not found`

我们 vendor 了 protoc,很少见。如果真碰到:

```bash
# macOS
brew install protobuf

# Debian / Ubuntu
sudo apt install protobuf-compiler

# 或 override
PROTOC=/path/to/protoc cargo build
```

### 编译磁盘满

`target/` 可到 5-10 GB。清理:

```bash
cargo clean                              # 删所有 build artifacts
cargo clean --release                    # 只删 release artifacts

# 或只清 ma-harness-* crate
cargo clean -p ma-harness-cordis
```

### 增量编译慢

加 `target-dir` 到 `~/.cargo/config.toml`:

```toml
[build]
target-dir = "/path/to/shared/target"  # 如 D:\rust_target
```

所有 build 放一个地方,共享依赖 cache。

## Models

### 设了 `OPENAI_API_KEY` 但用 stub

`mah run` 默认 stub。必须传 `--model`:

```bash
# ❌ 用 stub
mah run "hello"

# ✅ 用 OpenAI
mah run --model "openai:gpt-4o-mini" "hello"
```

### OpenAI 返 `401 Unauthorized`

1. 检查 key:
   ```bash
   echo $OPENAI_API_KEY   # Linux / macOS
   $env:OPENAI_API_KEY    # PowerShell
   ```
2. 直接验证:
   ```bash
   curl -H "Authorization: Bearer $OPENAI_API_KEY" \
        https://api.openai.com/v1/models
   ```
3. 需要就去 <https://platform.openai.com/api-keys> 重新生成。

### Anthropic 报 `401` 或 `model not found`

Anthropic model 名字带日期戳。用最新的:

```bash
# 列可用 model
curl -H "x-api-key: $ANTHROPIC_API_KEY" \
     -H "anthropic-version: 2023-06-01" \
     https://api.anthropic.com/v1/models

# 用最新的 (如 claude-3-5-sonnet-20241022)
mah run --model "anthropic:claude-3-5-sonnet-20241022" "hello"
```

### 真 LLM 网络 timeout

在公司代理后面:

```bash
export HTTPS_PROXY=http://your-proxy:8080
export NO_PROXY="localhost,127.0.0.1"
```

## Networking

### gRPC 客户端: "connection refused"

1. server 跑着?
   ```bash
   systemctl status mah-harness    # systemd
   curl http://localhost:50050/health
   ```

2. 端口开着?
   ```bash
   # Linux
   sudo ss -tlnp | grep 50051
   # Windows
   Get-NetTCPConnection -LocalPort 50051
   ```

3. 防火墙?
   ```bash
   # Linux
   sudo ufw allow 50051/tcp
   # macOS (if firewall enabled)
   sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /usr/local/bin/mah
   ```

### gRPC 过 nginx: "connection reset by peer"

nginx 的 listen 指令需要 `http2` (HTTP/2 cleartext → gRPC):

```nginx
server {
    listen 443 ssl http2;   # ← http2 必填
    # ...
}
```

### gRPC 本机通,远端不通

通常是防火墙或代理。测:

```bash
# 别的机器
nc -zv mah.example.com 50051    # 应该通
grpcurl mah.example.com:50051 list  # 应该列 services
```

`nc` 不通是网络/防火墙。`nc` 通但 `grpcurl` 不通是 TLS/代理。

## Plugins

### 插件不在 `mah plugins` 列表

忘了 import:

```rust
use my_plugin as _;  # ← 触发 inventory::submit!
```

不 import,插件编了但不注册。

### "duplicate plugin name" 错误

两个插件同名。要么改名 (在 `impl Plugin for MyPlugin { fn name() -> &str { ... } }`),
要么用 feature flag 关一个:

```toml
[features]
default = []
my-plugin-b = []
```

### Tool 对 LLM 不可见

`#[dsh_tool]` 宏生成 JSON schema。不用,方法就只是 service method:

```rust
// ✅ Tool — LLM 可见
#[dsh_tool(name = "search", description = "Search docs")]
impl MyService { pub async fn search(&self, ...) -> ... { } }

// ❌ 只是 service method
impl MyService { pub async fn search(&self, ...) -> ... { } }
```

## Storage

### Events db 巨大

`~/.ma-harness/events.db` 是 append-only。定期 vacuum:

```bash
sqlite3 ~/.ma-harness/events.db "VACUUM;"
```

长期归档:

```bash
sqlite3 ~/.ma-harness/events.db ".backup /backup/events-$(date +%F).db"
```

### "database is locked" 错误

SQLite 默认 serializable isolation。并发多 `mah run` 会锁竞争。解决:
- 用 `--store-path` 真 DB,不是 `/tmp/`
- 降并发 (用队列)
- 升级 Postgres (P12+)

### 内存无限增长

`mah start` 把 active session 放内存。检查:

```bash
mah sessions list
# Count: 1234 sessions
```

太多就定期重启 (systemd `RestartSec=300`)。

## Tests

### 本地过 CI 失败

最常见: race condition。我们用 `RUST_TEST_THREADS=1` 测:

```yaml
# .github/workflows/test.yml
- name: Run tests
  run: RUST_TEST_THREADS=1 cargo test --workspace --lib
```

### CI 报 `linker not found`

GitHub Actions ubuntu-latest 自带 gcc,macos-latest 自带 clang,Windows 自带 MSVC。
如果你看到,你 Dockerfile / action 缺 build tools。

### CI 报 `protoc not found`

应该能用 (我们 vendor)。如果不行:

```bash
sudo apt-get install -y protobuf-compiler
```

## Misc

### 文档中文乱码 (PowerShell)

PowerShell 的 `Get-Content` 默认系统 ANSI (中文 Windows 是 GBK),会损坏 UTF-8 文件。用:

```powershell
Get-Content file.md -Encoding utf8
```

或设默认:

```powershell
$PSDefaultParameterValues['Get-Content:Encoding'] = 'utf8'
$OutputEncoding = [System.Text.Encoding]::UTF8
```

### `mah start` 报 "address already in use"

别的进程占着端口。找:

```bash
# Linux
sudo lsof -i :50051

# Windows
Get-NetTCPConnection -LocalPort 50051
```

要么 kill,要么改端口:

```bash
mah start --grpc-port 60051 --http-port 60050
```

### Rust 报 "this file contains an unclosed delimiter"

语法错误,通常是少了 `{` 或 `}`。跑:

```bash
cargo check 2>&1 | grep -A 3 "unclosed"
# 会指到具体行
```

## 还是卡住?

1. 搜 [GitHub issues](https://github.com/ma-harness/ma-harness.rs/issues?q=is%3Aissue)
2. 提新 issue 带:
   - `mah version` 输出
   - `cargo --version` 输出
   - 完整错误信息
   - 最小复现 (一行命令,如可能)
3. 实时聊天看项目 README 的社区链接
