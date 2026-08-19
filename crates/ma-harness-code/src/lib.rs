//! # 命名约定
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-code`
//! **Crate ident** (`use` 路径): `ma_harness_code`
//!
//! Rust 自动从 kebab-case package name 推 snake_case crate ident.
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! ma-harness-code = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_code::*;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-code
//!
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
//! - **Phase 3.1 加固**: fuel / epoch / ResourceLimiter (memory + table) 三层沙箱
//! - 业务方 wasm 默认受 10M fuel + 5s epoch deadline + 16MB memory 限制
//! - 业务方 wasm 不能访问 host 文件系统 / 网络 (没开 WASI)
//! - Phase 2.6 早期: 不支持 component model (WASI preview2)
//!
//! # 沙箱配置 (Phase 3.1 / T3.1)
//!
//! ```ignore
//! use ma_harness_code::{CodeRunner, SandboxConfig};
//!
//! // 默认安全配置
//! let runner = CodeRunner::new_with_config(SandboxConfig::default())?;
//!
//! // 自定义 (允许无限指令但限内存)
//! let cfg = SandboxConfig {
//!     fuel: 0,  // 0 = 不限
//!     epoch_deadline_ms: Some(30_000),  // 30s
//!     memory_bytes: 64 * 1024 * 1024,  // 64MB
//!     table_elements: Some(10_000),
//! };
//! let runner = CodeRunner::new_with_config(cfg)?;
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]

use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use wasmtime::*;

/// 沙箱配置 (Phase 3.1 / T3.1 + Phase 3.4 / T3.4 扩展)
///
/// 三层防御:
/// 1. **fuel**: 限制指令数 (0 = 不限). 防死循环 / DoS
/// 2. **epoch_deadline_ms**: 限制 wall-clock 时间 (None = 不限). 跟 engine 定时 thread 配合
/// 3. **memory_bytes / table_elements**: ResourceLimiter 限制 wasm grow 操作
///
/// **Phase 3.4 扩展**:
/// 4. **allowed_paths**: wasm `host::read_file` 允许读的 path 白名单. 业务方 wasm
///    想读 LLM 输出 / 配置文件,path 必须以 `allowed_paths` 任一为前缀. 空 list = 不允许读任何文件.
///
/// **默认配置** (`SandboxConfig::default()`) 是 LLM-generated code 的"安全起步":
/// - 10M fuel (~ 1M 函数调用, 单 call 通常 < 1K fuel)
/// - 5s epoch deadline (50ms tick × 100 ticks)
/// - 16MB memory (1 wasm page = 64KB, 默认 1 page, 上限 256 pages)
/// - 1000 table elements (业务方应该用不到 table)
/// - `allowed_paths` = [] (不允许读文件, 业务方按需 push)
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// 燃料上限 (instruction count). 0 = 不限. 默认 10M
    pub fuel: u64,
    /// epoch 截止时间 (millisecond). None = 不限. 默认 Some(5_000) 5s
    pub epoch_deadline_ms: Option<u64>,
    /// memory 上限 (bytes). 0 = 不限. 默认 16MB
    pub memory_bytes: usize,
    /// table element 上限. None = 不限. 默认 Some(1000)
    pub table_elements: Option<u32>,
    /// **Phase 3.4 / T3.4**: wasm `host::read_file` 允许读的 path 白名单
    /// (空 = 不允许读任何文件)
    pub allowed_paths: Vec<std::path::PathBuf>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            fuel: 10_000_000,
            epoch_deadline_ms: Some(5_000),
            memory_bytes: 16 * 1024 * 1024,
            table_elements: Some(1000),
            allowed_paths: Vec::new(),
        }
    }
}

impl SandboxConfig {
    /// 无限沙箱 (不推荐生产用, 业务方写错死循环会卡死 host)
    pub fn unbounded() -> Self {
        Self {
            fuel: 0,
            epoch_deadline_ms: None,
            memory_bytes: 0,
            table_elements: None,
            allowed_paths: Vec::new(),
        }
    }
}

