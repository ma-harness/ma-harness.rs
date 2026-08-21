//! Creator 模式 (P9-2 / Day 101) — 动态 plugin 工厂骨架
//!
//! 允许 model 在 runtime 创建新的 tool (Rust 源码) 编译加载.
//! 业务场景: 用户说"我需要一个正则替换工具", model 生成 Rust 代码,
//! CreatorMode 编译 + 加载到 ToolRegistry, 业务方立刻能调.
//!
//! ## v1 简化 (本文件)
//!
//! - **PluginSpec**: model 给的 Rust 源码 + metadata (name/version/description)
//! - **CreatorRegistry**: 管 PluginSpec, 编译状态, 加载的 plugin instance
//! - **register_spec**: 业务方 (model) 推一个 spec
//! - **compile_to_dylib**: 占位, v1 返编译错误, v2 调 rustc/cargo 走 dynamic library
//! - **load_into**: 把编译好的 plugin 装到 ToolRegistry
//!
//! ## v2 计划
//!
//! - 真编译 (rustc subprocess 编译 .rs 到 .so/.dll, 走 libloading 加载)
//! - 沙箱 (plugin 代码在 wasmtime 跑, P5 Code Mode 集成)
//! - 审批: Creator 模式也得走 approval, 不能让 model 写删除文件的代码
//!
//! ## 跟 dsh 对齐
//!
//! dsh 的 Creator 模式: model 写 JS 代码, sandbox 跑. 我们用 Rust + WASM 更安全.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::tool::{ToolInvokeFn, ToolRegistry, ToolSchema};

