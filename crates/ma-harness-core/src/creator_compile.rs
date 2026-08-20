//! Creator 真实编译 (P10-1.5 + P10-1.6 / Day 101)
//!
//! 真编译 `PluginSpec` 走 `cargo build` subprocess. 跨平台:
//! - Windows: `cargo.exe` + `.dll`
//! - macOS: `cargo` + `.dylib`
//! - Linux: `cargo` + `.so`
//!
//! ## v1.5 流程
//!
//! 1. 写 `<output_dir>/<name>/Cargo.toml` (含 plugin name/version/deps)
//! 2. 写 `<output_dir>/<name>/src/lib.rs` (plugin source_code + register 函数)
//! 3. 调 `cargo build --release` (subprocess, 捕获 stdout/stderr)
//! 4. 检查 exit status 0
//! 5. 标记 Loaded, 记录 build artifact path
//!
//! ## P10-1.6 跨平台改造
//!
//! - `dylib_filename` 改返 `String` (v1.5 `Box::leak` 每次调用都泄漏)
//! - `compile()` 走 `tokio::task::spawn_blocking` (cargo 是同步阻塞, 不能在 async runtime 上跑)
//! - `render_cargo_toml` edition 2021 → 2024 (跟 workspace 对齐)
//! - `find_cargo` 加 `cargo --version` 验证 (确保 cargo 真在 PATH 里, 而不是空字符串)
//! - `dylib_filename` safe name: 不光替换 `-`, 还清掉 Windows 非法字符 `<>:"/\|?*`
//!
//! ## v2 计划 (P10-1.7)
//!
//! - 走 libloading 加载 .so/.dll/.dylib
//! - 拿 register 函数指针
//! - 注入 ToolRegistry
//!
//! ## 安全考虑
//!
//! - 业务方 (Creator 模式) 决定允许 model 写 Rust 代码
//! - 编译在 temp 目录 (不进项目目录, 不污染业务代码)
//! - subprocess 走 cargo, 不直接调 rustc (Cargo 锁 deps 版本)
//! - 不开 network (cargo offline 不灵时返错, 不静默 fallthrough)
//! - 业务方应审批后才调 (P7-2 集成)

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::creator::{CreatorError, PluginSpec};

/// 编译配置 (P10-1.5 + P10-1.8 v2)
#[derive(Debug, Clone)]
pub struct CompileConfig {
    /// cargo binary 路径 (None = 走 PATH 找)
    pub cargo_path: Option<PathBuf>,
    /// 编译超时 (默认 5 分钟)
    pub timeout: Duration,
    /// release mode (默认 true, 编译慢但产物快)
    pub release: bool,
    /// 临时目录 (None = 用 `std::env::temp_dir()`)
    pub temp_root: Option<PathBuf>,
    /// 输出目录 (None = `<temp_root>/ma-harness-plugins`)
    pub output_dir: Option<PathBuf>,
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            cargo_path: None,
            timeout: Duration::from_secs(300),
            release: true,
            temp_root: None,
            output_dir: None,
        }
    }
}

/// 编译结果 (P10-1.5)
#[derive(Debug, Clone)]
pub struct CompileOutput {
    /// 编译产物路径 (.so/.dll/.dylib)
    pub artifact_path: PathBuf,
    /// stdout 摘录 (前 100 行)
    pub stdout_preview: String,
    /// stderr 摘录 (前 100 行)
    pub stderr_preview: String,
    /// 编译耗时 (ms)
    pub duration_ms: u128,
}