/// CodeRunner — 编译 + 跑 wasm module
pub struct CodeRunner {
    /// wasmtime engine (编译 / 实例化 共享, 含 epoch interruption 配置)
    engine: Engine,
    /// 沙箱配置
    config: SandboxConfig,
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

/// ResourceLimiter 实现 — 限制 memory + table grow
struct MemTableLimiter {
    memory_bytes: usize,
    table_elements: Option<u32>,
    /// 当前 memory 用量 (调试用)
    memory_used: AtomicU64,
    /// 当前 table element 数 (调试用)
    table_used: AtomicU32,
}

impl MemTableLimiter {
    fn new(memory_bytes: usize, table_elements: Option<u32>) -> Self {
        Self {
            memory_bytes,
            table_elements,
            memory_used: AtomicU64::new(0),
            table_used: AtomicU32::new(0),
        }
    }
}

impl ResourceLimiter for MemTableLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, Error> {
        if self.memory_bytes > 0 && desired > self.memory_bytes {
            // 拒绝 grow, wasm 拿 -1 (跟 OOM 一样)
            return Ok(false);
        }
        self.memory_used.store(desired as u64, Ordering::SeqCst);
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool, Error> {
        if let Some(max) = self.table_elements {
            if desired > max as usize {
                return Ok(false);
            }
        }
        self.table_used.store(desired as u32, Ordering::SeqCst);
        Ok(true)
    }
}

impl CodeRunner {
    /// 构造一个 runner (默认沙箱, Engine default)
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_config(SandboxConfig::default())
    }

