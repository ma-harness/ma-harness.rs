# ma-harness.rs — Plugin Macro 设计

[English](macro-design.md) | [简体中文](macro-design.zh-CN.md)

> 目的: 5 个 proc-macro 的签名、行为、约束、例子,落到可执行的代码契约。
> 写这个文档时,Week 1-2 还没起 `ma_harness_plugin_macro` crate,本设计就是它的 spec。
>
> 命名约定: 项目改名 `ma-harness.rs`,但**内部宏前缀保留 `dsh_`** (致敬 DeepSeek Harness,见 `docs/decision-log.md#1`)。
>
> 本文档写在 `docs/` 而不是 `crates/ma_harness_plugin_macro/`,因为它是**设计 spec**,
> 等 Week 1-2 真正写 macro 实现时,以本文档为蓝本,允许偏离但必须更新本文档。

---

## 1. 总览

| Macro | 形态 | 作用 | 复杂度 |
|---|---|---|---|
| `#[dsh_service]` | derive | 给 struct 加 ctx.inject() 能力 | 薄糖 |
| `#[dsh_listener]` | derive | 给 struct 加 ctx.on(event, fn) 订阅能力 | 薄糖 |
| `#[dsh_tool]` | attribute | 注册一个 model-callable 工具,从函数签名提取 schema | 重头 |
| `#[dsh_command]` | attribute | 注册一个 CLI/REPL 可调指令 | 重头 |
| `#[dsh_handler]` | attribute | 注册一个 model adapter (接 LLM API) | 重头 |

> **derive vs attribute 区分**:
> - **derive** 加在 `struct` 上,目的是"自动 impl trait",省 boilerplate。
> - **attribute** 加在 `fn` 上,目的是"展开成完整的注册代码 + 提取 schema"。

---

## 2. `#[dsh_service]` — derive

### 2.1 作用

让一个 struct 通过 `ctx.inject::<MyService>()` 拿到实例,自动实现 `Service` trait。

### 2.2 签名

```rust
#[dsh_service]
pub struct MyService {
    field: String,
    // ...
}

impl MyService {
    pub fn new(ctx: &Context) -> Result<Self> {
        // 用户写的构造逻辑
        let field = ctx.get(SESSION_ID)?;
        Ok(Self { field })
    }

    pub fn do_thing(&self) -> String {
        // 业务方法
    }
}
```

展开后(伪代码):

```rust
impl Service for MyService {
    type Ctx = Context;
    type Error = anyhow::Error;

    fn install(ctx: &Context) -> Result<Self> {
        Self::new(ctx)
    }

    fn name() -> &'static str { "MyService" }
}

impl MyService {
    // 用户写的 do_thing 不动
}
```

### 2.3 约束

- 必须实现 `fn new(ctx: &Context) -> Result<Self>` (用户自己写, macro 不生成)
- `ctx: &Context` 是构造参数,不是字段
- 字段可以是任意类型,macro 不检查
- 默认 `Error = anyhow::Error`,如果想自定义,加 `#[dsh_service(error = MyError)]`

### 2.4 为什么是薄糖

因为 `Service` trait 已经很简单,macro 只是省 `impl Service for X { ... }` 这 6 行 boilerplate。

**用户也可以手写**:

```rust
impl Service for MyService {
    type Ctx = Context;
    type Error = anyhow::Error;
    fn install(ctx: &Context) -> Result<Self> { Self::new(ctx) }
    fn name() -> &'static str { stringify!(MyService) }
}
```

薄糖,但 6 行省下来,值得。

---

## 3. `#[dsh_listener]` — derive

### 3.1 作用

让 struct 能订阅 ctx 事件,展开成 `ctx.on(Event::X, fn)` 的注册集合。

### 3.2 签名

```rust
#[dsh_listener]
pub struct MyListener;

#[dsh_listener::on(Event::SessionStart)]
async fn on_session_start(&self, ctx: &Context, ev: &SessionStartEvent) -> Result<()> {
    // ...
}

#[dsh_listener::on(Event::ToolCall)]
async fn on_tool_call(&self, ctx: &Context, ev: &ToolCallEvent) -> Result<()> {
    // ...
}
```

展开后(伪代码):

