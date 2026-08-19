//! ma_harness_code — Code Mode (Phase 2 / T3.1)
//!
//! LLM-generated code 跑在 wasmtime 沙箱里, 跟 host 通过预定义 host functions 通信.
//! 业务方拿一段 WAT 文本 (`.wat` 文件 / inline string), CodeRunner 编译 + 实例化 + 调 export.
//!
//! # 设计
//!
//! - **Engine**: wasmtime 27 `Engine::default()` (cranelift codegen, 单 Engine 可复用)
//! - **Store**: 每个 run 一个 `Store<()>` (host state 简单, 不需要 typed)
//! - **Host imports** (业务方 wasm module 可见):
//!   - `host::log(ptr: i32, len: i32)` — 业务方在 wasm 里写一行到 host stdout
//!   - `host::read_template(name_ptr, name_len) -> (i32, i32)` — 拿 ctx 里的 typed key 字符串
//!
//! # 用法
//!
//! ```ignore
//! use ma_harness_code::CodeRunner;
//!
//! let runner = CodeRunner::new()?;
//! let wat = r#"
//!     (module
//!         (import "host" "log" (func $log (param i32 i32)))
//!         (memory (export "memory") 1)
//!         (data (i32.const 0) "hello from wasm\n")
//!         (func (export "run")
//!             i32.const 0
//!             i32.const 16
//!             call $log
//!         )
//!     )
//! "#;
//! let output = runner.run_wat(wat)?;
//! assert_eq!(output.stdout_lines, vec!["hello from wasm".to_string()]);
//! ```
//!
//! # 限制
//!
//! - Phase 2.6 PoC: 不支持 WASI, 不支持 fuel 限制, 不支持 component model
//! - 只支持 core wasm 1.0 + simple host import
//! - 业务方 wasm 不能访问 host 文件系统 / 网络 (没开 WASI)
//! - 后续 (Phase 3): 加 fuel 限制 + epoch interruption 防 DoS

#![deny(unsafe_code)]
#![warn(missing_docs)]

use parking_lot::Mutex;
use std::sync::Arc;
use wasmtime::*;

/// CodeRunner — 编译 + 跑 wasm module
pub struct CodeRunner {
    /// wasmtime engine (编译 / 实例化 共享)
    engine: Engine,
    /// 业务方在 wasm 里调 `host::log` 时, host 这边 append 到这里
    stdout: Arc<Mutex<Vec<String>>>,
}

/// 跑一次的输出
#[derive(Debug, Clone, Default)]
pub struct CodeOutput {
    /// 业务方调 `host::log` 的所有调用, 按行 (用 `\n` split)
    pub stdout_lines: Vec<String>,
    /// export "run" 函数的返回值 (i32, 业务方自定义)
    pub return_value: i32,
}

impl CodeOutput {
    /// 拿 stdout 整体 (按 `\n` join)
    pub fn stdout(&self) -> String {
        self.stdout_lines.join("\n")
    }
}