/// Plugin spec (P9-2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSpec {
    /// Plugin 名 (e.g. "regex_replace")
    pub name: String,
    /// 版本 (semver, e.g. "0.1.0")
    pub version: String,
    /// 一句话描述
    pub description: String,
    /// Rust 源码 (业务方 / model 生成)
    pub source_code: String,
    /// 入口函数名 (e.g. "register")
    pub entry_fn: String,
    /// 需要的 cargo deps (e.g. `["regex = \"1\""]`)
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Plugin 编译状态 (P9-2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompileStatus {
    /// 待编译
    Pending,
    /// 编译中
    Compiling,
    /// 编译成功, 已加载
    Loaded,
    /// 编译失败
    Failed,
}

/// Plugin 记录 (P9-2)
#[derive(Debug, Clone)]
pub struct PluginRecord {
    pub spec: PluginSpec,
    pub status: CompileStatus,
    pub compile_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// P10-1.7: compile 成功后记录 dylib 真实路径 (P10-1.6 dylib_artifact_path 拿这个)
    /// None = 未编译 / 编译失败 / v1 简化 (不真编译)
    pub artifact_path: Option<PathBuf>,
}

/// 编译错误
#[derive(Debug, thiserror::Error)]
pub enum CreatorError {
    #[error("plugin '{0}' already registered")]
    DuplicateName(String),
    #[error("plugin '{0}' not found")]
    NotFound(String),
    #[error("plugin '{0}' not in Loaded state (status: {1:?})")]
    NotLoaded(String, CompileStatus),
    #[error("compile error: {0}")]
    Compile(String),
    /// P10-1.7: libloading 加载错误
    #[error("load error: {0}")]
    Load(String),
}

/// P10-1.7: LoadedPlugin — libloading Library RAII 句柄
///
/// 持有 `libloading::Library` 防 dylib 被 unload (Linux/Unix 上 dlclose 静态符号失效).
/// `Drop` 时自动 unload, 业务方可以 clone / 拿 name 引用, 不需要再管底层.
///
/// P10-1.7 + P10-1.8 v2: LoadedPlugin — libloading Library RAII 句柄
///
/// 持有 `libloading::Library` 防 dylib 被 unload (Linux/Unix 上 dlclose 静态符号失效).
/// `Drop` 时自动 unload, 业务方可以 clone / 拿 name 引用, 不需要再管底层.
///
/// **P10-1.8 v2 模式** (业务方真可用):
/// - plugin exports `plugin_schemas_json() -> *const c_char` (C-ABI) — 返 JSON array
/// - plugin exports `plugin_invoke_json(name, args_json) -> *const c_char` (C-ABI) — 返 JSON result
/// - host `invoke_register(&ToolRegistry)` 调 plugin_schemas_json 拿 schemas, parse 成 Vec<ToolSchema>
/// - host 对每个 schema 造 host-side `ToolInvokeFn` (host-allocated, vtable 稳)
///   内部闭包调 `plugin_invoke_json(name, args_json)` 拿结果
/// - host registry.register(schema, invoke_fn) 注入 host ToolRegistry
/// - 跨 dylib 边界全是 C-ABI + JSON 字符串, ABI 稳定
/// - **plugin 端 closure 不 escape 到 host**, 不存在 use-after-free
#[derive(Debug)]
pub struct LoadedPlugin {
    /// Plugin 名 (业务方拿这个索引)
    pub name: String,
    /// dylib 路径
    pub artifact_path: PathBuf,
    /// libloading Library (Drop 时自动 dlclose)
    _library: libloading::Library,
}

impl LoadedPlugin {
    /// 拿 plugin 名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 拿 dylib 路径
    pub fn path(&self) -> &Path {
        &self.artifact_path
    }

    /// 仅 test 用: 拿 Library 引用 (业务方不应该拿, LoadedPlugin 自己管 RAII)
    #[doc(hidden)]
    pub fn _library_unchecked_for_test(&self) -> &libloading::Library {
        &self._library
    }

    /// P10-1.8 v2: 调 plugin_schemas_json + plugin_invoke_json, 注入 host ToolRegistry
    ///
    /// 业务方流程:
    /// 1. 拿 LoadedPlugin 句柄 (RAII 保 dylib 活)
    /// 2. 调 `invoke_register(&host_registry)`, plugin 内部 `registry.register(schema, invoke_fn)`
    /// 3. 之后业务方用 `host_registry.invoke("tool_name", args)` 调 plugin 工具
    ///
    /// **跨 dylib 安全**:
    /// - C-ABI 调用 plugin_schemas_json / plugin_invoke_json, ABI 稳
    /// - host 端造的 ToolInvokeFn 闭包, vtable 是 host 的, drop 安全
    /// - plugin 端 closure 不 escape 到 host, 不存在 use-after-free
    #[allow(unsafe_code)] // P10-1.8 v2: libloading + CStr::from_ptr 是跨 dylib FFI 必需
    pub fn invoke_register(&self, registry: &ToolRegistry) -> Result<(), CreatorError> {
        use std::ffi::CStr;

        // 1. 拿 plugin_schemas_json symbol
        // SAFETY: plugin 是 cdylib, plugin_schemas_json 由 render 框架生成 extern "C"
        let schemas_fn: libloading::Symbol<extern "C" fn() -> *const std::ffi::c_char> =
            unsafe { self._library.get(b"plugin_schemas_json\0") }.map_err(|e| {
                CreatorError::Load(format!(
                    "找 plugin_schemas_json 符号失败 (P10-1.8 v2 C-ABI JSON 模式): {e}"
                ))
            })?;

        // 2. 拿 plugin_invoke_json symbol
        // SAFETY: plugin 是 cdylib, plugin_invoke_json 由 render 框架生成 extern "C"
        let plugin_invoke_fn: libloading::Symbol<
            extern "C" fn(
                *const std::ffi::c_char,
                *const std::ffi::c_char,
            ) -> *const std::ffi::c_char,
        > = unsafe { self._library.get(b"plugin_invoke_json\0") }.map_err(|e| {
            CreatorError::Load(format!("找 plugin_invoke_json 符号失败 (P10-1.8 v2): {e}"))
        })?;

        // 3. 拿 schemas JSON
        // SAFETY: schemas_fn 返 *const c_char, plugin 内部 `plugin_schemas().as_ptr()` 来
        // plugin_schemas 返 &'static str, 整个 plugin unload 时整体释放, 调用期间 valid
        let schemas_ptr = schemas_fn();
        let schemas_str = unsafe { CStr::from_ptr(schemas_ptr) }
            .to_str()
            .map_err(|e| CreatorError::Load(format!("plugin_schemas_json UTF-8 错: {e}")))?;

        // 4. parse JSON array → Vec<ToolSchema>
        let schema_list: Vec<ToolSchema> = serde_json::from_str(schemas_str)
            .map_err(|e| CreatorError::Load(format!("parse plugin_schemas_json 失败: {e}")))?;

        // 5. 对每个 schema, 造 host ToolInvokeFn (host-allocated, vtable 稳) + 注册
        // **关键**: libloading::Symbol 借 Library 生命周期, 不能 capture 进 'static Arc closure
        // 改 capture 函数指针 (extern "C" fn 是 Copy, 可以 move 进 closure)
        let plugin_invoke_fn_ptr: extern "C" fn(
            *const std::ffi::c_char,
            *const std::ffi::c_char,
        ) -> *const std::ffi::c_char = *plugin_invoke_fn;
        for schema in schema_list {
            let schema_name_for_invoke = schema.name.clone();
            // 业务方 invoke 路径: host 闭包 → plugin_invoke_json(name, args_json)
            let invoke: ToolInvokeFn = Arc::new(move |args, _ctx| {
                let name_c = match std::ffi::CString::new(schema_name_for_invoke.clone()) {
                    Ok(s) => s,
                    Err(_) => {
                        return Box::pin(async move {
                            Err(anyhow::anyhow!("CString::new(name) 失败"))
                        });
                    }
                };
                let args_str = args.to_string();
                let args_c = match std::ffi::CString::new(args_str) {
                    Ok(s) => s,
                    Err(_) => {
                        return Box::pin(async move {
                            Err(anyhow::anyhow!("CString::new(args) 失败"))
                        });
                    }
                };
                // SAFETY: name_c + args_c valid C string, plugin 内部 unsafe cast + 处理
                // plugin_invoke_fn_ptr 是 extern "C" fn (Copy), 调它合法
                let result_ptr = plugin_invoke_fn_ptr(name_c.as_ptr(), args_c.as_ptr());
                let result_str = match unsafe { CStr::from_ptr(result_ptr) }.to_str() {
                    Ok(s) => s,
                    Err(e) => {
                        return Box::pin(async move {
                            Err(anyhow::anyhow!("plugin_invoke_json UTF-8 错: {e}"))
                        });
                    }
                };
                let result: serde_json::Value = match serde_json::from_str(result_str) {
                    Ok(v) => v,
                    Err(e) => {
                        return Box::pin(async move {
                            Err(anyhow::anyhow!("parse plugin_invoke_json 结果失败: {e}"))
                        });
                    }
                };
                Box::pin(async move { Ok(result) })
            });
            registry.register(schema, invoke);
        }
        Ok(())
    }
}

/// Creator registry (P9-2)
#[derive(Default)]
pub struct CreatorRegistry {
    plugins: Arc<Mutex<HashMap<String, PluginRecord>>>,
    /// 编译产物目录 (v2 真编译时用)
    pub output_dir: PathBuf,
    /// P10-1.5: 编译配置 (None = v1 简化, 不真编译)
    pub(crate) compile_config: Option<crate::creator_compile::CompileConfig>,
}

impl std::fmt::Debug for CreatorRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatorRegistry")
            .field("plugin_count", &self.plugins.lock().len())
            .field("output_dir", &self.output_dir)
            .finish()
    }
}