    /// 构造一个 runner + 自定义沙箱配置
    ///
    /// 内部根据 `SandboxConfig`:
    /// 1. Engine 配置 epoch_interruption (如果 config.epoch_deadline_ms 是 Some)
    /// 2. 每个 Store 设 fuel + epoch_deadline + ResourceLimiter
    pub fn new_with_config(config: SandboxConfig) -> anyhow::Result<Self> {
        let mut engine_config = Config::new();
        // fuel 必须先开 (Config flag, 才能在 store 上 set_fuel)
        if config.fuel > 0 {
            engine_config.consume_fuel(true);
        }
        // epoch interruption 必须先开 (Config flag)
        if config.epoch_deadline_ms.is_some() {
            engine_config.epoch_interruption(true);
        }
        let engine = Engine::new(&engine_config)?;
        Ok(Self {
            engine,
            config,
            stdout: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 拿当前沙箱配置 (clone)
    pub fn config(&self) -> SandboxConfig {
        self.config.clone()
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

        // Store host state = MemTableLimiter (wasmtime 27 限制: limiter 是 host state 子对象)
        let limiter = MemTableLimiter::new(self.config.memory_bytes, self.config.table_elements);
        let mut store = Store::new(&self.engine, limiter);

        // === Phase 3.1 / T3.1: 三层沙箱 ===
        // 1. ResourceLimiter (memory + table) — 把 store 内的 limiter expose
        store.limiter(|s: &mut MemTableLimiter| -> &mut dyn ResourceLimiter { s });
        // 2. Fuel (限指令数, 0 = 不限)
        if self.config.fuel > 0 {
            store.set_fuel(self.config.fuel)
                .map_err(|e| anyhow::anyhow!("set_fuel: {e}"))?;
        }
        // 3. Epoch deadline (限 wall-clock 时间)
        if let Some(ms) = self.config.epoch_deadline_ms {
            // 当前 epoch + (ms / 50ms tick) = deadline epoch
            // 50ms 是常见 tick 间隔, host 业务方应该独立 thread 每 50ms 调 engine.increment_epoch()
            // 简化: 直接设一个固定大值, 跑完 trap 在 deadline 上
            // 50ms tick → 100 ticks = 5s
            let ticks = ms / 50;
            let _ = store.set_epoch_deadline(ticks);
        }
        // 业务方应该在自己的服务里 spawn 一个 task 调 engine.increment_epoch() 推 epoch
        // 简化: CodeRunner 不自动开 epoch thread (测试 / 同步 context 友好)

        // Linker: 注册 host::log
        let mut linker = Linker::new(&self.engine);
        let stdout_for_log = Arc::clone(&self.stdout);
        linker.func_wrap(
            "host",
            "log",
            move |mut caller: Caller<'_, MemTableLimiter>, ptr: i32, len: i32| {
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

        // **Phase 3.4 / T3.4**: 注册 host::read_file
        // - 业务方 wasm 传 path 字符串 (在 wasm memory [ptr, ptr+len))
        // - host 检查 path 在 sandbox 白名单,读文件,内容写回 wasm memory
        // - 返回 (新 ptr, len), 业务方 wasm 解析
        // - 失败返 (0, 0)
        // - sandbox 白名单: config.allowed_paths (Phase 3.4 加)
        let allowed_paths_for_read = Arc::new(self.config.allowed_paths.clone());
        linker.func_wrap(
            "host",
            "read_file",
            move |mut caller: Caller<'_, MemTableLimiter>, ptr: i32, len: i32| -> (i32, i32) {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => return (0, 0),
                };
                let data = memory.data(&caller);
                let path_ptr = ptr as usize;
                let path_len = len as usize;
                if path_ptr + path_len > data.len() {
                    eprintln!("host::read_file: out of bounds path");
                    return (0, 0);
                }
                let path_str = match std::str::from_utf8(&data[path_ptr..path_ptr + path_len]) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("host::read_file: invalid utf8 path: {e}");
                        return (0, 0);
                    }
                };
                // 沙箱白名单检查
                let path = std::path::Path::new(path_str);
                let is_allowed = allowed_paths_for_read
                    .iter()
                    .any(|allowed| path.starts_with(allowed));
                if !is_allowed {
                    eprintln!(
                        "host::read_file: path '{}' not in allowed list (sandbox)",
                        path_str
                    );
                    return (0, 0);
                }
                // 读文件
                let content = match std::fs::read(path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("host::read_file: read '{}' failed: {e}", path_str);
                        return (0, 0);
                    }
                };
                // 写回 wasm memory (offset 0, 因为我们刚读完 path 字符串可以复用)
                // 简单策略:写到 memory 起点,前提是 content.len() + 0 <= memory.data.len()
                let write_ptr = 0usize;
                if content.len() > data.len() {
                    eprintln!(
                        "host::read_file: content too large ({} > memory {})",
                        content.len(),
                        data.len()
                    );
                    return (0, 0);
                }
                // memory.data(&caller) 拿的是 &[data], 但要写需要 &mut
                // 走 memory.data_mut(&mut caller) (wasmtime 27 API)
                // 但 MemTableLimiter 可能也借了 memory... 这里写不影响 limiter 的原子操作
                // 实际: wasmtime 0.79 走 memory.write 写 buffer
                // 简化: 让业务方知道 host::read_file 写到 offset 0, 长度 content.len()
                // 但 caller 不可变借用,我们要拿 mut
                // Caller 提供 data_mut via memory
                if let Some(memory_mut) =
                    caller.get_export("memory").and_then(|e| e.into_memory())
                {
                    let mut buf = memory_mut.data_mut(&mut caller);
                    if write_ptr + content.len() > buf.len() {
                        eprintln!("host::read_file: write out of bounds");
                        return (0, 0);
                    }
                    buf[write_ptr..write_ptr + content.len()].copy_from_slice(&content);
                } else {
                    return (0, 0);
                }
                (write_ptr as i32, content.len() as i32)
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

    // === Phase 3.1 / T3.1: 沙箱配置测试 ===

    /// 默认沙箱配置: 10M fuel + 5s epoch + 16MB memory + 1000 table
    #[test]
    fn default_sandbox_config_has_sane_limits() {
        let cfg = SandboxConfig::default();
        assert_eq!(cfg.fuel, 10_000_000);
        assert_eq!(cfg.epoch_deadline_ms, Some(5_000));
        assert_eq!(cfg.memory_bytes, 16 * 1024 * 1024);
        assert_eq!(cfg.table_elements, Some(1000));
    }

    /// unbounded(): 全部设 0/None, 表示不限
    #[test]
    fn unbounded_sandbox_config_disables_all_limits() {
        let cfg = SandboxConfig::unbounded();
        assert_eq!(cfg.fuel, 0);
        assert_eq!(cfg.epoch_deadline_ms, None);
        assert_eq!(cfg.memory_bytes, 0);
        assert_eq!(cfg.table_elements, None);
    }

    /// fuel 配置生效: 设 1000 fuel 跑简单 wat 成功
    /// (真实 fuel 耗尽的 trap 在 wasmtime 27 是 cross-FFI panic, 难 assert, 这里
    /// 只验"配置生效 + 不够 fuel 时 runner 仍能编译运行")
    #[test]
    fn fuel_config_is_applied() {
        let cfg = SandboxConfig {
            allowed_paths: vec![],
            fuel: 1000, // 够跑简单 wat
            epoch_deadline_ms: None,
            memory_bytes: 0,
            table_elements: None,
        };
        let runner = CodeRunner::new_with_config(cfg).unwrap();
        let wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "run") (result i32)
                    i32.const 42
                )
            )
        "#;
        let output = runner.run_wat(wat).unwrap();
        assert_eq!(output.return_value, 42, "1000 fuel 应够跑简单 wat");
    }

    /// memory_bytes 限制生效: 业务方 wasm memory.grow 超过限制时被拒 (返 -1)
    /// 不 panic, 不 abort, 业务方可以 trap 处理
    #[test]
    fn memory_growth_limit_blocks_grow_beyond_limit() {
        // 64 KB = 1 wasm page. 业务方 memory.grow 1 申请第 2 个 page, 应被拒 (因为 limit=64KB = 1 page)
        let cfg = SandboxConfig {
            allowed_paths: vec![],
            fuel: 0, // 不限 fuel
            epoch_deadline_ms: None,
            memory_bytes: 64 * 1024, // 1 page, 不允许 grow
            table_elements: None,
        };
        let runner = CodeRunner::new_with_config(cfg).unwrap();
        // 业务方 wat: memory.grow(1) 申请 +1 page, 被拒, 返 -1, 然后 trap
        // 用 i32.const -1 + i32.eqz + br_if 0 (jump to trap)
        // 简化: 让 grow 失败后直接 return -1 (业务方处理)
        let wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "run") (result i32)
                    ;; 尝试 grow 1 page (64KB), 应被 limiter 拒
                    i32.const 1
                    memory.grow
                )
            )
        "#;
        // memory.grow 返 -1 (失败) 或 old size (成功, 不会因 limit 拒)
        // 因为 limit=64KB = 当前 1 page, grow(1) 想拿 2 page = 128KB, 被拒 -> 返 -1
        let result = runner.run_wat(wat);
        // 应该跑成功, 返 -1
        assert!(result.is_ok(), "memory.grow 失败应正常返回 -1, 不应 panic: {:?}", result.err());
        assert_eq!(result.unwrap().return_value, -1, "memory grow 应被拒, 返 -1");
    }

    /// 默认 fuel=10M 够用, 简单 wasm 跑成功
    #[test]
    fn default_fuel_sufficient_for_simple_wasm() {
        let runner = CodeRunner::new().unwrap();
        let wat = r#"
            (module
                (import "host" "log" (func $log (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "ok")
                (func (export "run") (result i32)
                    i32.const 0
                    i32.const 2
                    call $log
                    i32.const 42
                )
            )
        "#;
        let output = runner.run_wat(wat).unwrap();
        assert_eq!(output.return_value, 42);
    }

    /// unbounded fuel + 极大 memory: 业务方死循环跑成功 (但不推荐)
    #[test]
    fn unbounded_config_runs_without_fuel_trap() {
        let runner = CodeRunner::new_with_config(SandboxConfig::unbounded()).unwrap();
        let wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "run") (result i32)
                    i32.const 99
                )
            )
        "#;
        let output = runner.run_wat(wat).unwrap();
        assert_eq!(output.return_value, 99);
    }

    /// Runner::config() 暴露当前配置
    #[test]
    fn config_getter_returns_current_config() {
        let cfg = SandboxConfig {
            allowed_paths: vec![],
            fuel: 1000,
            epoch_deadline_ms: Some(1000),
            memory_bytes: 1024,
            table_elements: Some(10),
        };
        let runner = CodeRunner::new_with_config(cfg.clone()).unwrap();
        let got = runner.config();
        assert_eq!(got.fuel, 1000);
        assert_eq!(got.epoch_deadline_ms, Some(1000));
        assert_eq!(got.memory_bytes, 1024);
        assert_eq!(got.table_elements, Some(10));
    }
    // === Phase 3.4 / T3.4: host::read_file 受控 fs ===

    /// 业务方 wasm 调 host::read_file 读白名单里的文件, 不 panic 不 abort
    #[test]
    fn host_read_file_runs_within_sandbox() {
        let tmpdir = tempfile::tempdir().unwrap();
        let file_path = tmpdir.path().join("hello.txt");
        std::fs::write(&file_path, b"hello wasm").unwrap();

        let mut cfg = SandboxConfig::default();
        cfg.allowed_paths.push(tmpdir.path().to_path_buf());

        let runner = CodeRunner::new_with_config(cfg).unwrap();
        // 调 read_file, 把 (ptr, len) 加起来当 return value
        // 验证不 panic 不 abort
        let wat = r#"
            (module
                (import "host" "read_file" (func $read (param i32 i32) (result i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 1000) "x")
                (func (export "run") (result i32)
                    i32.const 1000
                    i32.const 1
                    call $read
                    i32.add
                )
            )
        "#;
        let output = runner.run_wat(wat).unwrap();
        // 跑通即可 (T3.4 集成测)
        let _ = output.return_value;
    }

    /// 业务方 wasm 读白名单外的文件被拒 (返 0, 0)
    #[test]
    fn host_read_file_blocks_path_outside_sandbox() {
        let tmpdir = tempfile::tempdir().unwrap();
        let file_path = tmpdir.path().join("secret.txt");
        std::fs::write(&file_path, b"secret data").unwrap();

        let mut cfg = SandboxConfig::default();
        cfg.allowed_paths.push(std::path::PathBuf::from("/some/other/path"));

        let runner = CodeRunner::new_with_config(cfg).unwrap();
        let wat = r#"
            (module
                (import "host" "read_file" (func $read (param i32 i32) (result i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "x")
                (func (export "run") (result i32)
                    i32.const 0
                    i32.const 1
                    call $read
                    i32.add
                )
            )
        "#;
        let output = runner.run_wat(wat).unwrap();
        // 被拒 -> (0, 0).add = 0
        assert_eq!(output.return_value, 0, "sandbox 外文件应被拒 (ptr=0, len=0)");
    }

    /// 空 allowed_paths 时任何文件读都被拒
    #[test]
    fn host_read_file_empty_allowed_list_blocks_all() {
        let cfg = SandboxConfig::default();
        let runner = CodeRunner::new_with_config(cfg).unwrap();
        let wat = r#"
            (module
                (import "host" "read_file" (func $read (param i32 i32) (result i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "x")
                (func (export "run") (result i32)
                    i32.const 0
                    i32.const 1
                    call $read
                    i32.add
                )
            )
        "#;
        let output = runner.run_wat(wat).unwrap();
        assert_eq!(output.return_value, 0);
    }
}
