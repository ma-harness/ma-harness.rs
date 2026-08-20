# ma-harness-seam (中文 / 简体中文)

[English](README.md) | [简体中文](README.zh-CN.md)

> **推荐给插件作者使用的公开 API.** 其他 `ma-harness-*` crate (cordis, plugin-macro, core) 是内部构建块; 插件代码应该只依赖本 crate, 以免受内部 refactor 影响.

为 [ma-harness](https://gitee.com/yifenma/ma-harness.rs) AI agent 编排器提供的稳定插件 API facade.

## 特性

- **Trait 隔离表面** — `Service`, `Plugin`, `Listener`, `AsyncListener`, `Disposable`, `AsyncDisposable`, `Tool`, `Command`, `Handler`. 全部 `#[non_exhaustive]`, 我们能加方法不破坏你的代码.
- **Re-exported derives** — `DshService`, `DshListener`, `DshTool`, `DshCommand`, `DshHandler` (来自 `ma-harness-plugin-macro`)
- **Re-exported macros** — `ctx_key!`, `dsh_service_dual!`, `dsh_plugin_dual!`, `dsh_listener_priority!`
- **PluginLoader** — `list()` / `load_by_name(ctx, name)` / `load_all(ctx)` (Kahn 拓扑序)
- **PluginRegistry** — 类型化插件注册, name → `Plugin` 工厂
- **PluginEntry / PluginManifest** — `inventory::submit!` 从 binary 中任意 crate 注册
- **Helper re-exports** — `Context`, `CtxKey`, `EventType`, `ModelAdapter` 等, 插件只需要一个 `use`

## 快速示例

```rust
use ma_harness_seam::{Context, DshService, DshListener, Plugin, Service, dsh_service_dual, dsh_listener};
use ma_harness_core::EventType;

// 1. 定义 service
#[derive(DshService)]
#[dsh_service_dual(name = "greet", ctor = "create")]
pub struct GreetService { name: String }

impl GreetService {
    pub fn create() -> Self { Self { name: "world".into() } }
    pub fn greet(&self) -> String { format!("Hello, {}!", self.name) }
}

// 2. 定义 listener
#[derive(DshListener)]
pub struct CountSessions;

// 3. 跑
let ctx = Context::new();
let svc = GreetService::create();
ctx.emit(ma_harness_core::SessionEvent::new("demo", EventType::SessionStart));
```

## 插件加载 (inventory)

```rust
use ma_harness_seam::{PluginLoader, inventory};

// 在你的 plugin crate:
inventory::submit!(ma_harness_seam::PluginEntry::new("greet", || Box::new(GreetService::create())));
inventory::submit!(ma_harness_seam::PluginManifest::new("greet", &[]));  // 无依赖

// 在你的 app:
let ctx = Context::new();
let names = PluginLoader::list();           // ["greet", ...]
PluginLoader::load_by_name(&ctx, "greet")?;  // 安装
PluginLoader::load_all(&ctx)?;               // 拓扑序
```

## 稳定性

本 crate 当前 `0.1.0`. Trait 表面是 `#[non_exhaustive]` — 允许添加方法不触发 major 版本 bump. 删除 / 重命名方法需要 `0.2.0`.

## 文档

- [API docs (docs.rs)](https://docs.rs/ma-harness-seam)
- [ma-harness 架构](https://gitee.com/yifenma/ma-harness.rs)
- [插件作者指南](https://gitee.com/yifenma/ma-harness.rs/blob/main/docs/plugin-authoring.md)

## 许可证

MIT OR Apache-2.0
