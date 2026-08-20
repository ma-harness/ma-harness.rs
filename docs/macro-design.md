# ma-harness.rs — Plugin Macro Design

[English](macro-design.md) | [简体中文](zh-CN/macro-design.md)

> **Purpose**: Lock down the signatures, behavior, constraints, and examples of
> the 5 proc-macros into an executable code contract.
> This doc was written when Week 1-2 hadn't started the
> `ma_harness_plugin_macro` crate; this design **is** its spec.
>
> **Naming convention**: project renamed to `ma-harness.rs`, but the
> **internal macro prefix keeps `dsh_`** (a tribute to DeepSeek Harness; see
> `docs/decision-log.md#1`).
>
> This doc lives in `docs/` rather than `crates/ma_harness_plugin_macro/`
> because it is the **design spec**. When Week 1-2 actually writes the macro
> implementation, this doc is the blueprint; deviations are allowed but must
> update this doc.

---

## 1. Overview

| Macro              | Form     | Purpose                                                                 | Complexity |
|--------------------|----------|-------------------------------------------------------------------------|------------|
| `#[dsh_service]`   | derive   | Add `ctx.inject()` capability to a struct                                | thin sugar |
| `#[dsh_listener]`  | derive   | Add `ctx.on(event, fn)` subscription capability to a struct              | thin sugar |
| `#[dsh_tool]`      | attribute | Register a model-callable tool; extract schema from function signature   | heavy     |
| `#[dsh_command]`   | attribute | Register a CLI/REPL command                                              | heavy     |
| `#[dsh_handler]`   | attribute | Register a model adapter (talks to LLM API)                              | heavy     |

> **derive vs attribute**:
> - **derive** on a `struct`, goal: "auto impl trait", saves boilerplate.
> - **attribute** on a `fn`, goal: "expand into full registration code +
>   extract schema".

---

## 2. `#[dsh_service]` — derive

### 2.1 Purpose

Let a struct be obtained via `ctx.inject::<MyService>()`, automatically
implementing the `Service` trait.

### 2.2 Signature

```rust
#[dsh_service]
pub struct MyService {
    field: String,
    // ...
}

impl MyService {
    pub fn new(ctx: &Context) -> Result<Self> {
        // User-written construction logic
        let field = ctx.get(SESSION_ID)?;
        Ok(Self { field })
    }

    pub fn do_thing(&self) -> String {
        // business method
    }
}
```

After expansion (pseudocode):

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
    // User-written do_thing untouched
}
```

### 2.3 Constraints

- Must implement `fn new(ctx: &Context) -> Result<Self>` (user writes it; the
  macro does not generate it).
- `ctx: &Context` is a constructor argument, not a field.
- Fields can be any type; the macro does not check.
- Default `Error = anyhow::Error`; to customize, add
  `#[dsh_service(error = MyError)]`.

### 2.4 Why it's thin sugar

Because the `Service` trait is already simple; the macro just saves the 6
lines of `impl Service for X { ... }` boilerplate.

**Users can also write it by hand**:

```rust
impl Service for MyService {
    type Ctx = Context;
    type Error = anyhow::Error;
    fn install(ctx: &Context) -> Result<Self> { Self::new(ctx) }
    fn name() -> &'static str { stringify!(MyService) }
}
```

Thin sugar, but those 6 lines are worth saving.

---

## 3. `#[dsh_listener]` — derive

### 3.1 Purpose

Let a struct subscribe to ctx events; expand into a registration set
of `ctx.on(Event::X, fn)`.

### 3.2 Signature

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

After expansion (pseudocode):

```rust
impl Listener for MyListener {
    fn register(ctx: &Context) -> Result<()> {
        ctx.on(Event::SessionStart, Self::on_session_start)?;
        ctx.on(Event::ToolCall, Self::on_tool_call)?;
        Ok(())
    }
}
```

### 3.3 Constraints

- `#[dsh_listener]` on the struct + `#[dsh_listener::on(Event::X)]` on
  individual functions, paired
- Function signature must be `async fn(&self, &Context, &EventType) -> Result<()>`
- Missing `&self` or changing to `&mut self` → compile error
- `EventType` must be a variant of the `ctx.event::Event` enum

### 3.4 Why derive instead of attribute

The derive on the struct "declares I have listener capability"; the attribute
on the function "declares I subscribe to which event"; **both are needed**.
This macro design is a **dual macro**: `#[dsh_listener]` is the derive,
`#[dsh_listener::on(...)]` is a helper attribute.

---

## 4. `#[dsh_tool]` — attribute (heavy)

### 4.1 Purpose

Register a Rust function as a model-callable tool. Function signature →
JSON Schema → fed to the LLM; when the LLM calls → deserialize parameters
→ call the function.

### 4.2 Signature