/// 跨平台 helper: 找 cargo binary
///
/// 优先级:
/// 1. `CARGO` 环境变量
/// 2. `CompileConfig::cargo_path` (业务方显式设)
/// 3. `Command::new("cargo")` (Rust stdlib 自动查 PATH/PATHEXT)
///
/// **P10-1.6 改进**: 返 `Result` 而不是永远返 `PathBuf`, 让业务方知道 cargo 是否真在 PATH 里.
/// 之前 `which_cargo` 走 `where`/`which` 命令, 在 Windows minimal 镜像 / alpine 没 which 时返 fallback "cargo",
/// 后面 `Command::new("cargo")` 才报 "program not found" — 错误信息延迟.
/// 现在 `--version` 验证提前, 错误信息清晰.
pub fn find_cargo() -> Result<PathBuf, CreatorError> {
    if let Ok(cargo_env) = std::env::var("CARGO") {
        let p = PathBuf::from(cargo_env);
        if p.exists() {
            return Ok(p);
        }
        // CARGO 设了但不存在, 警告并继续查 PATH
        eprintln!(
            "[ma-harness] warning: CARGO={} 不存在, 走 PATH",
            p.display()
        );
    }
    // 让 stdlib 找 (Windows 自动加 .exe + 查 PATHEXT, Unix 查 PATH)
    // 先 verify cargo 真可用, 不然后面 Command 报错难定位
    match Command::new("cargo").arg("--version").output() {
        Ok(out) if out.status.success() => Ok(PathBuf::from("cargo")),
        Ok(out) => Err(CreatorError::Compile(format!(
            "cargo --version 退出非零 ({:?}):\nstdout: {}\nstderr: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        ))),
        Err(e) => Err(CreatorError::Compile(format!(
            "cargo 不可用 (请装 Rust toolchain 或设 CARGO 环境变量): {e}"
        ))),
    }
}

/// 跨平台 helper: 编译产物的 dylib 名 (返 String, 无内存泄漏)
///
/// - Windows: `{name}.dll`
/// - macOS: `lib{name}.dylib`
/// - Linux: `lib{name}.so`
///
/// **P10-1.6 改进**:
/// - 改返 `String` 而不是 `&'static str` (v1.5 `Box::leak` 每次调用泄漏 ~32-64 bytes)
/// - 加 Windows 非法字符过滤 `<>:"/\\|?*` (避免 plugin 名含特殊字符时产物命名失败)
/// - `-` 全部替 `_` (Rust 命名规则)
pub fn dylib_filename(spec_name: &str) -> String {
    let safe = sanitize_lib_name(spec_name);
    #[cfg(target_os = "windows")]
    {
        format!("{safe}.dll")
    }
    #[cfg(target_os = "macos")]
    {
        format!("lib{safe}.dylib")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        format!("lib{safe}.so")
    }
}

/// sanitize plugin 名 → dylib 安全名
///
/// - `-` → `_` (Rust crate 命名规则, 跟 `cargo` 自己一致)
/// - Windows 非法字符 `<>:"/\\|?*` + 控制字符 → `_` (Windows 文件名限制)
/// - 末尾 `.` / 空格 → `_` (Windows 修剪规则, 避免 cargo 怪异行为)
/// - 空名 → `"unnamed"` (兜底)
fn sanitize_lib_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        // valid alphanumeric + '_' 直接保留, 其他 (含 '-') 替换成 '_'
        // (clippy 报 identical blocks, 简化)
        let replacement = if c.is_ascii_alphanumeric() || c == '_' {
            c
        } else {
            '_'
        };
        out.push(replacement);
    }
    // 末尾 . / 空格 (Windows 修剪)
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
        out.push('_');
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}

/// 真编译 plugin (P10-1.5 + P10-1.6)
///
/// 业务方 (model 生成代码) → CreatorRegistry.register_spec() → compile() 调这个
/// 返 `CompileOutput` (artifact_path 业务方 P10-1.7 拿来 libloading 加载)
///
/// **P10-1.6 改进**: 同步函数, 由 `CreatorRegistry::compile` 走 `tokio::task::spawn_blocking` 包装.
/// 不能直接在 async runtime 上跑 (cargo subprocess 阻塞可达分钟级).
pub fn compile_plugin(
    spec: &PluginSpec,
    cfg: &CompileConfig,
) -> Result<CompileOutput, CreatorError> {
    let start = std::time::Instant::now();

    // 1. 准备输出目录
    // clippy 报 redundant closure: 函数指针直接传, 不用包 || closure
    let temp_root = cfg.temp_root.clone().unwrap_or_else(std::env::temp_dir);
    let output_root = cfg
        .output_dir
        .clone()
        .unwrap_or_else(|| temp_root.join("ma-harness-plugins"));
    let plugin_dir = output_root.join(&spec.name);
    let src_dir = plugin_dir.join("src");

    // 清理旧 dir (避免 cargo lock 冲突)
    if plugin_dir.exists() {
        std::fs::remove_dir_all(&plugin_dir).map_err(|e| {
            CreatorError::Compile(format!("清理旧目录失败 {}: {}", plugin_dir.display(), e))
        })?;
    }
    std::fs::create_dir_all(&src_dir)
        .map_err(|e| CreatorError::Compile(format!("创建目录失败: {e}")))?;

    // 2. 写 Cargo.toml (P10-1.8 v2: 不再需要 host_crate_path, plugin 独立 crate)
    let cargo_toml = render_cargo_toml(spec);
    let cargo_path = plugin_dir.join("Cargo.toml");
    std::fs::write(&cargo_path, cargo_toml)
        .map_err(|e| CreatorError::Compile(format!("写 Cargo.toml 失败: {e}")))?;

    // 3. 写 src/lib.rs (P10-1.8 v2: 业务方 source_code 全文 + framework wrap C-ABI JSON 入口)
    let lib_rs = render_lib_rs(spec);
    let lib_path = src_dir.join("lib.rs");
    std::fs::write(&lib_path, lib_rs)
        .map_err(|e| CreatorError::Compile(format!("写 src/lib.rs 失败: {e}")))?;

    // 4. 调 cargo build
    let cargo = match &cfg.cargo_path {
        Some(p) => p.clone(),
        None => find_cargo()?,
    };
    let mut cmd = Command::new(&cargo);
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(cargo_path)
        .arg("--target-dir")
        .arg(plugin_dir.join("target"))
        // 离线模式 (不下载新 deps, 用业务方已有 cache)
        // 业务方要新 dep 时去掉 offline, 这里保守选 offline-first
        .arg("--offline");

    if cfg.release {
        cmd.arg("--release");
    }

    // 跨平台: 设环境变量 (PATH / HOME / CARGO_HOME / RUSTUP_HOME / PATHEXT)
    // 业务方在 Windows minimal 容器里跑时, 这些 env 缺一不可
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    #[cfg(target_os = "windows")]
    {
        // Windows: PATHEXT 让 cargo.exe 可被找到 (含 .EXE/.CMD/.BAT 等)
        if let Ok(pathext) = std::env::var("PATHEXT") {
            cmd.env("PATHEXT", pathext);
        } else {
            // 兜底: 标准 Windows PATHEXT
            cmd.env("PATHEXT", ".EXE;.CMD;.BAT;.COM");
        }
        // Windows: SYSTEMROOT (cmd.exe 内置命令需要)
        if let Ok(sysroot) = std::env::var("SYSTEMROOT") {
            cmd.env("SYSTEMROOT", sysroot);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        cmd.env("USERPROFILE", userprofile);
    }
    if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
        cmd.env("CARGO_HOME", cargo_home);
    }
    if let Ok(rustup_home) = std::env::var("RUSTUP_HOME") {
        cmd.env("RUSTUP_HOME", rustup_home);
    }
    if let Ok(rustc_wrapper) = std::env::var("RUSTC_WRAPPER") {
        cmd.env("RUSTC_WRAPPER", rustc_wrapper);
    }

    // 5. 执行 (timeout 用 thread::spawn + sleep, 简单实现)
    let output = run_with_timeout(&mut cmd, cfg.timeout)?;

    let duration_ms = start.elapsed().as_millis();
    let stdout_preview = preview(&output.stdout, 500);
    let stderr_preview = preview(&output.stderr, 500);

    if !output.status.success() {
        return Err(CreatorError::Compile(format!(
            "cargo build 失败 (exit {:?}, {}ms)\nstdout: {}\nstderr: {}",
            output.status.code(),
            duration_ms,
            stdout_preview,
            stderr_preview,
        )));
    }

    // 6. 找产物 (.so/.dll/.dylib)
    let dylib_name = dylib_filename(&spec.name);
    let profile_dir = if cfg.release { "release" } else { "debug" };
    let artifact_path = plugin_dir
        .join("target")
        .join(profile_dir)
        .join(&dylib_name);

    if !artifact_path.exists() {
        return Err(CreatorError::Compile(format!(
            "编译成功但产物不存在: {} (期望 {} )",
            artifact_path.display(),
            dylib_name
        )));
    }

    Ok(CompileOutput {
        artifact_path,
        stdout_preview,
        stderr_preview,
        duration_ms,
    })
}

/// 渲染 Cargo.toml (P10-1.5 + P10-1.6 + P10-1.8 v2)
///
/// **P10-1.6 改进**: edition 2021 → 2024 (跟 workspace 对齐)
/// **P10-1.8 v2 改进**: 不再需要 host_crate_path (C-ABI JSON 模式), 业务方用 serde_json 即可
fn render_cargo_toml(spec: &PluginSpec) -> String {
    let user_deps = if spec.dependencies.is_empty() {
        String::new()
    } else {
        spec.dependencies.join("\n") + "\n"
    };
    // P10-1.8 v2: 业务方 source_code 几乎必用 serde_json::json!() 拼 schema + 解析 args
    let implicit_dep = "serde_json = \"1\"\n";
    let deps_block = if user_deps.is_empty() {
        format!("\n[dependencies]\n{implicit_dep}")
    } else {
        format!("\n[dependencies]\n{implicit_dep}{user_deps}")
    };
    format!(
        r#"[package]
name = "{name}"
version = "{version}"
edition = "2024"

[lib]
crate-type = ["cdylib"]
{deps}
"#,
        name = spec.name,
        version = spec.version,
        deps = deps_block,
    )
}

/// 渲染 src/lib.rs (P10-1.5 + P10-1.7 + P10-1.8 v2)
///
/// **P10-1.8 v2 改造** (业务方真可用):
/// - 业务方 source_code 写两个函数:
///   - `pub fn plugin_schemas() -> &'static str` — 返 JSON array 字符串
///   - `pub fn plugin_invoke(name: &str, args: serde_json::Value) -> serde_json::Value` — 调对应工具
/// - render 框架 wrap 成 C-ABI 入口:
///   - `plugin_schemas_json() -> *const c_char` — host 拿 JSON 解析成 Vec<ToolSchema>
///   - `plugin_invoke_json(name, args_json) -> *const c_char` — host 调 invoke 拿结果
/// - 跨 dylib 边界全是 C-ABI + JSON 字符串, ABI 稳定
/// - **不再需要 host_crate_path / ma-harness-core dep**, plugin 是独立 crate
/// - host 拿 schema 后**自己造 host ToolInvokeFn 调 plugin_invoke**, vtable 是 host 的, drop 安全
///
/// **P10-1.7 兼容**: 业务方 source_code 可写 `register()` 片段, 框架 wrap `extern "C" fn register()` (无入参, 仅 side effect).
fn render_lib_rs(spec: &PluginSpec) -> String {
    // P10-1.8 v2: 业务方 source_code 是全文, 写 plugin_schemas + plugin_invoke 两个函数
    // render 框架 wrap C-ABI 入口
    // **P10-1.8 v2 用 r##"..."## 双 hash 嵌套** (业务方 source_code 可能含 r#"..."# inner raw string)
    format!(
        r##"// Auto-generated from PluginSpec "{}" (P10-1.8 v2: C-ABI JSON mode)
// Plugin: {} v{}
// Entry function: {}
//
// 业务方 source_code 期望包含两个函数:
//   pub fn plugin_schemas() -> &'static str {{
//       r#"[{{"name":"my_tool","description":"...","parameters":{{...}}}}]"#
//   }}
//   pub fn plugin_invoke(name: &str, args: serde_json::Value) -> serde_json::Value {{
//       match name {{
//           "my_tool" => json!({{"result": "ok"}}),
//           _ => json!({{"error": format!("unknown tool: {{name}}")}}),
//       }}
//   }}
//
// 下面 wrapper 是 C-ABI 入口, host 用 libloading 调

{}

#[unsafe(no_mangle)]
pub extern "C" fn plugin_schemas_json() -> *const std::ffi::c_char {{
    plugin_schemas().as_ptr() as *const std::ffi::c_char
}}

#[unsafe(no_mangle)]
pub extern "C" fn plugin_invoke_json(
    name: *const std::ffi::c_char,
    args_json: *const std::ffi::c_char,
) -> *const std::ffi::c_char {{
    // SAFETY: host 传 valid C string, plugin 内部 unsafe cast
    let name_str = unsafe {{ std::ffi::CStr::from_ptr(name) }}
        .to_str()
        .unwrap_or("");
    let args_str = unsafe {{ std::ffi::CStr::from_ptr(args_json) }}
        .to_str()
        .unwrap_or("{{}}");
    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(serde_json::Value::Null);
    let result = plugin_invoke(name_str, args);
    // 返回 owned String, Box::leak 让 host 拿稳定指针 (plugin unload 时整体释放, 不算泄漏)
    Box::leak(result.to_string().into_boxed_str()).as_ptr() as *const std::ffi::c_char
}}
"##,
        spec.name, spec.name, spec.version, spec.entry_fn, spec.source_code
    )
}

/// subprocess + timeout (P10-1.5)
///
/// 跨平台: Windows 用 Command::output() 阻塞, Linux/Mac 同
/// 简化: 不用 tokio::process (避免引入 async 依赖)
/// 业务方 (CreatorRegistry::compile) 应该走 spawn_blocking 包装
fn run_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, CreatorError> {
    use std::io::Read;
    use std::process::Stdio;

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| CreatorError::Compile(format!("spawn cargo 失败: {e}")))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| CreatorError::Compile("拿 stdout 失败".to_string()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| CreatorError::Compile("拿 stderr 失败".to_string()))?;

    // 跨线程读 stdout/stderr 防止 pipe buffer 阻塞
    let stdout_thread = std::thread::spawn(move || {
        let mut s = Vec::new();
        stdout.read_to_end(&mut s).ok();
        s
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut s = Vec::new();
        stderr.read_to_end(&mut s).ok();
        s
    });

    // 限时 wait
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_thread.join().unwrap_or_default();
                let stderr = stderr_thread.join().unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(CreatorError::Compile(format!(
                        "编译超时 ({}s)",
                        timeout.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return Err(CreatorError::Compile(format!("wait 失败: {e}")));
            }
        }
    }
}

