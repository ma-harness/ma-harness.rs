# 04 — 插件

> **目标**: 装 first-party 插件, 写自己的, 发布到公开 registry。
> 插件**源码** 100% 跨平台 Rust — OS 特定的部分是 **build 环境** 跟 **路径约定**
> (下面 [Build env & paths per OS](#build-env-paths-per-os) 一节讲)。

[English](../../en/user-guide/04-plugins.md) | [简体中文](04-plugins.md)

## 选你的系统 (build env & paths 章节用)

| 系统 | 跳到 |
|---|---|
| **Linux** | [Linux build env & paths](#linux) |
| **macOS** | [macOS build env & paths](#macos) |
| **Windows** | [Windows build env & paths](#windows) |

如果已经走过 [01-installation.md](01-installation.md) 跟 `cargo build` 能跑,
直接跳 [Step 1](#step-1-first-party-)。

## 前置条件 (三个系统通用)

- 装好 `mah` CLI (见 [01-installation.md](01-installation.md))
- Rust 工具链能跑 (`cargo build` 成功) — 还没装看下面
  [Build env & paths per OS](#build-env-paths-per-os)
- 一个要用的插件, 或者一个写插件的 Rust workspace
- 从头写一个插件 ~30 分钟

## 什么是插件?

插件是一个 Rust crate, 实现下面一个或多个 trait (定义在 `ma-harness-seam`):

| Trait | 作用 |
|---|---|
| `Service` | 长寿命状态 (DB 连接, 缓存等) |
| `Plugin` | 生命周期 hook (install / uninstall) |
| `Listener` | 响应事件 |
| `Disposable` | scope 退出时清理 |
| `Tool` | LLM 可调函数 (带 name, description, JSON schema) |

每个插件还在 context 里声明自己配置的 typed key。

## Step 1 — 安装 first-party 插件 (一次性)

6 个 first-party 插件跟 `mah` 一起分发:

| Plugin | 功能 | Typed keys |
|---|---|---|
| `bash` | 跑 shell 命令 | `MAX_RUNTIME_MS` |
| `fs` | 读/写/列文件 (沙箱) | `READ_ALLOW_LIST`, `WRITE_ALLOW_LIST` |
| `web` | HTTP GET/POST (URL 白名单) | `EGRESS_ALLOW_LIST`, `TIMEOUT_MS` |
| `subagent` | fork 子 agent | `MAX_DEPTH` |
| `skill` | 加载 `.skill/` 文件 | `SKILLS_DIR` |
| `cordis` | 反射 context (meta) | `INSPECT_DEPTH` |

激活, 只需要在 agent 代码里 import 它们:

```rust
use ma_harness_plugin_hello as _;  // 通过 inventory 自动注册
use ma_harness_plugin_bash as _;
use ma_harness_plugin_fs as _;
use ma_harness_plugin_web as _;
use ma_harness_plugin_subagent as _;
use ma_harness_plugin_skill as _;
use ma_harness_plugin_cordis as _;
```

或者运行时列:

```bash
mah plugins
# 期望:
# Registered plugins (7 total):
#   - hello
#   - bash
#   - fs
#   - web
#   - subagent
#   - skill
#   - cordis
```

## Step 2 — 在 agent 代码里配置 typed key

```rust
ctx.set(MAX_RUNTIME_MS, 30_000)        // 30 秒超时
    .set(READ_ALLOW_LIST, vec!["/tmp".to_string(), "/home/me/docs".to_string()])
    .set(EGRESS_ALLOW_LIST, vec!["https://api.example.com".to_string()])
    .set(MAX_DEPTH, 3);
```

插件每次调用时从 context 读这些 — 运行时改, 不用重启。

## Step 3 — 在 agent 里调用 tool

```rust
let bash = ctx.service::<BashService>().await?;
let result = bash.run_command(ctx, "ls -la /tmp").await?;
println!("{}", result);
```

---

## Build env & paths per OS

插件**源码**跨平台一样。差异是:

1. **Build 工具链** — `cargo`, linker, 原生依赖 (01 装 `mah` 的时候已经讲了;
   本节是快速提醒)
2. **路径约定** — registry 文件位置, home 目录, 斜杠方向

### Linux

#### Build env

```bash
# 跟装 `mah` 用的是同一套:
rustc --version
cargo --version
# 还要 C 链接器 (build-essential / dnf / pacman — 见 01)
```

#### 路径约定

| 啥 | Linux 路径 |
|---|---|
| User home | `$HOME` (~ `/home/<you>`) |
| Plugin registry | `~/.ma-harness/registry.json` |
| Plugin 源码布局 | `~/projects/my-plugin/...` (你定) |
| `Cargo.toml` workspace | `~/projects/ma-harness.rs/` |
| Build 用原生依赖 | `apt` / `dnf` / `pacman` 装 (e.g. `libssl-dev`) |

#### 交叉编译插件到别的 OS

```bash
# Linux 上编 Windows .exe (测试用):
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu -p my-plugin
# 二进制在 target/x86_64-pc-windows-gnu/release/<name>.exe
```

> Linux → Windows: 装 `mingw-w64` (`sudo apt install mingw-w64`) 提供 cross-linker。
> 反向 (Windows → Linux) 比较麻烦, 用 WSL。

### macOS

#### Build env

```bash
xcode-select -p   # /Library/Developer/CommandLineTools — 应该有
rustc --version
cargo --version
```

如果 `xcode-select` 不在, 跑 `xcode-select --install` (见 01)。

#### 路径约定

| 啥 | macOS 路径 |
|---|---|
| User home | `$HOME` (~ `/Users/<you>`) |
| Plugin registry | `~/.ma-harness/registry.json` |
| Plugin 源码布局 | `~/Developer/my-plugin/...` (Xcode 习惯) 或者哪都行 |
| `Cargo.toml` workspace | `~/Developer/ma-harness.rs/` |
| Build 用原生依赖 | `brew install pkg-config openssl` |

> Apple Silicon vs Intel: `~/.ma-harness/registry.json` 路径一样, 但
> Homebrew 装在 `/opt/homebrew` vs `/usr/local`。如果插件要 OpenSSL (Apple Silicon):
> `export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl/lib/pkgconfig"`

#### 交叉编译插件到别的 OS

```bash
# macOS → Linux (少见, 但静态二进制可走):
rustup target add x86_64-unknown-linux-musl
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-unknown-linux-musl -p my-plugin
```

> 插件交叉编译很少用 — 大多数团队在每个目标 OS 上 build。
> 只在 CI matrix 跟静态二进制发布时用。

### Windows

#### Build env

```powershell
rustc --version
cargo --version
# 还要 MSVC 链接器 (Visual Studio Build Tools) — 见 01
```

如果报 `error: linker 'link.exe' not found`, 打开
**"x64 Native Tools Command Prompt for VS"** 跑。

#### 路径约定

| 啥 | Windows 路径 |
|---|---|
| User home | `$env:USERPROFILE` (~ `C:\Users\<you>`) |
| Plugin registry | `$env:USERPROFILE\.ma-harness\registry.json` |
| Plugin 源码布局 | `C:\Users\<you>\source\my-plugin\...` (VS 习惯) |
| `Cargo.toml` workspace | `C:\Users\<you>\source\ma-harness.rs\` |
| Build 用原生依赖 | `vcpkg` 或 vcpkg/Conan 装 |

> **`Cargo.toml` 里用正斜杠** (`path = "../../crates/..."`)。
> Cargo 在 Windows 上会 normalize, 但 `/` 和 `\` 混用容易出 bug。

#### 交叉编译插件到别的 OS

```powershell
# Windows → Linux (CI 常见):
rustup target add x86_64-unknown-linux-gnu
# 然后用 Linux 容器 / WSL / Linux CI runner 跑。
# 没 Unix 工具链从 Windows cross 出来基本不实用。
```

> Windows 上开发插件最舒服是进 **WSL** — 走 [Linux](#linux) 章节。

---

## Step 4 — 写自己的插件

下面例子跨平台。crate 在任何有 Rust 工具链的 OS 上都能编。

建新 crate:

```bash
cargo new --lib plugins/my-plugin
```

`Cargo.toml`:

```toml
[dependencies]
ma-harness-cordis = { path = "../../crates/ma-harness-cordis" }
ma-harness-seam = { path = "../../crates/ma-harness-seam" }
ma-harness-plugin-macro = { path = "../../crates/ma-harness-plugin-macro" }
inventory = "0.3"
async-trait = "0.1"
```

`src/lib.rs`:

```rust
use ma_harness_cordis::{Context, Service, Plugin};
use ma_harness_seam::Tool;
use ma_harness_plugin_macro::{dsh_service, dsh_tool, ctx_key};

// 配置 typed key
ctx_key!(pub static MAX_ITEMS: usize = 100);

#[dsh_service]
pub struct MyService {
    // state fields
}

#[dsh_tool(
    name = "search_docs",
    description = "Search the local docs directory"
)]
impl MyService {
    pub async fn search_docs(&self, ctx: &Context, query: String) -> Result<String, String> {
        let max = *ctx.get(MAX_ITEMS).unwrap_or(&100);
        // ... 实现 ...
        Ok(format!("found {} results for {}", max, query))
    }
}

pub struct MyPlugin {
    service: MyService,
}

impl Plugin for MyPlugin {
    fn name(&self) -> &str { "my-plugin" }
    fn install(&self, ctx: &Context) {
        ctx.inject(self.service.clone());
    }
}

// 启动时自动注册
inventory::submit! {
    pub fn register() -> Box<dyn Plugin> {
        Box::new(MyPlugin { service: MyService::new() })
    }
}
```

Build + check (三个系统通用):

```bash
cargo build -p my-plugin
cargo test -p my-plugin        # 如果有 test
```

## Step 5 — 跑带新插件的 agent

在 agent binary 里:

```rust
use my_plugin as _;  // 触发 inventory::submit!
```

然后跑:

```bash
cargo run -p my-agent
mah plugins | grep my-plugin
# 期望: - my-plugin
```

## Step 6 — 发布到 registry

先写 `plugin.toml`:

```toml
[plugin]
name = "my-plugin"
version = "0.1.0"
description = "Searches local docs"
author = "your-name"
source = { type = "local", path = "../plugins/my-plugin" }
tags = ["search", "docs"]
```

本地发布 (写到 registry 文件 — 看下面 OS 路径表):

```bash
mah plugin publish plugin.toml
# Linux / macOS:  建/更新 ~/.ma-harness/registry.json
# Windows:        建/更新 %USERPROFILE%\.ma-harness\registry.json
```

导出给 GitHub Pages:

```bash
# 三个系统通用 (相对仓库根的路径)
mah registry export --output docs/registry/registry.json
```

Commit + push (Linux / macOS / Windows Git Bash):

```bash
git add docs/registry/registry.json
git commit -m "feat(registry): publish my-plugin @ 0.1.0"
git push origin main
```

或 Windows PowerShell:

```powershell
git add docs/registry/registry.json
git commit -m "feat(registry): publish my-plugin @ 0.1.0"
git push origin main
```

`registry-pages.yml` workflow (如果开了) ~2 分钟内部署到
<https://ma-harness.github.io/ma-harness.rs/registry/>。

## Step 7 — 从 registry 装 (其他用户)

发布后, 任何人都能:

```bash
# 列可用插件 (用当前 OS 的默认 registry 路径)
mah registry list

# 装
mah plugin install my-plugin@0.1.0
```

用自定义 registry 路径:

```bash
# Linux / macOS
mah plugin install my-plugin@0.1.0 --registry /path/to/registry.json

# Windows PowerShell
mah plugin install my-plugin@0.1.0 --registry C:\path\to\registry.json
```

## 验证

Step 5 之后:

```bash
mah plugins
# 期望: 列出 - my-plugin

mah run "search docs for async"
# 期望: agent 用 MyService::search_docs
```

Step 6 (publish) 之后:

```bash
# 在发布的 GH Pages 站 (任何 OS):
curl -s https://ma-harness.github.io/ma-harness.rs/registry/registry.json | jq '.plugins | keys'
# 期望: 包含 "my-plugin"
```

## 下一步

- **验证** 插件行为 — 见 [05-conformance.md](05-conformance.md)
- **部署** 到生产 — 见 [03-server.md](03-server.md)
- **排错** 常见问题 — 见 [06-troubleshooting.md](06-troubleshooting.md)

## 参考

- Plugin schema: [docs/en/plugin-schema-v1.md](../plugin-schema-v1.md)
- Macro 设计: [docs/en/macro-design.md](../macro-design.md)
- Registry workflow: [docs/en/operations/registry-pages.md](../operations/registry-pages.md)
- hello plugin 源码: [../../../plugins/ma-harness-plugin-hello/](../../../plugins/ma-harness-plugin-hello/)

## Troubleshooting

### 跑 agent 时报 "plugin not registered"

确保在 binary 里 import 插件 crate:

```rust
use my_plugin as _;
```

`as _` 关键 — 强制跑 `inventory::submit!`。

### "duplicate plugin name" error

两个插件同名 (`name()`)。改一个名, 或者用 feature flag 隔离:

```toml
[features]
default = []
my-plugin-b = []
```

### Tool 对 LLM 不可见

Tool schema 生成要 `Tool` trait。确保用 `#[dsh_tool]`:

```rust
#[dsh_tool(name = "...", description = "...")]
impl MyService { ... }
```

没这个宏, tool 作为 service 注册了但 LLM 看不到。

### `mah plugin publish` 报 "registry file not found"

默认 registry 路径还没建。两条路:

- 直接跑 `mah plugin publish` (第一次 publish 时自动建), 或
- 显式传 `--registry`:
  - Linux/macOS: `--registry ~/.ma-harness/registry.json`
  - Windows: `--registry $env:USERPROFILE\.ma-harness\registry.json`

### Windows: `Cargo.toml` 里 `path = "..\..\crates\..."` 不工作

Cargo 只接受正斜杠。用:

```toml
ma-harness-cordis = { path = "../../crates/ma-harness-cordis" }
```

### Linux: 编插件报 `error: linker 'cc' not found`

跟 install 一样 — 装 C 工具链:

```bash
sudo apt install -y build-essential     # Debian / Ubuntu
sudo dnf install -y gcc gcc-c++ make    # Fedora
sudo pacman -S --needed base-devel      # Arch
```

### macOS: 编插件 OpenSSL 找不到

```bash
brew install openssl
# Apple Silicon:
export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl/lib/pkgconfig"
# Intel:
export PKG_CONFIG_PATH="$(brew --prefix openssl)/lib/pkgconfig"
```

### Windows: 编插件 `link.exe not found`

打开 **"x64 Native Tools Command Prompt for VS"** (或在当前 PowerShell 跑
`vcvarsall.bat x64`) 让 MSVC 上 `PATH`, 然后重跑 `cargo build`。

没装 VS Build Tools, 走 [Windows install 步骤](01-installation.md#windows)。