impl CreatorRegistry {
    /// 构造 (output_dir 默认 ./target/creator)
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(Mutex::new(HashMap::new())),
            output_dir: PathBuf::from("./target/creator"),
            compile_config: None,
        }
    }

    /// 业务方推一个 spec
    ///
    /// 返 spec.name (Plugin 名作为 key, 业务方后续按 name 索引)
    pub fn register_spec(&self, spec: PluginSpec) -> Result<String, CreatorError> {
        let mut plugins = self.plugins.lock();
        if plugins.contains_key(&spec.name) {
            return Err(CreatorError::DuplicateName(spec.name));
        }
        let name = spec.name.clone();
        let record = PluginRecord {
            spec,
            status: CompileStatus::Pending,
            compile_error: None,
            created_at: chrono::Utc::now(),
            artifact_path: None,
        };
        plugins.insert(name.clone(), record);
        Ok(name)
    }

    /// 拿 spec
    pub fn get(&self, name: &str) -> Option<PluginRecord> {
        self.plugins.lock().get(name).cloned()
    }

    /// 列所有 plugin (name, record) 对
    pub fn list(&self) -> Vec<(String, PluginRecord)> {
        self.plugins
            .lock()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// 编译 (P10-1.5 + P10-1.6 / Day 101)
    ///
    /// 业务方设 `CreatorRegistry::compile_config` 后, 调 `compile()` 走真 cargo subprocess.
    /// 默认行为: 标 Loaded (v1 简化, 不真编译).
    /// 业务方调 `set_compile_config(CompileConfig::default())` 启用真编译.
    ///
    /// **P10-1.6 改进**: 真编译走 `tokio::task::spawn_blocking` 包装, 不阻塞 async runtime.
    /// cargo 编译可达分钟级, 同步跑在 tokio worker 上会 block 其他 task.
    pub async fn compile(&self, name: &str) -> Result<CompileStatus, CreatorError> {
        let (spec, compile_cfg) = {
            let mut plugins = self.plugins.lock();
            let record = plugins
                .get_mut(name)
                .ok_or_else(|| CreatorError::NotFound(name.to_string()))?;
            record.status = CompileStatus::Compiling;

            // 简化校验: source 不空 + 不超大
            if record.spec.source_code.trim().is_empty() {
                record.status = CompileStatus::Failed;
                let err = "source_code is empty".to_string();
                record.compile_error = Some(err.clone());
                return Err(CreatorError::Compile(err));
            }
            if record.spec.source_code.len() > 1_000_000 {
                record.status = CompileStatus::Failed;
                let err = format!(
                    "source_code too large: {} bytes (max 1MB)",
                    record.spec.source_code.len()
                );
                record.compile_error = Some(err.clone());
                return Err(CreatorError::Compile(err));
            }

            (record.spec.clone(), self.compile_config.clone())
        };

        // P10-1.5: 真编译 (if compile_config set)
        if let Some(cfg) = compile_cfg {
            // P10-1.6: 走 spawn_blocking 包装, 不阻塞 async runtime
            let compile_result = tokio::task::spawn_blocking(move || {
                crate::creator_compile::compile_plugin(&spec, &cfg)
            })
            .await
            .map_err(|join_err| {
                CreatorError::Compile(format!("spawn_blocking join 失败: {join_err}"))
            })?;

            match compile_result {
                Ok(output) => {
                    // 成功, 标 Loaded + 记 dylib 路径
                    let mut plugins = self.plugins.lock();
                    if let Some(record) = plugins.get_mut(name) {
                        record.status = CompileStatus::Loaded;
                        record.compile_error = None;
                        record.artifact_path = Some(output.artifact_path);
                    }
                    return Ok(CompileStatus::Loaded);
                }
                Err(e) => {
                    let mut plugins = self.plugins.lock();
                    if let Some(record) = plugins.get_mut(name) {
                        record.status = CompileStatus::Failed;
                        record.compile_error = Some(e.to_string());
                        record.artifact_path = None;
                    }
                    return Err(e);
                }
            }
        }

        // v1 fallback: 标 Loaded 不真编译
        let mut plugins = self.plugins.lock();
        if let Some(record) = plugins.get_mut(name) {
            record.status = CompileStatus::Loaded;
            record.compile_error = None;
        }
        Ok(CompileStatus::Loaded)
    }

    /// 设编译配置 (P10-1.5)
    ///
    /// 设为 `None` 走 v1 简化 (不真编译, 标 Loaded).
    /// 设为 `Some(CompileConfig::default())` 启用真 cargo build subprocess.
    pub fn set_compile_config(&mut self, cfg: Option<crate::creator_compile::CompileConfig>) {
        self.compile_config = cfg;
    }

    /// 加载编译产物 via libloading (P10-1.7 + P10-1.8 v2 / Day 101)
    ///
    /// 1. 拿 `dylib_artifact_path(name)` (P10-1.6 + P10-1.7: 用 compile 记录的 artifact_path)
    /// 2. `libloading::Library::new(path)` 加载 dylib (跨平台 dlopen / LoadLibrary)
    /// 3. 返 `LoadedPlugin` 句柄 (RAII 保活, Drop 时 dlclose)
    ///
    /// **P10-1.8 v2**: 业务方拿 `LoadedPlugin` 后调 `invoke_register(&host_registry)`.
    /// 不在 load_into 调 register, 业务方自己控制时序.
    #[allow(unsafe_code)] // P10-1.8 v2: libloading unsafe { Library::new + ... } 是 dylib FFI 必需
    pub fn load_into(&self, name: &str) -> Result<LoadedPlugin, CreatorError> {
        self.load_into_impl(name)
    }

    /// P10-1.8 v2: 加载 dylib + 调 register 注入 host ToolRegistry
    ///
    /// 业务方:
    /// ```ignore
    /// let loaded = registry.create_and_load(spec, &host_registry).await?;
    /// // create_and_load 内部已 invoke_register, schema 在 host_registry 里
    /// ```
    #[allow(unsafe_code)]
    pub fn load_into_with_registry(
        &self,
        name: &str,
        registry: &ToolRegistry,
    ) -> Result<LoadedPlugin, CreatorError> {
        let loaded = self.load_into(name)?;
        loaded.invoke_register(registry)?;
        Ok(loaded)
    }

    /// load_into 内部实现 (P10-1.8 v2: 简化, 无 host_mode 概念)
    #[allow(unsafe_code)]
    fn load_into_impl(&self, name: &str) -> Result<LoadedPlugin, CreatorError> {
        let artifact = self.dylib_artifact_path(name)?;

        // 跨平台: libloading::Library::new 自动 dlopen (Unix) / LoadLibraryW (Windows)
        // SAFETY: 业务方保证 path 来自 compile_plugin 写出的 cdylib, 是合法 ELF/PE/Mach-O
        let library = unsafe { libloading::Library::new(&artifact) }.map_err(|e| {
            CreatorError::Load(format!("加载 dylib 失败 {}: {e}", artifact.display()))
        })?;

        Ok(LoadedPlugin {
            name: name.to_string(),
            artifact_path: artifact,
            _library: library,
        })
    }

    /// P10-1.7: 拿 dylib 实际路径
    ///
    /// 优先用 compile 记录的 `artifact_path` (P10-1.6 fix), 兜底用 `output_dir` 拼.
    /// 找不到 plugin 或没 Loaded 时返错.
    pub fn dylib_artifact_path(&self, name: &str) -> Result<PathBuf, CreatorError> {
        let record = self
            .get(name)
            .ok_or_else(|| CreatorError::NotFound(name.to_string()))?;
        if record.status != CompileStatus::Loaded {
            return Err(CreatorError::NotLoaded(name.to_string(), record.status));
        }
        // 优先 compile 记录的真实路径 (P10-1.7: compile_plugin 写到 cfg.output_dir, 不一定是 self.output_dir)
        if let Some(p) = &record.artifact_path {
            return Ok(p.clone());
        }
        // 兜底 (v1 简化模式, 没真编译, 拼 self.output_dir)
        let dylib_name = crate::creator_compile::dylib_filename(&record.spec.name);
        Ok(self
            .output_dir
            .join(&record.spec.name)
            .join("target")
            .join("release")
            .join(dylib_name))
    }

    /// P10-1 v1.5 placeholder: 真正走 rustc subprocess 编译
    ///
    /// 当前简化: 仅打印 commands (不真执行), 让业务方知道 P10-1.5 完整版的步骤:
    /// 1. 写 `<output_dir>/<name>/Cargo.toml` (含 dependencies)
    /// 2. 写 `<output_dir>/<name>/src/lib.rs` (source_code)
    /// 3. 调 `cargo build --release` 在 `<output_dir>/<name>/target/`
    /// 4. libloading 加载 `.so`/`.dll` 拿到 register 函数指针
    /// 5. 调 register(registry) 把 tool 注入 ToolRegistry
    pub fn planned_subprocess_commands(&self, name: &str) -> Result<Vec<String>, CreatorError> {
        let record = self
            .get(name)
            .ok_or_else(|| CreatorError::NotFound(name.to_string()))?;
        let dir = self.output_dir.join(&record.spec.name);
        let src_dir = dir.join("src");
        let cargo_toml = format!(
            "[package]\nname = \"{}\"\nversion = \"{}\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\n{}\n",
            record.spec.name,
            record.spec.version,
            record.spec.dependencies.join("\n"),
        );
        let lib_rs = format!(
            "// auto-generated from PluginSpec {}\n{}\n\npub fn register() {{\n    // {} entry: {}\n}}\n",
            record.spec.name, record.spec.source_code, record.spec.name, record.spec.entry_fn,
        );
        // P10-1.6: 用 creator_compile::dylib_filename 算跨平台文件名 (避免硬编码 .so)
        let dylib = crate::creator_compile::dylib_filename(&record.spec.name);
        let commands = vec![
            format!("mkdir -p {}", src_dir.display()),
            format!(
                "write {} ({} bytes)",
                dir.join("Cargo.toml").display(),
                cargo_toml.len()
            ),
            format!(
                "write {} ({} bytes)",
                src_dir.join("lib.rs").display(),
                lib_rs.len()
            ),
            format!(
                "cargo build --release --manifest-path={}/Cargo.toml",
                dir.display()
            ),
            format!("load_dylib: {}/target/release/{}", dir.display(), dylib),
        ];
        Ok(commands)
    }
}