```rust
/// Description visible to the LLM, will be in the schema
#[dsh_tool]
async fn search_files(
    /// Search pattern, supports glob
    pattern: String,
    /// Search root directory, default current working directory
    #[dsh_arg(default = ".")]
    root: String,
    /// Whether to recurse
    #[dsh_arg(default = false)]
    recursive: bool,
) -> Result<Vec<String>> {
    // ...
    Ok(vec!["...".into()])
}
```

### 4.3 After expansion (pseudocode)

```rust
// 1. Keep the original function
async fn search_files(pattern: String, root: String, recursive: bool) -> Result<Vec<String>> { ... }

// 2. Generate the schema struct
pub fn search_files_schema() -> ToolSchema {
    ToolSchema {
        name: "search_files",
        description: "Description visible to the LLM, will be in the schema",  // from doc comment
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Search pattern, supports glob",
                },
                "root": {
                    "type": "string",
                    "description": "Search root directory, default current working directory",
                    "default": ".",
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Whether to recurse",
                    "default": false,
                },
            },
            "required": ["pattern"],  // fields without default are required
        }),
    }
}

// 3. Generate the invocation entry point
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

// 4. Generate the registration entry point (called when plugin loads)
pub fn search_files_register(registry: &mut ToolRegistry) {
    registry.register("search_files", search_files_schema(), search_files_invoke);
}
```

### 4.4 Supported parameter types

| Rust type                              | JSON Schema type          | Notes                                       |
|----------------------------------------|---------------------------|---------------------------------------------|
| `String`                               | `string`                  |                                             |
| `&str`                                 | `string`                  | (ownership issue; prefer `String`)          |
| `i32` / `i64` / `u32` / `u64`         | `integer`                 |                                             |
| `f32` / `f64`                          | `number`                  |                                             |
| `bool`                                 | `boolean`                 |                                             |
| `Vec<T>`                               | `array` (recursive T)     |                                             |
| `Option<T>`                            | nullable T                |                                             |
| custom struct (derive `JsonSchema`)    | `object`                  | via `schemars` 0.8                          |
| enum (derive `JsonSchema`)             | `string` + `enum`         |                                             |

### 4.5 Hard constraints

- Must be `async fn` (Week 1-2 limit; sync tools added in Phase 2)
- Must return `Result<T, E: Display>` (T auto-converts to JSON, E to string)
- Parameters must **all be named** (Rust function params already are; we
  emphasize it)
- First paragraph of the doc comment is the description
- Per-parameter doc comment is the field description
- Missing doc comment → compile warning (`#[deny(missing_docs)]`)

### 4.6 Soft constraints / recommendations

- Naming: snake_case function name, used as tool name automatically
- Description: Chinese or English, internally unified to Chinese (project style)
- Don't prefix tool name with plugin name (avoid pollution; e.g. don't
  `bash_run`, just `run_bash_command`)

---

## 5. `#[dsh_command]` — attribute (heavy)

### 5.1 Purpose

Register a CLI/REPL command (not for the LLM, but for the **human** to call).

### 5.2 Signature

```rust
/// Start a session
#[dsh_command(name = "start", about = "Start a new session")]
async fn cmd_start(
    /// session name
    #[arg(long, short)]
    name: String,
    /// model adapter name
    #[arg(long, default_value = "openai")]
    adapter: String,
    ctx: &Context,  // auto-injected, not in the schema
) -> Result<()> {
    // ...
}
```

### 5.3 Difference from `#[dsh_tool]`

| Dimension       | `#[dsh_tool]`             | `#[dsh_command]`                |
|-----------------|---------------------------|---------------------------------|
| Caller          | LLM                       | human (CLI)                     |
| Arg parsing     | JSON deserialize          | clap                            |
| Registry       | ToolRegistry              | CommandRegistry                 |
| Schema audience | model prompt              | `mah --help`                    |
| Dependency      | `serde_json`              | `clap` 4.x                      |

### 5.4 After expansion (pseudocode)

```rust
// 1. Keep the original function (drop ctx arg)
async fn cmd_start(name: String, adapter: String) -> Result<()> { ... }

// 2. Generate the clap Command
pub fn start_clap_cmd() -> clap::Command {
    clap::Command::new("start")
        .about("Start a new session")
        .arg(clap::Arg::new("name").long("name").short('n').required(true).help("session name"))
        .arg(clap::Arg::new("adapter").long("adapter").default_value("openai").help("model adapter name"))
}

// 3. Generate the dispatch entry point (accepts clap matches)
pub async fn start_dispatch(ctx: &Context, matches: &clap::ArgMatches) -> Result<()> {
    let name = matches.get_one::<String>("name").cloned().unwrap();
    let adapter = matches.get_one::<String>("adapter").cloned().unwrap();
    cmd_start(name, adapter).await
}
```

### 5.5 Constraints

