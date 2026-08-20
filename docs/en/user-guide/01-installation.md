# 01 — Installation

> **Goal**: get `mah` CLI on your machine and verify it works.

[English](01-installation.md) | [简体中文](../../zh-CN/user-guide/01-installation.md)

## Prerequisites

| What | Version | How to check |
|---|---|---|
| **Rust** | 1.94+ | `rustc --version` |
| **Cargo** | (comes with Rust) | `cargo --version` |
| **protoc** (optional) | 3.x | `protoc --version` — only needed if you build `ma-harness-proto` from source |
| **OS** | Linux / macOS / Windows | — |

> **Note**: The `mah` binary links `ma-harness-proto` which uses tonic-build.
> Pre-built `protoc` is included in `vendor/`, so you usually don't need
> to install it system-wide. Skip unless you see a `protoc not found`
> error during build.

## Install `mah` CLI

### Option A — from source (recommended during development)

```bash
git clone https://github.com/ma-harness/ma-harness.rs.git
cd ma-harness.rs
cargo build --release -p ma-harness-cli
# Binary at target/release/mah
ls -la target/release/mah
```

Or, if you use the shared `target-dir` (recommended for Windows / shared caches):

```bash
CARGO_TARGET_DIR=/path/to/shared/target cargo build --release -p ma-harness-cli
```

### Option B — from crates.io (once published)

```bash
cargo install ma-harness-cli
# Binary at ~/.cargo/bin/mah
```

### Option C — Python SDK (`mah-py`)

```bash
pip install mah-py

# Verify
python -c "from mah_py import Mah; print('OK')"
```

See [crates/mah-py/README.md](../../../crates/mah-py/README.md) for details.

## Verify installation

```bash
# 1. mah binary present
which mah    # or: Get-Command mah

# 2. version check
mah version
# Expected: mah 0.1.0 (or current)

# 3. help
mah --help
# Expected: list of 9 subcommands (start / run / plugins / events / ...)
```

## Update

```bash
# from source
git pull && cargo build --release -p ma-harness-cli

# from crates.io
cargo install ma-harness-cli --force

# Python SDK
pip install --upgrade mah-py
```

## Uninstall

```bash
# mah CLI
cargo uninstall ma-harness-cli

# Python SDK
pip uninstall mah-py
```

## Troubleshooting

### `error: linker 'cc' not found` (Linux)

Install a C compiler:

```bash
# Debian / Ubuntu
sudo apt install build-essential

# Fedora
sudo dnf install gcc

# Arch
sudo pacman -S base-devel
```

### `protoc not found` during build

This is rare (we vendor protoc in `vendor/`), but if you see it:

```bash
# macOS
brew install protobuf

# Debian / Ubuntu
sudo apt install protobuf-compiler

# Or set PROTOC env var to a local binary
PROTOC=./vendor/protoc/binary/protoc cargo build
```

### `error: linker not found` on Windows

Install Visual Studio Build Tools:
<https://visualstudio.microsoft.com/visual-cpp-build-tools/>

During install, select "Desktop development with C++".

### `mah` command not found after install

Cargo installs to `~/.cargo/bin/`. Make sure it's in your `PATH`:

```bash
# bash / zsh
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# PowerShell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
# (add to $PROFILE for persistence)
```

## Next

- [02-quick-start.md](02-quick-start.md) — run your first agent
- [03-server.md](03-server.md) — deploy `mah start` to production
