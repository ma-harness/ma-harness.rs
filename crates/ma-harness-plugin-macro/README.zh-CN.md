# ma-harness-plugin-macro (中文 / 简体中文)

[English](README.md) | [简体中文](README.zh-CN.md)

> **备注**: 本 crate 是 [ma-harness](https://gitee.com/yifenma/ma-harness.rs) workspace 的一部分. 插件作者通常不需要直接依赖本 crate — `ma-harness-seam` 通过 `ma_harness_seam::dsh_service`, `ma_harness_seam::dsh_listener` 等 re-export 相关 derive.

为 [ma-harness](https://gitee.com/yifenma/ma-harness.rs) 插件框架提供的过程宏.

## 特性

- **`#[derive(DshService)]`** — 为你的 service struct 自动实现 `ma_harness_cordis::Service`
- **`#[derive(DshListener)]`** — 为你的 listener struct 自动实现 `ma_harness_cordis::Listener<E>`
- **`#[derive(DshTool)]`** — 声明带 JSON Schema 参数的可调用工具
- **`#[derive(DshCommand)]`** — 声明 CLI 子命令
- **`#[derive(DshHandler)]`** — 声明类型化 event handler
- **`#[dsh_service_dual(name, ctor)]`** — 一个 attribute 同时生成 cordis + seam `Service` 实现
- **`#[dsh_plugin_dual(name, install)]`** — 同时生成 cordis + seam `Plugin` 实现
- **`#[dsh_listener_priority(priority = N)]`** — 给 listener struct 附加 priority 常量
- **`ctx_key!(name)`** — 类型化 key 宏, 编译期 snake_case 校验

## 快速示例

```rust
use ma_harness_seam::{DshService, Service, Context};
use ma_harness_plugin_macro::dsh_service_dual;

#[derive(DshService)]
#[dsh_service_dual(name = "hello", ctor = "create")]
pub struct HelloService {
    name: String,
}

impl HelloService {
    pub fn create() -> Self {
        Self { name: "world".to_string() }
    }

    pub fn greet(&self) -> String {
        format!("Hello, {}!", self.name)
    }
}

// 现在两种方式都可用:
//   ctx.service::<dyn HelloService>()        // 走 cordis
//   ctx.service::<dyn ma_harness_seam::Service>()  // 走 seam (opaque)
```

## 稳定性

本 crate 当前 `0.1.0`. Attribute 语法可能会演进, 因为我们在学习插件作者实际需要什么; 如果不得不破坏调用点, 我们会 bump major 版本.

## 文档

- [API docs (docs.rs)](https://docs.rs/ma-harness-plugin-macro)
- [ma-harness 架构](https://gitee.com/yifenma/ma-harness.rs)
- [Macro 设计文档](https://gitee.com/yifenma/ma-harness.rs/blob/main/docs/macro-design.md)

## 许可证

MIT OR Apache-2.0
