# 01 — Installation

> **Goal**: get `mah` CLI on your machine and verify it works.
> Pick your operating system and jump to the matching section.

[English](01-installation.md) | [简体中文](../../zh-CN/user-guide/01-installation.md)

## Pick your OS

| OS | Jump to |
|---|---|
| **Linux** (Debian / Ubuntu / Fedora / Arch / etc.) | [Linux install](#linux) |
| **macOS** (Intel / Apple Silicon) | [macOS install](#macos) |
| **Windows** (10 / 11, PowerShell or WSL) | [Windows install](#windows) |

> If you only need the **Python SDK** (`mah-py`) and not the Rust `mah` CLI,
> skip the Rust toolchain and jump to [Python SDK](#python-sdk-mah-py).

## Common prerequisites (all OSes)

| What | Version | Notes |
|---|---|---|
| **Rust** | 1.94+ | `rustc --version` |
| **Cargo** | (with Rust) | `cargo --version` |
| **protoc** (optional) | 3.x | vendored in `vendor/`; only needed if the build fails |
| **Internet access** | — | to download crates and (optionally) PyPI |
| **~3 GB free disk** | — | Rust toolchain + target dir + crate registry cache |

> `mah` links `ma-harness-proto` which uses tonic-build; the binary release
> vendors a `protoc` so you usually do **not** need to install one
> system-wide. Skip unless you see `protoc not found` during build.

---

## Linux

Tested on: **Ubuntu 22.04 / 24.04**, **Debian 12**, **Fedora 40**, **Arch** (rolling).
Other distros work the same — pick your package manager.

### 1. Install Rust toolchain

```bash
# 1.1 Install rustup (the official Rust installer)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# 1.2 Verify
rustc --version   # rustc 1.94.x (...)
cargo --version
```

> For distro packages (`apt install rustc` etc.), versions are often older
> than 1.94. Always use **rustup** unless your distro ships a recent toolchain.

### 2. Install build tools

You need a C linker and `pkg-config` for some transitive deps (rusqlite / openssl-sys).

```bash
# Debian / Ubuntu
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev

# Fedora
sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config openssl-devel

# Arch
sudo pacman -S --needed base-devel openssl
```

### 3. (Optional) Install `protoc` system-wide

Only needed if vendored `protoc` is missing on your platform.

```bash
# Debian / Ubuntu
sudo apt install -y protobuf-compiler

# Fedora
sudo dnf install -y protobuf-compiler

# Arch
sudo pacman -S --needed protobuf

# Verify
protoc --version   # libprotoc 3.x.x
```

### 4. Install `mah` CLI

#### Option A — from source (recommended during development)

```bash
git clone https://github.com/ma-harness/ma-harness.rs.git
cd ma-harness.rs
cargo build --release -p ma-harness-cli
# Binary at target/release/mah
ls -la target/release/mah
```

For shared build cache (recommended if you build other Rust projects):

```bash
export CARGO_TARGET_DIR="$HOME/.cache/cargo-target"
cargo build --release -p ma-harness-cli
```

> Add `export CARGO_TARGET_DIR=...` to your `~/.bashrc` to persist.

#### Option B — from crates.io (once published)

```bash
cargo install ma-harness-cli
# Binary at ~/.cargo/bin/mah
ls -la "$HOME/.cargo/bin/mah"
```

### 5. Add `mah` to your `PATH`

`cargo install` puts the binary in `~/.cargo/bin/`. Make sure it's in your `PATH`:

```bash
# bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# zsh
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### 6. Verify

```bash
which mah           # should print ~/.cargo/bin/mah
mah version         # mah 0.1.0 (...)
mah --help          # lists 9 subcommands (start / run / plugins / events / ...)
```

### Update / Uninstall

```bash
# Update from source
cd ma-harness.rs && git pull && cargo build --release -p ma-harness-cli

# Update from crates.io
cargo install ma-harness-cli --force

# Uninstall
cargo uninstall ma-harness-cli
```

---

## macOS

Tested on: **macOS 14 Sonoma (Intel)**, **macOS 15 Sequoia (Apple Silicon M1/M2/M3/M4)**.
Both work; the install commands are identical.

### 1. Install Xcode Command Line Tools

You need `clang`, `git`, and the macOS SDK headers.

```bash
xcode-select --install
# A dialog appears. Click "Install", accept the license, wait ~5 min.
xcode-select -p    # should print /Library/Developer/CommandLineTools
```

### 2. Install Homebrew (optional but recommended)

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Apple Silicon: add brew to PATH (Intel users can skip)
echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
eval "$(/opt/homebrew/bin/brew shellenv)"

brew --version
```

### 3. Install Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

rustc --version
cargo --version
```

### 4. Install build tools

macOS has `clang` from step 1; you only need `pkg-config` and OpenSSL if you
build crates that link against it.

```bash
# Homebrew
brew install pkg-config openssl protobuf   # protobuf is optional
```

> On Apple Silicon, Homebrew installs to `/opt/homebrew/`. `pkg-config` will
> find OpenSSL automatically; if not, set:
> `export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl/lib/pkgconfig"`.

### 5. Install `mah` CLI

#### Option A — from source

```bash
git clone https://github.com/ma-harness/ma-harness.rs.git
cd ma-harness.rs
cargo build --release -p ma-harness-cli
ls -la target/release/mah
```

For shared build cache:

```bash
export CARGO_TARGET_DIR="$HOME/.cache/cargo-target"
cargo build --release -p ma-harness-cli
```

#### Option B — from crates.io

```bash
cargo install ma-harness-cli
ls -la "$HOME/.cargo/bin/mah"
```

### 6. Add `mah` to your `PATH`

```bash
# zsh (default shell on macOS 10.15+)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc

# bash (older Macs)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bash_profile
source ~/.bash_profile
```

### 7. Verify

```bash
which mah
mah version
mah --help
```

### Update / Uninstall

```bash
# Update from source
cd ma-harness.rs && git pull && cargo build --release -p ma-harness-cli

# Update from crates.io
cargo install ma-harness-cli --force

# Uninstall
cargo uninstall ma-harness-cli
```

---

## Windows

Tested on: **Windows 10 (21H2+)**, **Windows 11**, **PowerShell 5.1 / 7.x**.
The instructions use **PowerShell**; if you run **WSL** (Ubuntu inside Windows),
follow the [Linux](#linux) section instead.

### 1. Install Visual Studio Build Tools (C++ toolchain)

Rust on Windows needs the MSVC linker. Install the free Build Tools.

1. Download: <https://visualstudio.microsoft.com/visual-cpp-build-tools/>
2. Run the installer.
3. In the **Workloads** tab, check **"Desktop development with C++"**.
4. On the right side, ensure these are selected:
   - MSVC v143 (or latest)
   - Windows 11 SDK (or Windows 10 SDK)
   - C++ CMake tools for Windows
5. Click **Install** (~3-5 GB, takes 10-20 min).

> If you already have Visual Studio (any edition) with C++ workload, skip this step.

### 2. Install Rust toolchain (rustup-init.exe)

```powershell
# Download and run the rustup installer (in PowerShell)
Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile '$env:TEMP\rustup-init.exe'
& "$env:TEMP\rustup-init.exe" -y
# Restart the terminal, then:
rustc --version
cargo --version
```

> On ARM64 Windows, replace `x86_64` with `aarch64` in the URL above.

### 3. Install Git for Windows (if not already installed)

```powershell
git --version
# If not found, install from https://git-scm.com/download/win and restart the terminal.
```

### 4. (Optional) Install `protoc` system-wide

```powershell
# Option A: winget (Windows 11 / Windows 10 21H2+)
winget install --id=Google.Protobuf -e

# Option B: choco (if you use Chocolatey)
choco install protoc

# Option C: scoop
scoop install protobuf

# Verify
protoc --version
```

### 5. Install `mah` CLI

#### Option A — from source

Open **"x64 Native Tools Command Prompt for VS"** (or run `vcvarsall.bat x64`
in your current shell) so the MSVC linker is on `PATH`. Then:

```powershell
git clone https://github.com/ma-harness/ma-harness.rs.git
cd ma-harness.rs
cargo build --release -p ma-harness-cli
# Binary at target\release\mah.exe
Get-Item target\release\mah.exe
```

For shared build cache (recommended — saves 1-2 GB per project):

```powershell
$env:CARGO_TARGET_DIR = "D:\rust_target"
[Environment]::SetEnvironmentVariable("CARGO_TARGET_DIR", "D:\rust_target", "User")
cargo build --release -p ma-harness-cli
# Binary at D:\rust_target\release\mah.exe
```

> **Tip**: set `CARGO_TARGET_DIR` via **System Properties → Environment Variables**
> (User variables) so it persists across all shells.

#### Option B — from crates.io

```powershell
cargo install ma-harness-cli
# Binary at $env:USERPROFILE\.cargo\bin\mah.exe
Get-Item "$env:USERPROFILE\.cargo\bin\mah.exe"
```

### 6. Add `mah` to your `PATH`

`cargo install` puts the binary in `%USERPROFILE%\.cargo\bin\`. Make sure it's in `PATH`:

```powershell
# Check current PATH
$env:PATH -split ';' | Select-String '\.cargo'

# If not present, add for this session
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# Make it permanent (User PATH)
[Environment]::SetEnvironmentVariable(
    "PATH",
    "$env:USERPROFILE\.cargo\bin;$([Environment]::GetEnvironmentVariable('PATH','User'))",
    "User"
)
# Restart the terminal for the new PATH to take effect.
```

> PowerShell 7+ has `Add-Content $PROFILE '...'` for persistent shell config;
> for the system PATH, use the `[Environment]::SetEnvironmentVariable` above.

### 7. Verify

```powershell
Get-Command mah
# Expected: ...\.cargo\bin\mah.exe

mah version
# Expected: mah 0.1.0 (...)

mah --help
# Expected: list of 9 subcommands
```

### Update / Uninstall

```powershell
# Update from source (must use VS Developer Prompt, or with vcvarsall sourced)
cd ma-harness.rs
git pull
cargo build --release -p ma-harness-cli

# Update from crates.io
cargo install ma-harness-cli --force

# Uninstall
cargo uninstall ma-harness-cli
```

### WSL (alternative)

If you prefer Linux inside Windows:

```powershell
wsl --install -d Ubuntu-24.04
# Restart, then open "Ubuntu" from Start Menu, and follow the [Linux](#linux) section.
```

> Inside WSL, the Linux file system (`/home/you/...`) is much faster for
> Rust builds than the Windows drive (`/mnt/c/...`).

---

## Python SDK (`mah-py`)

Works on **all three OSes**. Install from PyPI:

```bash
# Linux / macOS
pip install mah-py

# Windows (PowerShell)
pip install mah-py

# Verify
python -c "from mah_py import Mah; print('OK')"
```

For Chinese users behind a slow PyPI:

```bash
pip install -i https://pypi.tuna.tsinghua.edu.cn/simple mah-py
```

See [crates/mah-py/README.md](../../../crates/mah-py/README.md) for the full API.

---

## Troubleshooting

### Linux: `error: linker 'cc' not found`

You skipped step 2 (build tools). Install them:

```bash
sudo apt install -y build-essential      # Debian / Ubuntu
sudo dnf install -y gcc gcc-c++ make     # Fedora
sudo pacman -S --needed base-devel       # Arch
```

### macOS: `error: linker 'cc' not found`

You skipped step 1 (Xcode CLT):

```bash
xcode-select --install
```

### macOS: OpenSSL not found during build

Some crates need OpenSSL on macOS:

```bash
brew install openssl
export PKG_CONFIG_PATH="$(brew --prefix openssl)/lib/pkgconfig"
# Apple Silicon: export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl/lib/pkgconfig"
```

### Windows: `error: linker 'link.exe' not found`

You skipped step 1 (VS Build Tools). Either:

1. Install **Visual Studio Build Tools** with the **C++ workload** (recommended).
2. Or use the **x64 Native Tools Command Prompt for VS** that ships with
   Build Tools — it has `link.exe` on `PATH`.

### Windows: `protoc not found`

You skipped step 4. Install via `winget install Google.Protobuf` or use the
vendored binary:

```powershell
$env:PROTOC = "$PWD\vendor\protoc\binary\protoc.exe"
cargo build --release -p ma-harness-cli
```

### `mah` command not found after install

`cargo install` puts the binary in `~/.cargo/bin/` (Linux/macOS) or
`%USERPROFILE%\.cargo\bin\` (Windows). See step 5/6 of your OS section
above for adding it to `PATH`.

### `error: failed to download ...` (network / proxy)

If you're behind a corporate proxy or in a region with slow access to
crates.io / GitHub:

```bash
# Use a Chinese mirror (rsproxy)
$env:CARGO_REGISTRIES_CRATES_IO_PROTOCOL = "git"  # PowerShell
# Or edit ~/.cargo/config.toml:
cat >> ~/.cargo/config.toml <<'EOF'
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy]
registry = "https://rsproxy.cn/crates.io-index"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
EOF
```

### `disk full` during `cargo build`

The `target/` directory can grow to **2-5 GB**. Free up space, or move it
to a larger drive via `CARGO_TARGET_DIR`.

## Next

- [02-quick-start.md](02-quick-start.md) — run your first agent (works on all 3 OSes)
- [03-server.md](03-server.md) — deploy `mah start` (covers Linux systemd + Windows service)