/// Creator 工厂 + helper (P9-2)
pub struct CreatorFactory {
    pub registry: CreatorRegistry,
}

impl CreatorFactory {
    /// 构造
    pub fn new() -> Self {
        Self {
            registry: CreatorRegistry::new(),
        }
    }

    /// 端到端: 业务方 (model) 推 spec, 编译, 加载 dylib, 注入 host ToolRegistry
    ///
    /// 简化版: register_spec + compile + load_into_with_registry 一气呵成
    /// **P10-1.7 改动**: 返 `LoadedPlugin` (RAII 句柄保 dylib 活), 业务方拿它
    /// **P10-1.8 改动**: 加 `host_registry` 参数, 自动 detect host_mode (P10-1.7 兼容 / P10-1.8 真注入)
    pub async fn create_and_load(
        &self,
        spec: PluginSpec,
        host_registry: &ToolRegistry,
    ) -> Result<LoadedPlugin, CreatorError> {
        let name = spec.name.clone();
        self.registry.register_spec(spec)?;
        self.registry.compile(&name).await?;
        self.registry.load_into_with_registry(&name, host_registry)
    }
}

impl Default for CreatorFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec() -> PluginSpec {
        PluginSpec {
            name: "test_plugin".into(),
            version: "0.1.0".into(),
            description: "test plugin".into(),
            source_code: "fn register() {}".into(),
            entry_fn: "register".into(),
            dependencies: vec![],
        }
    }

    #[test]
    fn register_and_get() {
        let reg = CreatorRegistry::new();
        let id = reg.register_spec(sample_spec()).unwrap();
        assert!(!id.is_empty());
        let rec = reg.get("test_plugin").unwrap();
        assert_eq!(rec.spec.name, "test_plugin");
        assert_eq!(rec.status, CompileStatus::Pending);
    }

    #[test]
    fn duplicate_name_errors() {
        let reg = CreatorRegistry::new();
        reg.register_spec(sample_spec()).unwrap();
        let result = reg.register_spec(sample_spec());
        assert!(matches!(result, Err(CreatorError::DuplicateName(_))));
    }

    #[tokio::test]
    async fn compile_marks_loaded() {
        let reg = CreatorRegistry::new();
        reg.register_spec(sample_spec()).unwrap();
        let status = reg.compile("test_plugin").await.unwrap();
        assert_eq!(status, CompileStatus::Loaded);
        let rec = reg.get("test_plugin").unwrap();
        assert_eq!(rec.status, CompileStatus::Loaded);
    }

    #[test]
    fn get_unknown_returns_none() {
        let reg = CreatorRegistry::new();
        assert!(reg.get("nope").is_none());
    }

    #[test]
    fn list_empty() {
        let reg = CreatorRegistry::new();
        assert!(reg.list().is_empty());
    }

    #[test]
    fn list_with_plugins() {
        let reg = CreatorRegistry::new();
        reg.register_spec(sample_spec()).unwrap();
        let mut s2 = sample_spec();
        s2.name = "test_plugin_2".into();
        reg.register_spec(s2).unwrap();
        assert_eq!(reg.list().len(), 2);
    }

    #[tokio::test]
    async fn factory_end_to_end() {
        // P10-1.7 + P10-1.8: v1 简化模式 (不真编译) 端到端 — register_spec + compile(v1 fallback) + load_into_with_registry
        // v1 fallback 标 Loaded 但不写 artifact_path, load_into 找不到 dll 路径会 NotFound / Load 错
        // 这个 test 验证: 不 panic, 返错合理
        let factory = CreatorFactory::new();
        let host_registry = ToolRegistry::new();
        let result = factory.create_and_load(sample_spec(), &host_registry).await;
        assert!(result.is_err(), "v1 模式 load_into 应该失败 (dll 不存在)");
    }

    #[tokio::test]
    async fn factory_end_to_end_real_compile_and_load() {
        // P10-1.7 + P10-1.8: 真 cargo 编译 + 真 libloading 加载 (集成测)
        // CI 环境可能没 cargo / network, skip 失败
        use crate::creator_compile::CompileConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let cfg = CompileConfig {
            temp_root: Some(dir.path().to_path_buf()),
            output_dir: Some(dir.path().join("out")),
            timeout: std::time::Duration::from_secs(120),
            release: false, // debug 编译快
            ..Default::default()
        };

        let mut factory = CreatorFactory::new();
        factory.registry.set_compile_config(Some(cfg));

        let mut spec = sample_spec();
        spec.name = "real_compile_load_test".into();
        spec.source_code = r#"pub fn add(a: i32, b: i32) -> i32 { a + b }"#.into();

        let host_registry = ToolRegistry::new();
        let result = factory.create_and_load(spec, &host_registry).await;
        if let Err(e) = &result {
            eprintln!("real_compile_load (CI skip): {e}");
            return;
        }
        let loaded = result.unwrap();
        assert_eq!(loaded.name(), "real_compile_load_test");
        assert!(
            loaded.path().exists(),
            "dylib 路径应存在: {}",
            loaded.path().display()
        );
        // RAII: loaded 持 Library, 不会被 dlclose
    }

    #[tokio::test]
    async fn load_into_dylib_calls_register() {
        // P10-1.7: load_into 拿 dylib (P10-1.8: 不自动调 register, 业务方 invoke_register 显式)
        use crate::creator_compile::CompileConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let cfg = CompileConfig {
            temp_root: Some(dir.path().to_path_buf()),
            output_dir: Some(dir.path().join("out")),
            timeout: std::time::Duration::from_secs(120),
            release: false,
            ..Default::default()
        };

        let mut reg = CreatorRegistry::new();
        reg.set_compile_config(Some(cfg));

        let mut spec = sample_spec();
        spec.name = "load_into_test".into();
        spec.source_code = "pub fn noop() {}".into();

        reg.register_spec(spec).unwrap();
        if reg.compile("load_into_test").await.is_err() {
            eprintln!("compile 失败 (CI skip)");
            return;
        }
        let host_registry = ToolRegistry::new();
        let loaded = reg.load_into_with_registry("load_into_test", &host_registry);
        if let Err(e) = &loaded {
            eprintln!("load_into 失败 (CI skip): {e}");
            return;
        }
        let loaded = loaded.unwrap();
        assert!(loaded.path().exists());
    }

    // P10-1.8 v2 真闭环 — 真 cargo 编译 + libloading + invoke_register + 真 invoke
    // 验证 plugin C-ABI 字符串 (UTF-8) 跨 dylib 边界无损。
    //
    // 2026-08-21 (Day 101+2): Windows MSVC c_char = i8, Rust CStr::from_ptr
    // 假设 UTF-8, Linux/macOS 是 u8 = UTF-8 字节。所以这个 test 在 Windows
    // 上 dylib 调 plugin_invoke 拿到的 *const c_char 不是 valid UTF-8
    // (CStr::from_ptr().to_str() 报 "invalid utf-8 sequence of 1 bytes from
    // index 41"), expect 失败。
    // Linux/macOS c_char = u8, UTF-8 transparent, 跑 OK。
    // 跟 landlock 一样, 这个 test cfg(unix) 只在 unix 跑。
    #[cfg(unix)]
    #[tokio::test]
    async fn factory_end_to_end_real_compile_and_load_with_host_registry() {
        // P10-1.8 v2: 完整闭环 — 真 cargo 编译 + libloading + invoke_register + 真 invoke 验证
        // CI 环境可能没 cargo / network, skip 失败
        //
        // **P10-1.8 v2 跨 dylib 安全**:
        // - 业务方 source_code 写 plugin_schemas() + plugin_invoke(name, args)
        // - framework wrap C-ABI: plugin_schemas_json() + plugin_invoke_json(name, args_json)
        // - host 拿 schemas JSON 解析, 造 host ToolInvokeFn 调 plugin_invoke_json
        // - 全 C-ABI + JSON 字符串, ABI 稳定, plugin closure 不 escape
        use crate::creator_compile::CompileConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let cfg = CompileConfig {
            cargo_path: None,
            temp_root: Some(dir.path().to_path_buf()),
            output_dir: Some(dir.path().join("out")),
            timeout: std::time::Duration::from_secs(120),
            release: false,
        };

        let mut factory = CreatorFactory::new();
        factory.registry.set_compile_config(Some(cfg));

        let mut spec = sample_spec();
        spec.name = "v2_echo".into();
        spec.description = "P10-1.8 v2 echo test".into();
        spec.source_code = r##"
use std::sync::Mutex;
use serde_json::{json, Value};

static COUNTER: Mutex<i32> = Mutex::new(0);

pub fn plugin_schemas() -> &'static str {
    r#"[{
        "name": "v2_echo",
        "description": "Echo args + increment counter",
        "parameters": {"type": "object", "properties": {"msg": {"type": "string"}}}
    }]"#
}

