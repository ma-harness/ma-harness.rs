//! Profile 隔离 (P10-3 / Day 101)
//!
//! 业务方可以定义多个 profile, 每个 profile 是独立的 tool/approval/model 配置.
//! 跟 dsh profiles 设计对齐 (dsh 走 ~/.ma-harness/profiles/*.toml).
//!
//! ## 用法
//!
//! ```ignore
//! use ma_harness_core::profile::{Profile, ProfileStore};
//!
//! let store = ProfileStore::new("./profiles");
//! let default = Profile::default_dev(); // 系统默认 (开发模式)
//! store.save("default", &default);
//!
//! let prod = Profile::default_prod();
//! store.save("prod", &prod);
//!
//! // 切 profile
//! let active = store.load("prod")?;
//! // 把 active.approval_policy 装到 ctx
//! ```
//!
//! ## 字段
//!
//! 每个 profile 包含:
//! - `name`: profile 名 (e.g. "default", "prod", "experimental")
//! - `description`: 一句话说明
//! - `operating_mode`: Default / Minimal / PTC / Creator
//! - `approval_policy`: Never / Ask / Always / Whitelist { tools }
//! - `enabled_plugins`: 启用的 plugin 名列表
//! - `default_model`: 默认 model (e.g. "openai:gpt-4o-mini")
//! - `agents_md`: 自定义 AGENTS.md 路径 (override 默认搜索)
//! - `max_tool_calls_per_turn`: override
//! - `compression_policy`: override
//!
//! ## 持久化
//!
//! 走 JSON 文件 (简单, 跨平台, 易手工编辑). v2 可选 TOML 跟 dsh 对齐.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{agent_compress::CompressionPolicy, operating_mode::OperatingMode};

/// 单 profile 配置 (P10-3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// profile 名 (e.g. "default")
    pub name: String,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 操作模式
    #[serde(default)]
    pub operating_mode: OperatingMode,
    /// 审批策略 (跟 cordis::ApprovalPolicy 对齐, 这里简化 string)
    #[serde(default)]
    pub approval_policy: String,
    /// 启用 plugins
    #[serde(default)]
    pub enabled_plugins: HashSet<String>,
    /// 默认 model (model prefix syntax: "openai:gpt-4o" / "anthropic:claude" / "stub")
    #[serde(default = "default_model_string")]
    pub default_model: String,
    /// AGENTS.md 自定义路径 (None = 走默认搜索)
    #[serde(default)]
    pub agents_md_path: Option<PathBuf>,
    /// max tool calls per turn
    #[serde(default)]
    pub max_tool_calls_per_turn: Option<u32>,
    /// 压缩策略
    #[serde(default)]
    pub compression_policy: Option<CompressionPolicy>,
    /// system prompt 额外 suffix
    #[serde(default)]
    pub system_prompt_suffix: String,
}

fn default_model_string() -> String {
    "stub".to_string()
}

impl Profile {
    /// 默认开发 profile (Default mode, Ask 审批, 7 first-party plugins, stub model)
    pub fn default_dev() -> Self {
        let mut plugins = HashSet::new();
        plugins.insert("hello".to_string());
        plugins.insert("bash".to_string());
        plugins.insert("fs".to_string());
        plugins.insert("web".to_string());
        plugins.insert("subagent".to_string());
        plugins.insert("skill".to_string());
        plugins.insert("cordis".to_string());
        Self {
            name: "default".to_string(),
            description: "Default development profile (full features, all first-party plugins)"
                .to_string(),
            operating_mode: OperatingMode::Default,
            approval_policy: "Ask".to_string(),
            enabled_plugins: plugins,
            default_model: "stub".to_string(),
            agents_md_path: None,
            max_tool_calls_per_turn: None,
            compression_policy: None,
            system_prompt_suffix: String::new(),
        }
    }

    /// 默认生产 profile (Default mode + 严格审批 + 不装所有 plugins)
    pub fn default_prod() -> Self {
        let mut plugins = HashSet::new();
        plugins.insert("fs".to_string());
        plugins.insert("bash".to_string());
        Self {
            name: "prod".to_string(),
            description: "Production profile (strict approval, minimal plugins)".to_string(),
            operating_mode: OperatingMode::Default,
            approval_policy: "Always".to_string(),
            enabled_plugins: plugins,
            default_model: "openai:gpt-4o-mini".to_string(),
            agents_md_path: None,
            max_tool_calls_per_turn: Some(5),
            compression_policy: Some(CompressionPolicy::SlidingWindow { keep_last_n: 50 }),
            system_prompt_suffix:
                "You are running in production. Be extra careful with file operations.".to_string(),
        }
    }

    /// 默认 Minimal profile (无 plugins, 纯 LLM 调)
    pub fn default_minimal() -> Self {
        Self {
            name: "minimal".to_string(),
            description: "Minimal profile (no plugins, pure LLM chat)".to_string(),
            operating_mode: OperatingMode::Minimal,
            approval_policy: "Never".to_string(),
            enabled_plugins: HashSet::new(),
            default_model: "stub".to_string(),
            agents_md_path: None,
            max_tool_calls_per_turn: Some(0),
            compression_policy: None,
            system_prompt_suffix: String::new(),
        }
    }

