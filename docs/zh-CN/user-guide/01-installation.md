# 01 — 安装

> **目标**: 把 `mah` CLI 装上你的机器,验证能用。

[English](01-installation.md) | [简体中文](01-installation.md)

## 前置条件

| 工具 | 版本 | 怎么检查 |
|---|---|---|
| **Rust** | 1.94+ | `rustc --version` |
| **Cargo** | (跟 Rust 一起装) | `cargo --version` |
| **protoc** (可选) | 3.x | `protoc --version` — 只有从源码编 `ma-harness-proto` 才需要 |
| **OS** | Linux / macOS / Windows | — |

> **注意**: `mah` 二进制依赖 `ma-harness-proto`,后者用 tonic-build。
> 预编译的 `protoc` 已经在 `vendor/` 里,通常不用系统装。除非编译报 `protoc not found`。

## 装 `mah` CLI

### 方案 A — 从源码 (开发期推荐)

```bash
git clone https://github.com/ma-harness/ma-harness.rs.git
cd ma-harness.rs
cargo build --release -p ma-harness-cli
# 二进制在 target/release/mah
ls -la target/release/mah
```

或者用共享 `target-dir` (Windows / 共享缓存推荐):

```bash
CARGO_TARGET_DIR=/path/to/shared/target cargo build --release -p ma-harness-cli
```

### 方案 B — 从 crates.io (发版后)

```bash
cargo install ma-harness-cli
# 二进制在 ~/.cargo/bin/mah
```

### 方案 C — Python SDK (`mah-py`)

```bash
pip install mah-py

# 验证
python -c "from mah_py import Mah; print('OK')"
```

细节看 [crates/mah-py/README.md](../../../crates/mah-py/README.md)。

## 验证安装

```bash
# 1. mah 二进制在
which mah    # 或 PowerShell: Get-Command mah

# 2. 版本检查
mah version
# 期望: mah 0.1.0 (或当前版本)

# 3. 帮助
mah --help
# 期望: 9 个子命令列表 (start / run / plugins / events / ...)
```

## 更新

```bash
# 从源码
git pull && cargo build --release -p ma-harness-cli

# 从 crates.io
cargo install ma-harness-cli --force

# Python SDK
pip install --upgrade mah-py
```

## 卸载

```bash
# mah CLI
cargo uninstall ma-harness-cli

# Python SDK
pip uninstall mah-py
```

## Troubleshooting

### `error: linker 'cc' not found` (Linux)

装 C 编译器:

```bash
# Debian / Ubuntu
sudo apt install build-essential

# Fedora
sudo dnf install gcc

# Arch
sudo pacman -S base-devel
```

### 编译时 `protoc not found`

罕见 (我们已经 vendor 了),如果真碰到:

```bash
# macOS
brew install protobuf

# Debian / Ubuntu
sudo apt install protobuf-compiler

# 或者用 PROTOC 环境变量指本地二进制
PROTOC=./vendor/protoc/binary/protoc cargo build
```

### Windows 报 `error: linker not found`

装 Visual Studio Build Tools:
<https://visualstudio.microsoft.com/visual-cpp-build-tools/>
装的时候选 "Desktop development with C++"。

### 装完 `mah` 命令找不到

Cargo 装在 `~/.cargo/bin/`。检查 PATH:

```bash
# bash / zsh
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# PowerShell
$env:PATH = "$env:USERPROFILE\.cargoin;$env:PATH"
# (加到 $PROFILE 持久化)
```

## 下一步

- [02-quick-start.md](02-quick-start.md) — 跑你的第一个 agent
- [03-server.md](03-server.md) — 部署 `mah start` 到生产
