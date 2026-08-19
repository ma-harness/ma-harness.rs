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
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::tool::ToolRegistry;

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
}

/// Creator registry (P9-2)
#[derive(Default)]
pub struct CreatorRegistry {
    plugins: Arc<Mutex<HashMap<String, PluginRecord>>>,
    /// 编译产物目录 (v2 真编译时用)
    pub output_dir: PathBuf,
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

    /// 编译 (P10-1 / Day 101)
    ///
    /// v1 简化: 标 Loaded (不真编译)
    /// v2 真编译: 写 plugin 到 `<output_dir>/<name>/Cargo.toml` + `src/lib.rs`,
    /// 调 `cargo build --release`, 产物 `.so`/`.dll` 走 libloading 加载.
    ///
    /// 当前 v2 简化: 检查 source_code 不空 + 长度 < 1MB, 标 Loaded.
    /// 业务方真要 dynamic plugin 编译需 P10-1.5 (rustc subprocess).
    pub async fn compile(&self, name: &str) -> Result<CompileStatus, CreatorError> {
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

        // v1: 标 Loaded (P10-1.5 改真编译)
        record.status = CompileStatus::Loaded;
        record.compile_error = None;
        Ok(CompileStatus::Loaded)
    }

    /// 加载到 ToolRegistry (P9-2 v1 简化: 占位, v2 libloading)
    pub fn load_into(&self, _name: &str, _registry: &ToolRegistry) -> Result<(), CreatorError> {
        // v1: 简化, 仅 mark as loaded
        Ok(())
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
        let record = self.get(name).ok_or_else(|| CreatorError::NotFound(name.to_string()))?;
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
            record.spec.name,
            record.spec.source_code,
            record.spec.name,
            record.spec.entry_fn,
        );
        let commands = vec![
            format!("mkdir -p {}", src_dir.display()),
            format!("write {} ({} bytes)", dir.join("Cargo.toml").display(), cargo_toml.len()),
            format!("write {} ({} bytes)", src_dir.join("lib.rs").display(), lib_rs.len()),
            format!("cargo build --release --manifest-path={}/Cargo.toml", dir.display()),
            format!(
                "load_dylib: {}/target/release/lib{}.so (or .dll on Windows)",
                dir.display(),
                record.spec.name.replace('-', "_")
            ),
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

    /// 端到端: 业务方 (model) 推 spec, 编译, 加载到 ToolRegistry
    ///
    /// 简化版 v1: register_spec + compile + load_into 一气呵成
    pub async fn create_and_load(
        &self,
        spec: PluginSpec,
        registry: &ToolRegistry,
    ) -> Result<String, CreatorError> {
        let name = spec.name.clone();
        let id = self.registry.register_spec(spec)?;
        self.registry.compile(&name).await?;
        self.registry.load_into(&name, registry)?;
        Ok(id)
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
        let factory = CreatorFactory::new();
        let log = crate::log::EventLog::open_in_memory().unwrap();
        let reg = ToolRegistry::new();
        // Just verify the reg is alive, not actually load
        let _ = log;
        let id = factory.create_and_load(sample_spec(), &reg).await.unwrap();
        assert!(!id.is_empty());
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
