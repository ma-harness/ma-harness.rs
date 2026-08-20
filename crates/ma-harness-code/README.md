# ma-harness-code

> Run LLM-generated WebAssembly (WAT/WASM) in a 4-layer sandbox.


[English](README.md) | [简体中文](README.zh-CN.md)


> Run LLM-generated WebAssembly (WAT/WASM) in a 4-layer sandbox.

Part of the [ma-harness](https://gitee.com/yifenma/ma-harness.rs) AI agent orchestrator. Code Mode lets a model generate code at runtime, which is then compiled and executed in a `wasmtime` sandbox.

## Features

- **4-layer sandbox defense**:
  1. **Fuel** — limit instruction count (default 10M)
  2. **Epoch** — limit wall-clock time (default 5s)
  3. **Memory + Table** — `ResourceLimiter` constrains wasm grow (default 16MB + 1000)
  4. **Filesystem** — host `read_file` with `allowed_paths` whitelist (default empty = no reads)
- **Host imports**: `host::log` (stdout) + `host::read_file` (sandboxed file read)
- **WAT + WASM** support — accepts `.wat` text or pre-compiled `.wasm` bytes
- **Simple API** — `CodeRunner::new()` then `runner.run_wat(text)` or `runner.run_wasm(bytes)`

## Quick example

```rust
use ma_harness_code::{CodeRunner, SandboxConfig};

let runner = CodeRunner::new_with_config(SandboxConfig::default())?;

let wat = r#"
    (module
        (import "host" "log" (func $log (param i32 i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "hello from wasm")
        (func (export "run") (result i32)
            i32.const 0
            i32.const 15
            call $log
            i32.const 42
        )
    )
"#;

let output = runner.run_wat(wat)?;
assert_eq!(output.stdout_lines, vec!["hello from wasm"]);
assert_eq!(output.return_value, 42);
```

## CLI usage (in ma-harness workspace)

```bash
# Run a .wat file
mah code run examples/hello.wat

# End-to-end: business prompt -> LLM generates .wat -> run in sandbox
OPENAI_API_KEY=sk-... mah run-prompt "compute 1+1, return result as i32"
```

## Custom sandbox

```rust
use ma_harness_code::SandboxConfig;

let cfg = SandboxConfig {
    fuel: 0,                          // 0 = unlimited
    epoch_deadline_ms: Some(30_000),  // 30s
    memory_bytes: 64 * 1024 * 1024,  // 64MB
    table_elements: Some(10_000),
    allowed_paths: vec![std::path::PathBuf::from("/var/llm-output")],
};
```

## Stability

Currently `0.1.0`. Sandbox API is stable; host imports may grow in future minor versions (additive).

## Documentation

- [API docs (docs.rs)](https://docs.rs/ma-harness-code)
- [ma-harness architecture](https://gitee.com/yifenma/ma-harness.rs)

## License

MIT OR Apache-2.0
