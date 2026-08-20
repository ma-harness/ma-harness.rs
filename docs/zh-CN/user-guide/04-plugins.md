# 04 — 插件

> **目标**: 装 first-party 插件,自己写一个,publish 到公开 registry。

[English](04-plugins.md) | [简体中文](04-plugins.md)

## 前置条件

- 装好 `mah` CLI (见 [01-installation.md](01-installation.md))
- 想用的插件,或写插件用的 Rust workspace
- 从零写 ~30 分钟

## 插件是什么

插件是实现一个或多个 trait (在 `ma-harness-seam` 里定义) 的 Rust crate:

| Trait | 作用 |
|---|---|
| `Service` | 长期状态 (数据库连接,缓存等) |
| `Plugin` | 生命周期 hook (install / uninstall) |
| `Listener` | 响应事件 |
| `Disposable` | scope 退出时清理 |
| `Tool` | LLM 可调函数 (带 name / description / JSON schema) |

每个插件还在 ctx 里声明 typed key 存配置。

## 步骤

### 第 1 步 — 装 first-party 插件 (一次)

6 个 first-party 插件跟 `mah` 一起发布:

| 插件 | 做什么 | Typed keys |
|---|---|---|
| `bash` | 跑 shell 命令 | `MAX_RUNTIME_MS` |
| `fs` | 读写列文件 (sandbox) | `READ_ALLOW_LIST`, `WRITE_ALLOW_LIST` |
| `web` | HTTP GET/POST (URL 白名单) | `EGRESS_ALLOW_LIST`, `TIMEOUT_MS` |
| `subagent` | 起子 agent | `MAX_DEPTH` |
| `skill` | 加载 `.skill/` 文件 | `SKILLS_DIR` |
| `cordis` | 反射 ctx (meta) | `INSPECT_DEPTH` |

激活,只需在 agent 代码里 import:

```rust
use ma_harness_plugin_hello as _;  // 通过 inventory 自动注册
use ma_harness_plugin_bash as _;
use ma_harness_plugin_fs as _;
use ma_harness_plugin_web as _;
use ma_harness_plugin_subagent as _;
use ma_harness_plugin_skill as _;
use ma_harness_plugin_cordis as _;
```

或运行时列:

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

### 第 2 步 — 在 agent 代码里配 typed key

```rust
ctx.set(MAX_RUNTIME_MS, 30_000)        // 30 秒超时
    .set(READ_ALLOW_LIST, vec!["/tmp".to_string(), "/home/me/docs".to_string()])
    .set(EGRESS_ALLOW_LIST, vec!["https://api.example.com".to_string()])
    .set(MAX_DEPTH, 3);
```

插件每次调用都从 ctx 读 — 运行时改,不用重启。

### 第 3 步 — 从 agent 调 tool

```rust
let bash = ctx.service::<BashService>().await?;
let result = bash.run_command(ctx, "ls -la /tmp").await?;
println!("{}", result);
```

### 第 4 步 — 写自己的插件

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

### 第 5 步 — 用新插件跑 agent

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

### 第 6 步 — Publish 到 registry

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

本地 publish:

```bash
mah plugin publish plugin.toml
# 创建/更新 ~/.ma-harness/registry.json
```

导出给 GitHub Pages:

```bash
mah registry export --output docs/registry/registry.json
```

commit + push:

```bash
git add docs/registry/registry.json
git commit -m "feat(registry): publish my-plugin @ 0.1.0"
git push origin main
```

`registry-pages.yml` workflow (如果启用) 会在 ~2 分钟内部署到
<https://ma-harness.github.io/ma-harness.rs/registry/>。

### 第 7 步 — 从 registry 装 (其他用户)

发布后,任何人都可以:

```bash
# 列可用插件
mah registry list

# 装
mah plugin install my-plugin@0.1.0
```

## 验证

第 5 步后:

```bash
mah plugins
# 期望: - my-plugin 在列表里

mah run "search docs for async"
# 期望: agent 用 MyService::search_docs
```

第 6 步后 (publish):

```bash
# 在发布的 GH Pages 站点:
curl -s https://ma-harness.github.io/ma-harness.rs/registry/registry.json | jq '.plugins | keys'
# 期望: 含 "my-plugin"
```

## 下一步

- **验证** 插件行为用 conformance 测试 — 见 [05-conformance.md](05-conformance.md)
- **部署** 到生产 server — 见 [03-server.md](03-server.md)
- **Troubleshoot** 常见插件问题 — 见 [06-troubleshooting.md](06-troubleshooting.md)

## 参考

- 插件 schema: [docs/zh-CN/plugin-schema-v1.md](../plugin-schema-v1.md)
- 宏设计: [docs/zh-CN/macro-design.md](../macro-design.md)
- Registry workflow: [docs/zh-CN/operations/registry-pages.md](../operations/registry-pages.md)
- hello 插件源码: [../../../plugins/ma-harness-plugin-hello/](../../../plugins/ma-harness-plugin-hello/)

## Troubleshooting

### 跑 agent 时 "plugin not registered"

确保在 binary 里 import 插件 crate:

```rust
use my_plugin as _;  // ← 这触发 inventory::submit!
```

`as _` 必填 — 它强制跑 crate 的 `inventory::submit!`。

### "duplicate plugin name" 错误

两个插件同名。要么改名 (在 `impl Plugin for MyPlugin { fn name() -> &str { ... } }`),
要么用 feature flag 关掉一个:

```toml
[features]
default = []
my-plugin-b = []
```

### Tool 对 LLM 不可见

Tool schema 生成需要 `Tool` trait。确保用 `#[dsh_tool]`:

```rust
// ✅ 这是 tool — LLM 可见
#[dsh_tool(name = "search", description = "Search docs")]
impl MyService { pub async fn search(&self, ...) -> ... { } }

// ❌ 只是 service method
impl MyService { pub async fn search(&self, ...) -> ... { } }
```