```rust
impl Listener for MyListener {
    fn register(ctx: &Context) -> Result<()> {
        ctx.on(Event::SessionStart, Self::on_session_start)?;
        ctx.on(Event::ToolCall, Self::on_tool_call)?;
        Ok(())
    }
}
```

### 3.3 约束

- struct 上 `#[dsh_listener]` + impl 内 `#[dsh_listener::on(Event::X)]` 配对
- 函数签名必须是 `async fn(&self, &Context, &EventType) -> Result<()>`
- 漏写 `&self` 或改 `&mut self` → 编译错误
- `EventType` 必须是 `ctx.event::Event` 枚举的 variant

### 3.4 为什么是 derive 不是 attribute

derive 在 struct 上"声明我有 listener 能力",attribute 在 fn 上"声明我订阅哪个 event",**两个都要**。这里 macro 设计是**双重宏**:`#[dsh_listener]` 是 derive,`#[dsh_listener::on(...)]` 是 helper attribute。

---

## 4. `#[dsh_tool]` — attribute (重头)

### 4.1 作用

把一个 Rust 函数注册成 model-callable 工具。函数签名 → JSON Schema → 喂给 LLM,LLM 调用时 → 反序列化参数 → 调函数。

### 4.2 签名

```rust
/// 给 LLM 看的工具描述,会进 schema
#[dsh_tool]
async fn search_files(
    /// 搜索模式,支持 glob
    pattern: String,
    /// 搜索根目录,默认当前工作目录
    #[dsh_arg(default = ".")]
    root: String,
    /// 是否递归
    #[dsh_arg(default = false)]
    recursive: bool,
) -> Result<Vec<String>> {
    // ...
    Ok(vec!["...".into()])
}
```

### 4.3 展开后 (伪代码)

```rust
// 1. 保留原函数
async fn search_files(pattern: String, root: String, recursive: bool) -> Result<Vec<String>> { ... }

// 2. 生成 schema 结构
pub fn search_files_schema() -> ToolSchema {
    ToolSchema {
        name: "search_files",
        description: "给 LLM 看的工具描述,会进 schema",  // 来自 doc comment
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "搜索模式,支持 glob",
                },
                "root": {
                    "type": "string",
                    "description": "搜索根目录,默认当前工作目录",
                    "default": ".",
                },
                "recursive": {
                    "type": "boolean",
                    "description": "是否递归",
                    "default": false,
                },
            },
            "required": ["pattern"],  // 没有 default 的字段是 required
        }),
    }
}

// 3. 生成调用入口
pub async fn search_files_invoke(args: serde_json::Value) -> Result<serde_json::Value> {
    let pattern: String = serde_json::from_value(args["pattern"].clone())?;
    let root: String = args.get("root")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_else(|| ".".to_string());
    let recursive: bool = args.get("recursive")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or(false);
    let result = search_files(pattern, root, recursive).await?;
    Ok(serde_json::to_value(result)?)
}

// 4. 生成注册入口 (供 plugin 装载时调用)
pub fn search_files_register(registry: &mut ToolRegistry) {
    registry.register("search_files", search_files_schema(), search_files_invoke);
}
```

### 4.4 参数支持类型

| Rust 类型 | JSON Schema 类型 | 备注 |
|---|---|---|
| `String` | `string` | |
| `&str` | `string` | (但 ownership 问题,推荐 `String`) |
| `i32` / `i64` / `u32` / `u64` | `integer` | |
| `f32` / `f64` | `number` | |
| `bool` | `boolean` | |
| `Vec<T>` | `array` (T 的 schema 递归) | |
| `Option<T>` | nullable T | |
| 自定义 struct (derive `JsonSchema`) | `object` | 通过 `schemars` 0.8 |
| enum (derive `JsonSchema`) | `string` + `enum` | |

### 4.5 约束 (硬性)

- 必须 `async fn` (Week 1-2 限定,Phase 2 加 sync 工具)
- 必须返回 `Result<T, E: Display>` (T 自动转 JSON,E 转 string)
- 参数必须**全部有名** (Rust 函数参数本来就有,这里只是强调)
- doc comment 第一段是 description
- 参数 doc comment 是该字段 description
- 漏写 doc comment → 编译警告 (`#[deny(missing_docs)]`)

### 4.6 约束 (软性 / 推荐)

