# 01 — 安装

> **目标**: 把 `mah` CLI 装上你的机器,验证能用。
> 先选你的操作系统,跳到对应章节。

[English](../../en/user-guide/01-installation.md) | [简体中文](01-installation.md)

## 选你的系统

| 系统 | 跳到 |
|---|---|
| **Linux** (Debian / Ubuntu / Fedora / Arch / 等) | [Linux 安装](#linux) |
| **macOS** (Intel / Apple Silicon) | [macOS 安装](#macos) |
| **Windows** (10 / 11, PowerShell 或 WSL) | [Windows 安装](#windows) |

> 如果你只要 **Python SDK** (`mah-py`) 不需要 Rust `mah` CLI,
> 跳过 Rust 工具链,直接看 [Python SDK](#python-sdk-mah-py)。

## 通用前置条件 (三个系统都要)

| 工具 | 版本 | 备注 |
|---|---|---|
| **Rust** | 1.94+ | `rustc --version` |
| **Cargo** | (跟 Rust 一起) | `cargo --version` |
| **protoc** (可选) | 3.x | `vendor/` 里已经 vendored, 编译失败再装 |
| **网络** | — | 拉 crates 跟 (可选) PyPI 包 |
| **磁盘** | ~3 GB 可用 | Rust 工具链 + target 目录 + crate 缓存 |

> `mah` 依赖 `ma-harness-proto` (用 tonic-build)。`vendor/` 里已经放了
> 预编译的 `protoc`,通常 **不用** 系统装。除非编译报 `protoc not found`。

---

## Linux

测试环境: **Ubuntu 22.04 / 24.04**, **Debian 12**, **Fedora 40**, **Arch** (rolling)。
其他发行版同理, 换包管理器命令即可。

### 1. 装 Rust 工具链

```bash
# 1.1 装 rustup (Rust 官方安装器)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# 1.2 验证
rustc --version   # rustc 1.94.x (...)
cargo --version
```

> 发行版仓库里的 rust (`apt install rustc` 等) 通常比 1.94 旧。
> 除非你发行版自带新版, 否则**总是用 rustup**。

### 2. 装编译工具

需要 C 链接器 + `pkg-config` (rusqlite / openssl-sys 等间接依赖)。

```bash
# Debian / Ubuntu
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev

# Fedora
sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config openssl-devel

# Arch
sudo pacman -S --needed base-devel openssl
```

### 3. (可选) 装系统级 `protoc`

只有 vendored `protoc` 在你的平台上没匹配时才需要。

```bash
# Debian / Ubuntu
sudo apt install -y protobuf-compiler

# Fedora
sudo dnf install -y protobuf-compiler

# Arch
sudo pacman -S --needed protobuf

# 验证
protoc --version   # libprotoc 3.x.x
```

### 4. 装 `mah` CLI

#### 方案 A — 从源码 (开发期推荐)

```bash
git clone https://github.com/ma-harness/ma-harness.rs.git
cd ma-harness.rs
cargo build --release -p ma-harness-cli
# 二进制在 target/release/mah
ls -la target/release/mah
```

共享 build 缓存 (同时跑多个 Rust 项目时推荐):

```bash
export CARGO_TARGET_DIR="$HOME/.cache/cargo-target"
cargo build --release -p ma-harness-cli
```

> 把 `export CARGO_TARGET_DIR=...` 加到 `~/.bashrc` 持久化。

#### 方案 B — 从 crates.io (发版后)

```bash
cargo install ma-harness-cli
# 二进制在 ~/.cargo/bin/mah
ls -la "$HOME/.cargo/bin/mah"
```

### 5. 把 `mah` 加到 `PATH`

`cargo install` 把二进制装到 `~/.cargo/bin/`,确保在 `PATH` 里:

```bash
# bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# zsh
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### 6. 验证

```bash
which mah           # 应该打印 ~/.cargo/bin/mah
mah version         # mah 0.1.0 (...)
mah --help          # 9 个子命令 (start / run / plugins / events / ...)
```

### 更新 / 卸载

```bash
# 从源码更新
cd ma-harness.rs && git pull && cargo build --release -p ma-harness-cli

# 从 crates.io 更新
cargo install ma-harness-cli --force

# 卸载
cargo uninstall ma-harness-cli
```

---

## macOS

测试环境: **macOS 14 Sonoma (Intel)**, **macOS 15 Sequoia (Apple Silicon M1/M2/M3/M4)**。
两个都能装, 命令一样。

### 1. 装 Xcode Command Line Tools

需要 `clang` + `git` + macOS SDK 头。

```bash
xcode-select --install
# 弹窗点 "Install", 接受 license, 等 ~5 分钟
xcode-select -p    # 应该打印 /Library/Developer/CommandLineTools
```

### 2. 装 Homebrew (可选, 推荐)

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Apple Silicon: 把 brew 加到 PATH (Intel 跳过)
echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
eval "$(/opt/homebrew/bin/brew shellenv)"

brew --version
```

### 3. 装 Rust 工具链

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

rustc --version
cargo --version
```

### 4. 装编译工具

macOS 自带 `clang` (步骤 1 装的), 只需要 `pkg-config` 和 OpenSSL
(只有编链接 openssl 的 crate 才需要)。

```bash
# Homebrew
brew install pkg-config openssl protobuf   # protobuf 可选
```

> Apple Silicon 上, Homebrew 装在 `/opt/homebrew/`。`pkg-config` 默认能找到 OpenSSL;
> 如果找不到, 设:
> `export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl/lib/pkgconfig"`

### 5. 装 `mah` CLI

#### 方案 A — 从源码

```bash
git clone https://github.com/ma-harness/ma-harness.rs.git
cd ma-harness.rs
cargo build --release -p ma-harness-cli
ls -la target/release/mah
```

共享 build 缓存:

```bash
export CARGO_TARGET_DIR="$HOME/.cache/cargo-target"
cargo build --release -p ma-harness-cli
```

#### 方案 B — 从 crates.io

```bash
cargo install ma-harness-cli
ls -la "$HOME/.cargo/bin/mah"
```

### 6. 把 `mah` 加到 `PATH`

```bash
# zsh (macOS 10.15+ 默认 shell)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc

# bash (老 Mac)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bash_profile
source ~/.bash_profile
```

### 7. 验证

```bash
which mah
mah version
mah --help
```

### 更新 / 卸载

```bash
# 从源码更新
cd ma-harness.rs && git pull && cargo build --release -p ma-harness-cli

# 从 crates.io 更新
cargo install ma-harness-cli --force

# 卸载
cargo uninstall ma-harness-cli
```

---

## Windows

测试环境: **Windows 10 (21H2+)**, **Windows 11**, **PowerShell 5.1 / 7.x**。
本节用 **PowerShell** 写; 如果你用 **WSL** (Ubuntu inside Windows),
请走 [Linux](#linux) 章节。

### 1. 装 Visual Studio Build Tools (C++ 工具链)

Windows 上的 Rust 需要 MSVC 链接器。装免费的 Build Tools。

1. 下载: <https://visualstudio.microsoft.com/visual-cpp-build-tools/>
2. 跑安装器。
3. **Workloads** 选项卡, 勾 **"Desktop development with C++"**。
4. 右侧确保选中:
   - MSVC v143 (或更新)
   - Windows 11 SDK (或 Windows 10 SDK)
   - C++ CMake tools for Windows
5. 点 **Install** (~3-5 GB, 10-20 分钟)。

> 如果你已经装了带 C++ 工作负载的 Visual Studio (任何版本), 跳过这步。

### 2. 装 Rust 工具链 (rustup-init.exe)

```powershell
# 用 PowerShell 拉 rustup 装器
Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile "$env:TEMP\rustup-init.exe"
& "$env:TEMP\rustup-init.exe" -y
# 重启终端, 然后:
rustc --version
cargo --version
```

> ARM64 Windows 上, 把 URL 的 `x86_64` 换成 `aarch64`。

### 3. 装 Git for Windows (没装的话)

```powershell
git --version
# 如果没装, 从 https://git-scm.com/download/win 装完重启终端
```

### 4. (可选) 装系统级 `protoc`

```powershell
# 方案 A: winget (Windows 11 / Windows 10 21H2+)
winget install --id=Google.Protobuf -e

# 方案 B: choco (用 Chocolatey 的话)
choco install protoc

# 方案 C: scoop
scoop install protobuf

# 验证
protoc --version
```

### 5. 装 `mah` CLI

#### 方案 A — 从源码

打开 **"x64 Native Tools Command Prompt for VS"** (或在当前 shell 跑
`vcvarsall.bat x64`),确保 MSVC 链接器在 `PATH` 上。然后:

```powershell
git clone https://github.com/ma-harness/ma-harness.rs.git
cd ma-harness.rs
cargo build --release -p ma-harness-cli
# 二进制在 target\release\mah.exe
Get-Item target\release\mah.exe
```

共享 build 缓存 (推荐 — 每个项目省 1-2 GB):

```powershell
$env:CARGO_TARGET_DIR = "D:\rust_target"
[Environment]::SetEnvironmentVariable("CARGO_TARGET_DIR", "D:\rust_target", "User")
cargo build --release -p ma-harness-cli
# 二进制在 D:\rust_target\release\mah.exe
```

> **Tip**: 用 **系统属性 → 环境变量** (用户变量) 永久设 `CARGO_TARGET_DIR`,
> 这样所有 shell 都能用。

#### 方案 B — 从 crates.io

```powershell
cargo install ma-harness-cli
# 二进制在 $env:USERPROFILE\.cargo\bin\mah.exe
Get-Item "$env:USERPROFILE\.cargo\bin\mah.exe"
```

### 6. 把 `mah` 加到 `PATH`

`cargo install` 把二进制装到 `%USERPROFILE%\.cargo\bin\`。确保在 `PATH` 里:

```powershell
# 查当前 PATH
$env:PATH -split ';' | Select-String '\.cargo'

# 如果没有, 当前 session 加
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# 永久加 (User PATH)
[Environment]::SetEnvironmentVariable(
    "PATH",
    "$env:USERPROFILE\.cargo\bin;$([Environment]::GetEnvironmentVariable('PATH','User'))",
    "User"
)
# 重启终端让新 PATH 生效
```

> PowerShell 7+ 有 `Add-Content $PROFILE '...'` 持久化 shell 配置;
> 改系统 PATH, 用上面的 `[Environment]::SetEnvironmentVariable`。

### 7. 验证

```powershell
Get-Command mah
# 期望: ...\.cargo\bin\mah.exe

mah version
# 期望: mah 0.1.0 (...)

mah --help
# 期望: 9 个子命令
```

### 更新 / 卸载

```powershell
# 从源码更新 (必须用 VS Developer Prompt, 或已 source vcvarsall)
cd ma-harness.rs
git pull
cargo build --release -p ma-harness-cli

# 从 crates.io 更新
cargo install ma-harness-cli --force

# 卸载
cargo uninstall ma-harness-cli
```

### WSL (另一种选择)

如果你更喜欢 Windows 里的 Linux:

```powershell
wsl --install -d Ubuntu-24.04
# 重启, 从开始菜单打开 "Ubuntu", 然后走 [Linux](#linux) 章节
```

> WSL 里, Linux 文件系统 (`/home/you/...`) 跑 Rust 比 Windows 盘
> (`/mnt/c/...`) 快很多。

---

## Python SDK (`mah-py`)

三个系统都能装。从 PyPI:

```bash
# Linux / macOS
pip install mah-py

# Windows (PowerShell)
pip install mah-py

# 验证
python -c "from mah_py import Mah; print('OK')"
```

国内网络慢的话:

```bash
pip install -i https://pypi.tuna.tsinghua.edu.cn/simple mah-py
```

完整 API 看 [crates/mah-py/README.md](../../../crates/mah-py/README.md)。

---

## Troubleshooting

### Linux: `error: linker 'cc' not found`

你跳了步骤 2 (编译工具)。装上:

```bash
sudo apt install -y build-essential      # Debian / Ubuntu
sudo dnf install -y gcc gcc-c++ make     # Fedora
sudo pacman -S --needed base-devel       # Arch
```

### macOS: `error: linker 'cc' not found`

你跳了步骤 1 (Xcode CLT):

```bash
xcode-select --install
```

### macOS: 编译时 OpenSSL 找不到

macOS 上某些 crate 需要 OpenSSL:

```bash
brew install openssl
export PKG_CONFIG_PATH="$(brew --prefix openssl)/lib/pkgconfig"
# Apple Silicon: export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl/lib/pkgconfig"
```

### Windows: `error: linker 'link.exe' not found`

你跳了步骤 1 (VS Build Tools)。两条路:

1. 装 **Visual Studio Build Tools**, 选 **C++ 工作负载** (推荐)。
2. 用 **x64 Native Tools Command Prompt for VS** (Build Tools 自带),
   它已经把 `link.exe` 加到 `PATH`。

### Windows: `protoc not found`

你跳了步骤 4。装上 (`winget install Google.Protobuf`), 或者用 vendored 二进制:

```powershell
$env:PROTOC = "$PWD\vendor\protoc\binary\protoc.exe"
cargo build --release -p ma-harness-cli
```

### 装完 `mah` 命令找不到

`cargo install` 把二进制装到 `~/.cargo/bin/` (Linux/macOS) 或
`%USERPROFILE%\.cargo\bin\` (Windows)。看上面系统对应章节的步骤 5/6
加到 `PATH`。

### `error: failed to download ...` (网络 / 代理)

在公司代理后面, 或者 crates.io / GitHub 慢的地区:

```powershell
# 用国内镜像 (rsproxy)
[Environment]::SetEnvironmentVariable("CARGO_REGISTRIES_CRATES_IO_PROTOCOL", "git", "User")
# 或编辑 ~/.cargo/config.toml:
@'
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy]
registry = "https://rsproxy.cn/crates.io-index"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
'@ | Out-File -Append -Encoding utf8 $env:USERPROFILE\.cargo\config.toml
```

### `disk full` 编译时

`target/` 目录能涨到 **2-5 GB**。清出空间, 或者用 `CARGO_TARGET_DIR` 移到
大点的盘。

## 下一步

- [02-quick-start.md](02-quick-start.md) — 跑你的第一个 agent (三个系统通用)
- [03-server.md](03-server.md) — 部署 `mah start` (含 Linux systemd + Windows service)