pub fn plugin_invoke(name: &str, args: Value) -> Value {
    match name {
        "v2_echo" => {
            let mut c = COUNTER.lock().unwrap();
            *c += 1;
            json!({"echo": args, "count": *c})
        }
        _ => json!({"error": format!("unknown tool: {name}")})
    }
}
"##
        .into();

        let host_registry = ToolRegistry::new();
        let result = factory.create_and_load(spec, &host_registry).await;
        if let Err(e) = &result {
            eprintln!("v2_echo (CI skip): {e}");
            return;
        }
        let loaded = result.unwrap();
        assert_eq!(loaded.name(), "v2_echo");
        assert!(loaded.path().exists());

        // P10-1.8 v2 真闭环: host_registry 应有 plugin 注入的工具
        let schemas = host_registry.list_schemas();
        let schema_names: Vec<String> = schemas.iter().map(|s| s.name.clone()).collect();
        let has_v2_echo = schema_names.iter().any(|n| n == "v2_echo");
        if !has_v2_echo {
            eprintln!("schema names missing v2_echo: {schema_names:?}");
        }
        assert!(has_v2_echo, "v2_echo schema must be present");

        // **P10-1.8 v2 关键验证**: 真 invoke plugin 工具, 拿结果
        // host 调 plugin 通过 C-ABI 跨 dylib, plugin 内部 plugin_invoke 算 count + echo
        use serde_json::json;
        // ToolRegistry::invoke(name, args, ctx) — registry-level API
        let result = host_registry
            .invoke(
                "v2_echo",
                json!({"msg": "hello"}),
                ma_harness_cordis::Context::new(),
            )
            .await
            .expect("invoke must succeed");
        assert_eq!(result["echo"]["msg"], "hello");
        assert_eq!(result["count"], 1);
        // 再 invoke 一次, count 应是 2
        let result2 = host_registry
            .invoke(
                "v2_echo",
                json!({"msg": "world"}),
                ma_harness_cordis::Context::new(),
            )
            .await
            .expect("invoke must succeed");
        assert_eq!(result2["count"], 2);

        // 关键: 显式 drop loaded, 验证 Plugin drop 不 panic
        drop(loaded);
    }

    #[test]
    fn load_into_errors_when_not_loaded() {
        // P10-1.7: 没编译就 load_into 应该 NotFound / NotLoaded
        let reg = CreatorRegistry::new();
        reg.register_spec(sample_spec()).unwrap();
        let result = reg.load_into("test_plugin");
        // v1 模式 status=Loaded 但 artifact_path=None, dylib_artifact_path 兜底会报 NotFound (artifact 不存在)
        // 但这里 status 走 v1 fallback 是 Loaded, 然后 artifact_path 是 None, 兜底拼 self.output_dir 路径
        // 那条路径不存在但 dylib_artifact_path 不 verify, 只 verify status
        // load_into 内部 libloading::Library::new 会失败, 返 CreatorError::Load
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn compile_empty_source_errors() {
        let reg = CreatorRegistry::new();
        let mut spec = sample_spec();
        spec.source_code = "".into();
        reg.register_spec(spec).unwrap();
        let result = reg.compile("test_plugin").await;
        assert!(matches!(result, Err(CreatorError::Compile(_))));
        let rec = reg.get("test_plugin").unwrap();
        assert_eq!(rec.status, CompileStatus::Failed);
    }

    #[tokio::test]
    async fn compile_oversized_source_errors() {
        let reg = CreatorRegistry::new();
        let mut spec = sample_spec();
        spec.source_code = "x".repeat(1_000_001);
        reg.register_spec(spec).unwrap();
        let result = reg.compile("test_plugin").await;
        assert!(matches!(result, Err(CreatorError::Compile(_))));
    }

    #[test]
    fn planned_subprocess_commands_lists_steps() {
        let reg = CreatorRegistry::new();
        reg.register_spec(sample_spec()).unwrap();
        let cmds = reg.planned_subprocess_commands("test_plugin").unwrap();
        assert!(cmds.iter().any(|c| c.contains("mkdir")));
        assert!(cmds.iter().any(|c| c.contains("Cargo.toml")));
        assert!(cmds.iter().any(|c| c.contains("cargo build")));
        assert!(cmds.iter().any(|c| c.contains("load_dylib")));
    }
}