- 命名:snake_case 函数名,自动作为 tool name
- description:中文或英文都行,内部统一中文 (项目风格)
- 工具名不要带 plugin 前缀 (避免污染,例如不要 `bash_run` 直接叫 `run_bash_command`)

---

## 5. `#[dsh_command]` — attribute (重头)

### 5.1 作用

注册一个 CLI/REPL 指令(不是给 LLM 的,是给**人**调的)。

### 5.2 签名

```rust
/// 启动一个 session
#[dsh_command(name = "start", about = "启动一个新会话")]
async fn cmd_start(
    /// session 名字
    #[arg(long, short)]
    name: String,
    /// model adapter 名
    #[arg(long, default_value = "openai")]
    adapter: String,
    ctx: &Context,  // 自动注入,不进 schema
) -> Result<()> {
    // ...
}
```

### 5.3 跟 `#[dsh_tool]` 区别

| 维度 | `#[dsh_tool]` | `#[dsh_command]` |
|---|---|---|
| 调用者 | LLM | 人 (CLI) |
| 参数解析 | JSON 反序列化 | clap |
| 注册位置 | ToolRegistry | CommandRegistry |
| schema 给谁看 | model prompt | `mah --help` |
| 依赖 | `serde_json` | `clap` 4.x |

### 5.4 展开后 (伪代码)

```rust
// 1. 保留原函数 (去掉 ctx 参数)
async fn cmd_start(name: String, adapter: String) -> Result<()> { ... }

// 2. 生成 clap Command
pub fn start_clap_cmd() -> clap::Command {
    clap::Command::new("start")
        .about("启动一个 session")
        .arg(clap::Arg::new("name").long("name").short('n').required(true).help("session 名字"))
        .arg(clap::Arg::new("adapter").long("adapter").default_value("openai").help("model adapter 名"))
}

// 3. 生成调用入口 (接 clap matches)
pub async fn start_dispatch(ctx: &Context, matches: &clap::ArgMatches) -> Result<()> {
    let name = matches.get_one::<String>("name").cloned().unwrap();
    let adapter = matches.get_one::<String>("adapter").cloned().unwrap();
    cmd_start(name, adapter).await
}
```

### 5.5 约束

- 最后一个参数 `ctx: &Context` 自动从 ctx pool 取,不出现在 clap 里
- `#[arg(...)]` 是 clap 标准,直接透传
- 必须 `async fn` + `Result<()>`

---

## 6. `#[dsh_handler]` — attribute (重头)

### 6.1 作用

注册一个 model adapter (接 LLM API 的处理函数)。Phase 1 只有一个内置 OpenAI-compatible,但 trait 要可扩展。

### 6.2 签名

```rust
/// OpenAI Chat Completions adapter
#[dsh_handler(adapter = "openai", endpoint = "https://api.openai.com/v1")]
pub async fn openai_handler(
    req: ModelRequest,
    ctx: &Context,
) -> Result<ModelResponse> {
    let client = reqwest::Client::new();
    let api_key = ctx.get(OPENAI_API_KEY)?;

    let resp = client.post(format!("{}/chat/completions", "https://api.openai.com/v1"))
        .bearer_auth(api_key)
        .json(&req.to_openai_format())
        .send()
        .await?;

    let body: OpenAIResponse = resp.json().await?;
    Ok(ModelResponse::from_openai(body))
}
```

### 6.3 跟 `#[dsh_tool]` / `#[dsh_command]` 区别

| 维度 | `#[dsh_handler]` | `#[dsh_tool]` | `#[dsh_command]` |
|---|---|---|---|
| 调用者 | model loop | LLM | 人 |
| 输入 | `ModelRequest` (强类型) | JSON (model 输出) | clap matches |
| 输出 | `ModelResponse` (强类型) | JSON (给 LLM 回传) | `Result<()>` |
| 注册位置 | `AdapterRegistry` | `ToolRegistry` | `CommandRegistry` |
| 数量 | 每个 adapter 一个 | 每个 tool 一个 | 每个 cmd 一个 |

### 6.4 约束

- 函数签名 `async fn(ModelRequest, &Context) -> Result<ModelResponse>` 固定
- `adapter = "..."` 是必填,作为注册 key
- `endpoint` 默认从 env `MA_HARNESS_ADAPTER_<NAME>_ENDPOINT` 读
- 内部实现 reqwest / tonic HTTP,错误用 `anyhow!` 包