impl CodeRunner {
    /// 构造一个 runner (内部建 Engine, 一次)
    pub fn new() -> anyhow::Result<Self> {
        let engine = Engine::default();
        Ok(Self {
            engine,
            stdout: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 跑 WAT 文本
    pub fn run_wat(&self, wat_text: &str) -> anyhow::Result<CodeOutput> {
        let bytes = wat::parse_str(wat_text)
            .map_err(|e| anyhow::anyhow!("parse WAT: {e}"))?;
        self.run_wasm(&bytes)
    }

    /// 跑 预编译 wasm bytes
    pub fn run_wasm(&self, bytes: &[u8]) -> anyhow::Result<CodeOutput> {
        // 清空 stdout (跑多次, 上次结果应消失)
        self.stdout.lock().clear();

        let module = Module::new(&self.engine, bytes)
            .map_err(|e| anyhow::anyhow!("compile module: {e}"))?;

        // Store<()> — host state 简单, stdout 走 Arc<Mutex> 共享
        let mut store = Store::new(&self.engine, ());

        // Linker: 注册 host::log
        let mut linker = Linker::new(&self.engine);
        let stdout_for_log = Arc::clone(&self.stdout);
        linker.func_wrap(
            "host",
            "log",
            move |mut caller: Caller<'_, ()>, ptr: i32, len: i32| {
                // 拿 wasm 内存, 读 [ptr, ptr+len) 字节, 转 UTF-8
                let memory = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .expect("wasm module must export 'memory' for host::log");
                let data = memory.data(&caller);
                let ptr = ptr as usize;
                let len = len as usize;
                if ptr + len > data.len() {
                    eprintln!("host::log: out of bounds ptr={ptr} len={len}");
                    return;
                }
                let bytes = &data[ptr..ptr + len];
                let s = String::from_utf8_lossy(bytes).to_string();
                stdout_for_log.lock().push(s);
            },
        )?;

        // 实例化
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| anyhow::anyhow!("instantiate: {e}"))?;

        // 调 export "run" — 没有 arg, 返回 i32
        let run = instance
            .get_typed_func::<(), i32>(&mut store, "run")
            .map_err(|e| anyhow::anyhow!("get export 'run': {e}"))?;
        let return_value = run
            .call(&mut store, ())
            .map_err(|e| anyhow::anyhow!("call 'run': {e}"))?;

        Ok(CodeOutput {
            stdout_lines: self.stdout.lock().clone(),
            return_value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_wat_with_host_log() {
        let runner = CodeRunner::new().unwrap();
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
        let output = runner.run_wat(wat).unwrap();
        assert_eq!(output.stdout_lines, vec!["hello from wasm"]);
        assert_eq!(output.return_value, 42);
    }

    #[test]
    fn run_wat_with_multiple_logs() {
        let runner = CodeRunner::new().unwrap();
        let wat = r#"
            (module
                (import "host" "log" (func $log (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "line1\nline2")
                (func (export "run") (result i32)
                    i32.const 0
                    i32.const 5
                    call $log
                    i32.const 6
                    i32.const 5
                    call $log
                    i32.const 0
                )
            )
        "#;
        let output = runner.run_wat(wat).unwrap();
        assert_eq!(output.stdout_lines, vec!["line1", "line2"]);
        assert_eq!(output.return_value, 0);
    }

    #[test]
    fn run_wat_compute_and_log() {
        // 业务方 wasm: log 字符串 "12" (业务方把数字转字符串写到内存, 然后 log)
        let runner = CodeRunner::new().unwrap();
        let wat = r#"
            (module
                (import "host" "log" (func $log (param i32 i32)))
                (memory (export "memory") 1)
                ;; 内存: "12" 在 offset 100 (data 只放 2 byte, 不带 padding 0)
                (data (i32.const 100) "12")
                (func (export "run") (result i32)
                    ;; 算 5 + 7
                    i32.const 5
                    i32.const 7
                    i32.add
                    drop
                    ;; log "12"
                    i32.const 100
                    i32.const 2
                    call $log
                    i32.const 0
                )
            )
        "#;
        let output = runner.run_wat(wat).unwrap();
        assert_eq!(output.stdout_lines, vec!["12"]);
        assert_eq!(output.return_value, 0);
    }

    #[test]
    fn run_wat_invalid_fails() {
        let runner = CodeRunner::new().unwrap();
        // parse 失败 (语法错)
        let bad = "(module (func (export \"run\") (result i32) i32.const 1";
        let result = runner.run_wat(bad);
        assert!(result.is_err(), "invalid WAT should fail");
    }

    #[test]
    fn run_wat_no_run_export_fails() {
        let runner = CodeRunner::new().unwrap();
        // 没有 (export "run")
        let wat = r#"
            (module
                (memory (export "memory") 1)
            )
        "#;
        let result = runner.run_wat(wat);
        assert!(result.is_err(), "module without 'run' should fail");
    }

    #[test]
    fn run_wasm_bytes_directly() {
        // 跳过 WAT, 直接喂 .wasm bytes (用 wat::parse_str 先编, 拿 bytes)
        let runner = CodeRunner::new().unwrap();
        let bytes = wat::parse_str(
            r#"
            (module
                (import "host" "log" (func $log (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "raw wasm")
                (func (export "run") (result i32)
                    i32.const 0
                    i32.const 8
                    call $log
                    i32.const 7
                )
            )
            "#,
        )
        .unwrap();
        let output = runner.run_wasm(&bytes).unwrap();
        assert_eq!(output.stdout_lines, vec!["raw wasm"]);
        assert_eq!(output.return_value, 7);
    }

    #[test]
    fn run_twice_clears_stdout() {
        // 同一个 runner 跑两次, stdout 应被清空
        let runner = CodeRunner::new().unwrap();
        let wat = r#"
            (module
                (import "host" "log" (func $log (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "first")
                (func (export "run") (result i32)
                    i32.const 0
                    i32.const 5
                    call $log
                    i32.const 0
                )
            )
        "#;
        runner.run_wat(wat).unwrap();
        let wat2 = r#"
            (module
                (import "host" "log" (func $log (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "second")
                (func (export "run") (result i32)
                    i32.const 0
                    i32.const 6
                    call $log
                    i32.const 0
                )
            )
        "#;
        let out2 = runner.run_wat(wat2).unwrap();
        assert_eq!(out2.stdout_lines, vec!["second"], "stdout 应被清空, 不会带 first");
    }
}