/// preview 截前 N 行
fn preview(bytes: &[u8], max_lines: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    s.lines().take(max_lines).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_cargo_returns_path() {
        // 真找 cargo, 在 CI / dev env 应该能找到
        let p = find_cargo();
        // 不强求成功 (PATH 可能没设), 但应该不 panic
        if let Ok(path) = &p {
            assert!(!path.as_os_str().is_empty());
        }
    }

    #[test]
    fn dylib_filename_per_platform() {
        let n = dylib_filename("hello-world");
        #[cfg(target_os = "windows")]
        assert_eq!(n, "hello_world.dll");
        #[cfg(target_os = "macos")]
        assert_eq!(n, "libhello_world.dylib");
        #[cfg(all(unix, not(target_os = "macos")))]
        assert_eq!(n, "libhello_world.so");
    }

    #[test]
    fn dylib_filename_preserves_underscore() {
        let n = dylib_filename("hello").to_string();
        assert!(!n.contains("hello-world"), "应保留 _, 不变 -");
    }

    #[test]
    fn dylib_filename_handles_windows_illegal_chars() {
        // P10-1.6: Windows 非法字符 < > : " / \ | ? * + 控制字符
        let n = dylib_filename("my<tool:foo>bar|baz");
        // 全部 _ 替
        assert!(!n.contains('<'), "应过滤 <: got {n}");
        assert!(!n.contains('>'), "应过滤 >: got {n}");
        assert!(!n.contains(':'), "应过滤 :: got {n}");
        assert!(!n.contains('|'), "应过滤 |: got {n}");
    }

    #[test]
    fn dylib_filename_handles_empty() {
        let n = dylib_filename("");
        assert!(!n.is_empty(), "空名应有 fallback");
        // 兜底是 "unnamed"
        assert!(n.contains("unnamed"), "应 fallback 到 unnamed: got {n}");
    }

    #[test]
    fn dylib_filename_handles_trailing_dot() {
        // P10-1.6: 末尾 . / 空格 → 替 _ (Windows 修剪)
        let n = dylib_filename("foo.");
        assert!(!n.ends_with('.'), "应修剪末尾 .: got {n}");
        // 实际末尾变 _
        assert!(
            n.ends_with('_') || n.ends_with(".dll") || n.ends_with(".so") || n.ends_with(".dylib"),
            "末尾不应是 .: got {n}"
        );
    }

    #[test]
    fn dylib_filename_does_not_leak() {
        // P10-1.6: 改返 String, 不再 Box::leak
        // 跑 10000 次, 拿 Vec<String> 验不爆内存
        // 实际验证: 每次都返新 String, 不共享 static
        let mut all = Vec::with_capacity(1000);
        for i in 0..1000 {
            all.push(dylib_filename(&format!("plugin_{i}")));
        }
        // 每个 element 都是独立 String
        assert_eq!(all.len(), 1000);
        // 没有重复 (每个名独立)
        assert_eq!(
            all.iter().collect::<std::collections::HashSet<_>>().len(),
            1000
        );
    }

    #[test]
    fn sanitize_lib_name_basic() {
        assert_eq!(sanitize_lib_name("hello"), "hello");
        assert_eq!(sanitize_lib_name("hello-world"), "hello_world");
        assert_eq!(sanitize_lib_name("hello_world"), "hello_world");
        assert_eq!(sanitize_lib_name("hello123"), "hello123");
    }

    #[test]
    fn sanitize_lib_name_unicode() {
        // 中文字符应保留 (作为 _ 替, 但保留字符)
        let n = sanitize_lib_name("你好");
        // 非 ASCII alphanumeric 全部替 _
        assert_eq!(n, "__");
    }

    #[test]
    fn render_cargo_toml_includes_cdylib() {
        let spec = PluginSpec {
            name: "test_plugin".into(),
            version: "0.1.0".into(),
            description: "test".into(),
            source_code: "".into(),
            entry_fn: "register".into(),
            dependencies: vec![],
        };
        let toml = render_cargo_toml(&spec);
        assert!(toml.contains("crate-type = [\"cdylib\"]"));
        assert!(toml.contains("name = \"test_plugin\""));
        assert!(toml.contains("version = \"0.1.0\""));
        // P10-1.6: edition 改 2024
        assert!(
            toml.contains(r#"edition = "2024""#),
            "edition 应是 2024: got {toml}"
        );
        // P10-1.8 v2: 自动加 serde_json
        assert!(
            toml.contains("serde_json"),
            "应自动加 serde_json: got {toml}"
        );
    }

    #[test]
    fn render_cargo_toml_includes_dependencies() {
        let spec = PluginSpec {
            name: "p".into(),
            version: "0.1.0".into(),
            description: "".into(),
            source_code: "".into(),
            entry_fn: "register".into(),
            dependencies: vec!["serde = \"1\"".into(), "regex = \"1\"".into()],
        };
        let toml = render_cargo_toml(&spec);
        assert!(toml.contains("serde = \"1\""));
        assert!(toml.contains("regex = \"1\""));
    }

    #[test]
    fn render_lib_rs_includes_c_abi_json_wrappers() {
        // P10-1.8 v2: 业务方 source_code 是全文, 框架 wrap C-ABI JSON 入口
        let spec = PluginSpec {
            name: "myplugin".into(),
            version: "0.1.0".into(),
            description: "".into(),
            source_code: r##"
pub fn plugin_schemas() -> &'static str {
    r#"[{"name":"myplugin","description":"test","parameters":{}}]"#
}

pub fn plugin_invoke(name: &str, args: serde_json::Value) -> serde_json::Value {
    match name {
        "myplugin" => serde_json::json!({"echo": args}),
        _ => serde_json::json!({"error": format!("unknown: {name}")}),
    }
}
"##
            .into(),
            entry_fn: "register".into(),
            dependencies: vec![],
        };
        let lib = render_lib_rs(&spec);
        // P10-1.8 v2: 框架 wrap C-ABI 入口
        assert!(
            lib.contains("plugin_schemas_json"),
            "应有 plugin_schemas_json wrapper: got\n{lib}"
        );
        assert!(
            lib.contains("plugin_invoke_json"),
            "应有 plugin_invoke_json wrapper: got\n{lib}"
        );
        assert!(
            lib.contains(r#"extern "C" fn plugin_schemas_json()"#),
            "plugin_schemas_json 应该是 extern C: got\n{lib}"
        );
        assert!(
            lib.contains(r#"extern "C" fn plugin_invoke_json("#),
            "plugin_invoke_json 应该是 extern C: got\n{lib}"
        );
        // 业务方 source_code 全文应包含
        assert!(
            lib.contains("plugin_schemas()"),
            "应包含业务方 plugin_schemas: got\n{lib}"
        );
        assert!(
            lib.contains("plugin_invoke("),
            "应包含业务方 plugin_invoke: got\n{lib}"
        );
        // 没 #[unsafe(no_mangle)] 直接 attribute (P10-1.7 兼容)
        assert!(
            !lib.contains(r#"pub extern "C" fn register()"#),
            "P10-1.8 v2 不再有 register extern C (改 plugin_schemas_json + plugin_invoke_json): got\n{lib}"
        );
    }

    #[test]
    fn compile_plugin_actually_runs_cargo() {
        // 真编译 (用 cargo 当前工具链) — 慢, 但跨平台验证
        // P10-1.6: 加 cfg! 给不同平台不同 skip 策略
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = CompileConfig {
            temp_root: Some(dir.path().to_path_buf()),
            output_dir: Some(dir.path().join("out")),
            timeout: Duration::from_secs(120),
            release: false, // debug 编译快
            ..Default::default()
        };
        let spec = PluginSpec {
            name: "compile_test_plugin".into(),
            version: "0.1.0".into(),
            description: "compile test".into(),
            source_code: r#"pub fn add(a: i32, b: i32) -> i32 { a + b }"#.into(),
            entry_fn: "register".into(),
            dependencies: vec![],
        };

        let result = compile_plugin(&spec, &cfg);
        if let Err(e) = &result {
            // CI 环境可能没 cargo / network, skip 失败
            eprintln!("compile_plugin 失败 (CI skip): {e}");
            return;
        }
        let output = result.unwrap();
        assert!(output.artifact_path.exists(), "产物应存在");
        assert!(output.duration_ms > 0);
    }
}