### 6.5 Phase 2 扩展

- 流式响应 (`async_stream` / `futures::Stream`)
- 多 model 协议 (Anthropic / 内部)
- 模型选路策略

---

## 7. Macro 实现要点 (给 Week 1-2 写 macro 的自己看)

### 7.1 crate 划分

```
crates/ma_harness_plugin_macro/        ← proc-macro crate
├── src/
│   ├── lib.rs                ← re-export 5 个 macro
│   ├── service.rs            ← #[dsh_service] derive
│   ├── listener.rs           ← #[dsh_listener] + #[dsh_listener::on]
│   ├── tool.rs               ← #[dsh_tool] attribute
│   ├── command.rs            ← #[dsh_command] attribute
│   └── handler.rs            ← #[dsh_handler] attribute
└── Cargo.toml                ← proc-macro = true
```

### 7.2 依赖

```toml
[dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
schemars = "0.8"     # 给 tool 生成 JSON Schema
serde_json = "1"
```

### 7.3 编译期检查清单 (每个 macro 都要)

- [ ] 函数签名合法
- [ ] 必填字段都有
- [ ] snake_case / 命名规范 (per arch-map)
- [ ] doc comment 完整
- [ ] 类型在支持表里 (`#[dsh_tool]` 参数类型)
- [ ] ctx / Result / async 三件套齐

### 7.4 错误信息友好度

每个 macro 出错时,`compile_error!` 给出**带 span 的可读错误**,不要 `expected TokenTree`。

错误信息例子 (好):

```
error: #[dsh_tool] 参数 `recursive` 类型 `bool` 不支持
       支持的类型: String, integer, number, boolean, Vec<T>, Option<T>
  --> src/lib.rs:42:5
   |
42 |     recursive: HashMap<String, String>,
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

错误信息例子 (差):

```
error: unexpected token
  --> src/lib.rs:42:5
   |
42 |     recursive: HashMap<String, String>,
   |     ^
```

---

## 8. 例子:hello-world plugin

> ⚠️ 真实插件作者**不应该**直接 use `ma_harness_cordis::*` (2026-08-18 锁定为内部 crate)。
> 下面是**内部视角**的写法,展示 macro 实际工作。**插件作者视角**用
> `ma_harness_seam::{Plugin, Service, Listener, ToolRegistry}` 走 seam 抽象层。

```rust
// plugins/ma_harness_plugin_hello/src/lib.rs (内部视角示例)

use ma_harness_cordis::{Context, Service, Plugin};
use ma_harness_plugin_macro::{dsh_service, dsh_tool};

#[dsh_service]
pub struct HelloService {
    greeting: String,
}

impl HelloService {
    pub fn new(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(Self { greeting: "hello".into() })
    }
}

/// 给 model 看的工具:向某人打招呼
#[dsh_tool]
async fn greet(
    /// 要打招呼的人的名字
    who: String,
) -> anyhow::Result<String> {
    Ok(format!("Hello, {}!", who))
}

pub struct HelloPlugin;

impl Plugin for HelloPlugin {
    fn install(ctx: &Context) -> anyhow::Result<()> {
        ctx.inject::<HelloService>();
        greet_register(ctx.tool_registry_mut());
        Ok(())
    }

    fn name() -> &'static str { "hello" }
}
```

---

## 9. 不做的 (避免诱惑)

| 想做的 | 不做 |
|---|---|
| 给 `#[dsh_service]` 加自动 ctx 注入 field | 用户自己 new(ctx) 写,清晰 |
| `#[dsh_tool]` 支持自定义 schema 改写 | 等 Phase 2 有人提出再加 |
| `#[dsh_command]` 支持子命令 (subcommand) | clap 4.x 自己支持,加 wrapper 复杂度不值 |
| `#[dsh_handler]` 多个 endpoint 选路 | Phase 2 |
| 给 listener 加 `priority` 参数 | Phase 2,Phase 1 顺序就是注册顺序 |

---

## 10. 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-18 | 初版,5 个 proc-macro 签名 + 展开伪代码 + 约束 + 例子 |
| 2026-08-20 | P11+ 更新: P11-5 加 Anthropic handler; P11-5/9 加 vision handler; 实际实现 `dsh_listener` / `dsh_listener::on` 双重宏设计 |