- Last parameter `ctx: &Context` is auto-injected from the ctx pool; not in clap
- `#[arg(...)]` is clap's standard, passed through
- Must be `async fn` + `Result<()>`

---

## 6. `#[dsh_handler]` — attribute (heavy)

### 6.1 Purpose

Register a model adapter (handler that talks to an LLM API). Phase 1 ships
only one built-in OpenAI-compatible, but the trait must be extensible.

### 6.2 Signature

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

### 6.3 Difference from `#[dsh_tool]` / `#[dsh_command]`

| Dimension       | `#[dsh_handler]`           | `#[dsh_tool]`             | `#[dsh_command]`        |
|-----------------|----------------------------|---------------------------|-------------------------|
| Caller          | model loop                 | LLM                       | human                   |
| Input           | `ModelRequest` (strong)    | JSON (model output)       | clap matches            |
| Output          | `ModelResponse` (strong)   | JSON (back to LLM)        | `Result<()>`            |
| Registry       | `AdapterRegistry`          | `ToolRegistry`            | `CommandRegistry`       |
| Quantity        | one per adapter            | one per tool              | one per command         |

### 6.4 Constraints

- Function signature `async fn(ModelRequest, &Context) -> Result<ModelResponse>` is fixed
- `adapter = "..."` is required; used as the registry key
- `endpoint` defaults to reading from env `MA_HARNESS_ADAPTER_<NAME>_ENDPOINT`
- Internally uses reqwest / tonic HTTP; errors are wrapped with `anyhow!`

### 6.5 Phase 2 extensions

- Streaming responses (`async_stream` / `futures::Stream`)
- Multiple model protocols (Anthropic / internal)
- Model selection strategy

---

## 7. Macro implementation notes (for the Week 1-2 author)

### 7.1 Crate layout

```
crates/ma_harness_plugin_macro/        ← proc-macro crate
├── src/
│   ├── lib.rs                ← re-export 5 macros
│   ├── service.rs            ← #[dsh_service] derive
│   ├── listener.rs           ← #[dsh_listener] + #[dsh_listener::on]
│   ├── tool.rs               ← #[dsh_tool] attribute
│   ├── command.rs            ← #[dsh_command] attribute
│   └── handler.rs            ← #[dsh_handler] attribute
└── Cargo.toml                ← proc-macro = true
```

### 7.2 Dependencies

```toml
[dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
schemars = "0.8"     # generate JSON Schema for tool
serde_json = "1"
```

### 7.3 Compile-time checks (each macro should have these)

- [ ] function signature is valid
- [ ] required fields all present
- [ ] snake_case / naming convention (per arch-map)
- [ ] doc comments complete
- [ ] type is in the supported table (for `#[dsh_tool]` param types)
- [ ] ctx / Result / async triple is present

### 7.4 Error message quality

Each macro error uses `compile_error!` to emit a **span-anchored, readable
error**, not `expected TokenTree`.

Good error:

```
error: #[dsh_tool] parameter `recursive` of type `bool` is not supported
       Supported types: String, integer, number, boolean, Vec<T>, Option<T>
  --> src/lib.rs:42:5
   |
42 |     recursive: HashMap<String, String>,
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Bad error:

```
error: unexpected token
  --> src/lib.rs:42:5
   |
42 |     recursive: HashMap<String, String>,
   |     ^
```

---

## 8. Example: hello-world plugin

> ⚠️ Real plugin authors **should not** directly `use ma_harness_cordis::*`
> (locked as an internal crate on 2026-08-18).
> Below is the **internal view** that shows how the macros actually work.
> The **plugin author view** uses
> `ma_harness_seam::{Plugin, Service, Listener, ToolRegistry}` through the
> seam abstraction layer.

```rust
// plugins/ma_harness_plugin_hello/src/lib.rs (internal-view example)

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

/// Tool visible to the model: greet someone
#[dsh_tool]
async fn greet(
    /// Name of the person to greet
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

## 9. Things we don't do (avoiding temptation)

| Want to do                                       | Why not                                                 |
|--------------------------------------------------|---------------------------------------------------------|
| Auto-inject ctx field for `#[dsh_service]`       | User writes `new(ctx)` themselves; clearer              |
| `#[dsh_tool]` allow custom schema override       | Wait until someone asks in Phase 2                      |
| `#[dsh_command]` support subcommands             | clap 4.x already does; adding a wrapper isn't worth it |
| `#[dsh_handler]` multi-endpoint routing         | Phase 2                                                 |
| `priority` parameter for listener                | Phase 2; Phase 1 order is registration order            |

---

## 10. Changelog

| Date       | Change |
|------------|--------|
| 2026-08-18 | Initial version: 5 proc-macro signatures + expansion pseudocode + constraints + examples |
| 2026-08-20 | P11+ updates: Anthropic handler added in P11-5; vision handler in P11-5/9; dual macro design for `dsh_listener` / `dsh_listener::on` in actual implementation |
