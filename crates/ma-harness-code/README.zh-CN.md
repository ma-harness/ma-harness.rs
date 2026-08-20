# ma-harness-code (中文 / 简体中文)

[English](README.md) | [简体中文](README.zh-CN.md)

> 在 4 层沙箱中运行 LLM 生成的 WebAssembly (WAT/WASM).

[ma-harness](https://gitee.com/yifenma/ma-harness.rs) AI agent 编排器的一部分. Code Mode 让 model 在运行时生成代码, 然后编译并在 `wasmtime` 沙箱中执行.

## 特性

- **4 层沙箱防御**:
  1. **Fuel** — 限制指令数 (默认 10M)
  2. **Epoch** — 限制 wall-clock 时间 (默认 5s)
  3. **Memory + Table** — `ResourceLimiter` 限制 wasm grow (默认 16MB + 1000)
  4. **Filesystem** — 宿主 `read_file` 带 `allowed_paths` 白名单 (默认空 = 不允许读)
- **Host imports**: `host::log` (stdout) + `host::read_file` (沙箱化文件读)
- **WAT + WASM** 支持 — 接收 `.wat` 文本或预编译的 `.wasm` bytes
- **简洁 API** — `CodeRunner::new()` 然后 `runner.run_wat(text)` 或 `runner.run_wasm(bytes)`

## 快速示例

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

## CLI 用法 (在 ma-harness workspace 中)

```bash
# 跑 .wat 文件
mah code run examples/hello.wat

# 端到端: 业务 prompt -> LLM 生成 .wat -> 在沙箱中跑
OPENAI_API_KEY=sk-... mah run-prompt "compute 1+1, return result as i32"
```

## 自定义沙箱

```rust
use ma_harness_code::SandboxConfig;

let cfg = SandboxConfig {
    fuel: 0,                          // 0 = 无限
    epoch_deadline_ms: Some(30_000),  // 30s
    memory_bytes: 64 * 1024 * 1024,  // 64MB
    table_elements: Some(10_000),
    allowed_paths: vec![std::path::PathBuf::from("/var/llm-output")],
};
```

## 稳定性

当前 `0.1.0`. 沙箱 API 稳定; host imports 未来 minor 版本可能增加 (additive).

## 文档

- [API docs (docs.rs)](https://docs.rs/ma-harness-code)
- [ma-harness 架构](https://gitee.com/yifenma/ma-harness.rs)

## 许可证

MIT OR Apache-2.0