    /// 默认 PTC profile (单轮多 tool 调)
    pub fn default_ptc() -> Self {
        let mut plugins = HashSet::new();
        plugins.insert("fs".to_string());
        plugins.insert("bash".to_string());
        plugins.insert("web".to_string());
        Self {
            name: "ptc".to_string(),
            description: "PTC profile (persistent tool calling, multi-tool per turn)".to_string(),
            operating_mode: OperatingMode::Ptc,
            approval_policy: "Ask".to_string(),
            enabled_plugins: plugins,
            default_model: "openai:gpt-4o".to_string(),
            agents_md_path: None,
            max_tool_calls_per_turn: Some(20),
            compression_policy: Some(CompressionPolicy::SlidingWindow { keep_last_n: 100 }),
            system_prompt_suffix: "You are in PTC mode. Call multiple tools per turn without intermediate confirmation."
                .to_string(),
        }
    }
}

/// Profile store (P10-3)
pub struct ProfileStore {
    /// profiles 目录
    dir: PathBuf,
    /// 内存缓存 (path -> Profile)
    cache: Mutex<std::collections::HashMap<String, Profile>>,
}

impl std::fmt::Debug for ProfileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileStore")
            .field("dir", &self.dir)
            .field("cached", &self.cache.lock().len())
            .finish()
    }
}

impl ProfileStore {
    /// 构造 (dir 不存在会自动创建)
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = std::fs::create_dir_all(&dir);
        Self {
            dir,
            cache: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 保存 profile 到磁盘 (JSON)
    pub fn save(&self, profile: &Profile) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path_for(&profile.name);
        let json = serde_json::to_string_pretty(profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, json)?;
        self.cache
            .lock()
            .insert(profile.name.clone(), profile.clone());
        Ok(())
    }

    /// 加载 profile (先看缓存, 没再从磁盘)
    pub fn load(&self, name: &str) -> std::io::Result<Profile> {
        if let Some(p) = self.cache.lock().get(name).cloned() {
            return Ok(p);
        }
        let path = self.path_for(name);
        let json = std::fs::read_to_string(&path)?;
        let profile: Profile = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.cache.lock().insert(name.to_string(), profile.clone());
        Ok(profile)
    }

    /// 列所有 profile 名
    pub fn list(&self) -> std::io::Result<Vec<String>> {
        let mut names = Vec::new();
        if !self.dir.exists() {
            return Ok(names);
        }
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// 删 profile
    pub fn delete(&self, name: &str) -> std::io::Result<()> {
        let path = self.path_for(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.cache.lock().remove(name);
        Ok(())
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }
}

/// 默认 profile store 路径 (~/.ma-harness/profiles)
pub fn default_profile_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".ma-harness").join("profiles")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, ProfileStore) {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn default_dev_has_seven_plugins() {
        let p = Profile::default_dev();
        assert_eq!(p.enabled_plugins.len(), 7);
        assert!(p.enabled_plugins.contains("hello"));
        assert_eq!(p.operating_mode, OperatingMode::Default);
    }

    #[test]
    fn default_prod_is_strict() {
        let p = Profile::default_prod();
        assert_eq!(p.approval_policy, "Always");
        assert_eq!(p.enabled_plugins.len(), 2);
        assert!(p.max_tool_calls_per_turn.is_some());
    }

    #[test]
    fn default_minimal_no_plugins() {
        let p = Profile::default_minimal();
        assert_eq!(p.operating_mode, OperatingMode::Minimal);
        assert!(p.enabled_plugins.is_empty());
        assert_eq!(p.approval_policy, "Never");
    }

    #[test]
    fn default_ptc_allows_multi_tool() {
        let p = Profile::default_ptc();
        assert_eq!(p.operating_mode, OperatingMode::Ptc);
        assert!(p.max_tool_calls_per_turn.unwrap() >= 10);
    }

    #[test]
    fn save_and_load_round_trip() {
        let (_dir, store) = temp_store();
        let p = Profile::default_dev();
        store.save(&p).unwrap();
        let loaded = store.load("default").unwrap();
        assert_eq!(loaded.name, "default");
        assert_eq!(loaded.enabled_plugins.len(), 7);
    }

    #[test]
    fn list_returns_saved_profiles() {
        let (_dir, store) = temp_store();
        store.save(&Profile::default_dev()).unwrap();
        store.save(&Profile::default_prod()).unwrap();
        store.save(&Profile::default_minimal()).unwrap();
        let names = store.list().unwrap();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"default".to_string()));
        assert!(names.contains(&"prod".to_string()));
        assert!(names.contains(&"minimal".to_string()));
    }

    #[test]
    fn delete_removes_profile() {
        let (_dir, store) = temp_store();
        store.save(&Profile::default_dev()).unwrap();
        assert!(store.load("default").is_ok());
        store.delete("default").unwrap();
        assert!(store.load("default").is_err());
    }

    #[test]
    fn load_missing_returns_error() {
        let (_dir, store) = temp_store();
        assert!(store.load("nope").is_err());
    }

    #[test]
    fn profiles_serialize_to_valid_json() {
        let p = Profile::default_dev();
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("operating_mode"));
        assert!(json.contains("enabled_plugins"));
    }
}
