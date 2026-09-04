//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-profile`
//! **Crate ident** (`use` 路径): `ma_harness_profile`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-profile = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_profile::{Profile, ProfileRegistry, ProfileLoader, builtin_profiles};
//!
//! // 业务方用 builtin 5 profile (web / headless / sdk / sdk-minimal / acp)
//! let mut registry = ProfileRegistry::new();
//! for profile in builtin_profiles() {
//!     registry.register(profile);
//! }
//!
//! // 业务方用自定义 profile (从 ~/.ma-harness/profiles/foo/cordis.yml 加载)
//! let loader = ProfileLoader::default();
//! let profile = loader.load_from_dir("~/.ma-harness/profiles/web")?;
//! registry.register(profile);
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-profile
//!
//! # 设计 (Design) — P14.9
//!
//! **目标**: 抽象 profile system (跟 dsh 5 shipped profiles 1:1 对等).
//! 业务方
//! - 用 `Profile` 描述一组 plugins + settings (类似 dsh `cordis.yml`)
//! - 用 `ProfileLoader` 从 `~/.ma-harness/profiles/<name>/` 目录加载
//! - 用 `ProfileRegistry` 存所有 profiles, 业务方 `mah --profile <name>` 选
//!
//! **背景**: 见 [dsh-feature-parity-table §5] "Profiles & Bundles". ma-harness 之前无 profile system.
//!
//! **核心抽象**:
//! - [`Profile`] struct (name / description / bundles / patches / settings)
//! - [`Bundle`] struct (name / version / plugins / settings)
//! - [`ProfileLoader`] (从 `cordis.yml` 读 Profile)
//! - [`ProfileRegistry`] (in-memory registry)
//! - [`builtin_profiles`] (5 default: web / headless / sdk / sdk-minimal / acp)
//!
//! **5 builtin profiles** (P14.9.1, 跟 dsh 1:1):
//! - `web` — Web UI (P15+ 实装, P14.9.1 占位)
//! - `headless` — one-shot runner (`mah run "task"`)
//! - `sdk` — SDK JSON-RPC server (`mah acp serve`)
//! - `sdk-minimal` — standalone SDK bundle (no `ma-harness-base`)
//! - `acp` — automation-only ACP server (跟 sdk 类似但只 automation)
//!
//! **6 质量属性**:
//! - 可复用: 业务方可注册自定义 profile, 5 default 跟 dsh 对齐
//! - 可维护: 模块化分块, profile / bundle / loader 集中 lib.rs
//! - 鲁棒: validate (name 非空 / bundles 非空), 解析错误显式
//! - 安全: 不 eval settings, 静态 string
//! - 可测: 6+ 测试覆盖 builtin / loader / registry
//! - 可扩展: Bundle layer 机制 (P14.9.2), plugin discovery (P15+)
//!
//! # 限制 (Limitations) — P14.9.1
//!
//! - CLI flag `--profile <name>` 留 P14.9.2 (在 ma-harness-cli 集成)
//! - Bundle layer 机制留 P14.9.2 (业务方可多 bundle 叠加 patch)
//! - Patches 解析 (cordis.yml `patches:` 段) 留 P14.9.2
//!
//! [dsh-feature-parity-table §5]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#5-profiles--bundles

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::path::Path;

use thiserror::Error;
use tokio::sync::Mutex;

// ============================================================================
// ProfileError
// ============================================================================

/// Profile capability 错误.
#[derive(Debug, Error)]
pub enum ProfileError {
    /// IO 错误
    #[error("profile I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML 解析错误
    #[error("profile YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// Profile 不存在
    #[error("profile not found: {0}")]
    NotFound(String),

    /// 验证失败
    #[error("profile validation failed: {0}")]
    Validation(String),
}

// ============================================================================
// Bundle
// ============================================================================

/// Bundle — Profile 的组成单元 (类似 dsh `cordis.bundle`).
///
/// **业务方场景**: `~/.ma-harness/bundles/ma-harness-base/0.1.1/bundle.yml`
/// 描述 plugin 集合 + 默认 settings, 多个 bundle 可被一个 profile 引用 (P14.9.2 layer).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Bundle {
    /// Bundle 名 (e.g. "ma-harness-base")
    pub name: String,
    /// Bundle 版本 (semver 字符串, e.g. "0.1.1")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 描述
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 要加载的 plugins (plugin crate name 列表)
    #[serde(default)]
    pub plugins: Vec<String>,
    /// Bundle 默认 settings
    #[serde(default)]
    pub settings: BTreeMap<String, serde_yaml::Value>,
}

impl Bundle {
    /// 创建一个新 Bundle
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            description: None,
            plugins: Vec::new(),
            settings: BTreeMap::new(),
        }
    }

    /// 设置 version
    pub fn with_version(mut self, v: impl Into<String>) -> Self {
        self.version = Some(v.into());
        self
    }

    /// 设置 description
    pub fn with_description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    /// 加 plugin
    pub fn with_plugin(mut self, plugin: impl Into<String>) -> Self {
        self.plugins.push(plugin.into());
        self
    }

    /// 加 setting
    pub fn with_setting(mut self, key: impl Into<String>, value: serde_yaml::Value) -> Self {
        self.settings.insert(key.into(), value);
        self
    }
}

// ============================================================================
// Profile
// ============================================================================

/// Profile — 业务方选的一组 bundles + 自定义 settings + patches.
///
/// **业务方 YAML 格式** (`cordis.yml` in `~/.ma-harness/profiles/<name>/`):
/// ```yaml
/// name: web
/// description: Web UI (browser app at :3080)
/// bundles:
///   - name: ma-harness-base
///     version: 0.1.1
///   - name: ma-harness-web-app
///     version: 0.1.0
/// patches: []  # P14.9.2 占位
/// settings:
///   web.bind: 0.0.0.0:3080
///   log.level: info
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    /// Profile 名 (跟 dsh `web` / `headless` / `sdk` / `sdk-minimal` / `acp` 对齐)
    pub name: String,
    /// 一句话描述
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 组成 bundles (按顺序, P14.9.2 layer 机制)
    #[serde(default)]
    pub bundles: Vec<Bundle>,
    /// 自定义 patches (cordis.yml `patches:` 段, P14.9.2 实现)
    #[serde(default)]
    pub patches: Vec<serde_yaml::Value>,
    /// Profile 级别 settings
    #[serde(default)]
    pub settings: BTreeMap<String, serde_yaml::Value>,
}

impl Profile {
    /// 创建一个新 Profile
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            bundles: Vec::new(),
            patches: Vec::new(),
            settings: BTreeMap::new(),
        }
    }

    /// 设置 description
    pub fn with_description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    /// 加 bundle
    pub fn with_bundle(mut self, bundle: Bundle) -> Self {
        self.bundles.push(bundle);
        self
    }

    /// 加 plugin 进首个 bundle (业务方 shorthand)
    pub fn with_plugin(mut self, plugin: impl Into<String>) -> Self {
        if self.bundles.is_empty() {
            self.bundles.push(Bundle::new("default"));
        }
        if let Some(b) = self.bundles.last_mut() {
            b.plugins.push(plugin.into());
        }
        self
    }

    /// 加 setting
    pub fn with_setting(mut self, key: impl Into<String>, value: serde_yaml::Value) -> Self {
        self.settings.insert(key.into(), value);
        self
    }

    /// 验证 profile (name 非空, 至少 1 bundle 或自定义 settings)
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.name.trim().is_empty() {
            return Err(ProfileError::Validation("name is empty".into()));
        }
        if self.bundles.is_empty() && self.settings.is_empty() {
            return Err(ProfileError::Validation(format!(
                "profile '{}' has no bundles and no settings, nothing to load",
                self.name
            )));
        }
        Ok(())
    }

    /// 业务方按 plugin name 查 bundle (第一个含该 plugin 的 bundle)
    pub fn find_bundle_with_plugin(&self, plugin: &str) -> Option<&Bundle> {
        self.bundles
            .iter()
            .find(|b| b.plugins.iter().any(|p| p == plugin))
    }
}

// ============================================================================
// ProfileLoader
// ============================================================================

/// Profile loader (从 cordis.yml 读 Profile).
///
/// **业务方用**: 业务方 CLI 启动时, 调 `loader.load_from_dir("~/.ma-harness/profiles/web")` 读 profile.
pub struct ProfileLoader {
    /// 是否要求 cordis.yml 必须存在
    require_explicit: bool,
}

impl Default for ProfileLoader {
    fn default() -> Self {
        Self {
            require_explicit: true,
        }
    }
}

impl ProfileLoader {
    /// 创建一个新 loader
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置是否要求 cordis.yml 必须存在 (false = directory 没文件不报错, 业务方用 default profile)
    pub fn with_require_explicit(mut self, b: bool) -> Self {
        self.require_explicit = b;
        self
    }

    /// 从 directory 加载 profile
    ///
    /// 业务方传 `~/.ma-harness/profiles/web` (或 `~/.ma-harness/profiles/web/cordis.yml` 直接).
    /// 文件名 `cordis.yml` 是 dsh 约定.
    pub async fn load_from_dir(&self, dir: impl AsRef<Path>) -> Result<Profile, ProfileError> {
        let path = dir.as_ref();
        // 业务方传 dir / yaml file 都行:
        // - 是 dir: 找 dir/cordis.yml
        // - 是 .yml / .yaml file: 直接用
        // - 都不是: 当成 dir 处理 (之后 yaml_path.exists() 会是 false)
        let yaml_path = if path.is_dir() {
            path.join("cordis.yml")
        } else {
            path.to_path_buf()
        };

        if !yaml_path.exists() {
            if self.require_explicit {
                return Err(ProfileError::NotFound(yaml_path.display().to_string()));
            }
            // require_explicit=false: 返回空 profile (业务方用 default)
            return Ok(Profile::new(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("default")
                    .to_string(),
            ));
        }

        let content = tokio::fs::read_to_string(&yaml_path).await?;
        let profile: Profile = serde_yaml::from_str(&content)?;
        profile.validate()?;
        tracing::debug!(profile = %profile.name, path = %yaml_path.display(), "profile loaded");
        Ok(profile)
    }

    /// 从 yaml string 直接 parse
    pub fn load_from_str(yaml: &str) -> Result<Profile, ProfileError> {
        let profile: Profile = serde_yaml::from_str(yaml)?;
        profile.validate()?;
        Ok(profile)
    }
}

// ============================================================================
// ProfileRegistry
// ============================================================================

/// Profile registry (in-memory, 业务方 CLI / runtime 用).
pub struct ProfileRegistry {
    profiles: Mutex<std::collections::HashMap<String, Profile>>,
}

impl ProfileRegistry {
    /// 创建一个空 registry
    pub fn new() -> Self {
        Self {
            profiles: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 注册一个 profile
    pub async fn register(&self, profile: Profile) {
        profile.validate().expect("profile validation");
        let mut profiles = self.profiles.lock().await;
        profiles.insert(profile.name.clone(), profile);
    }

    /// 按名拿 profile
    pub async fn get(&self, name: &str) -> Option<Profile> {
        let profiles = self.profiles.lock().await;
        profiles.get(name).cloned()
    }

    /// 列出所有 profile 名 (sorted)
    pub async fn list(&self) -> Vec<String> {
        let profiles = self.profiles.lock().await;
        let mut names: Vec<String> = profiles.keys().cloned().collect();
        names.sort();
        names
    }

    /// 数量
    pub async fn len(&self) -> usize {
        let profiles = self.profiles.lock().await;
        profiles.len()
    }

    /// 是否空
    pub async fn is_empty(&self) -> bool {
        let profiles = self.profiles.lock().await;
        profiles.is_empty()
    }
}

impl Default for ProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Builtin 5 profiles (P14.9.1: 跟 dsh 1:1 对齐)
// ============================================================================

/// 5 builtin profiles (跟 dsh 1:1 对齐: web / headless / sdk / sdk-minimal / acp).
///
/// **业务方用**: `let mut registry = ProfileRegistry::new(); for p in builtin_profiles() { registry.register(p).await; }`
pub fn builtin_profiles() -> Vec<Profile> {
    vec![
        // 1. web — Web UI (P15+ 实装, P14.9.1 占位)
        Profile::new("web")
            .with_description("Web UI (browser app at :3080) — P15+ implements")
            .with_bundle(
                Bundle::new("ma-harness-base")
                    .with_version("0.1.1")
                    .with_plugin("ma-harness-plugin-web"),
            )
            .with_setting("web.bind", serde_yaml::Value::String("0.0.0.0:3080".into())),
        // 2. headless — one-shot runner
        Profile::new("headless")
            .with_description("One-shot runner (no server, no UI)")
            .with_bundle(
                Bundle::new("ma-harness-base")
                    .with_version("0.1.1")
                    .with_plugin("ma-harness-plugin-bash"),
            ),
        // 3. sdk — SDK JSON-RPC server
        Profile::new("sdk")
            .with_description("SDK JSON-RPC server (interoperable with dsh)")
            .with_bundle(
                Bundle::new("ma-harness-base")
                    .with_version("0.1.1")
                    .with_plugin("ma-harness-plugin-bash"),
            )
            .with_setting("acp.transport", serde_yaml::Value::String("stdio".into())),
        // 4. sdk-minimal — standalone SDK bundle (no `ma-harness-base`)
        Profile::new("sdk-minimal")
            .with_description("Standalone SDK bundle (no ma-harness-base, minimal deps)")
            .with_bundle(
                Bundle::new("ma-harness-sdk-minimal")
                    .with_version("0.1.0")
                    .with_plugin("ma-harness-plugin-acp-minimal"),
            ),
        // 5. acp — automation-only ACP server
        Profile::new("acp")
            .with_description("Automation-only ACP server (no interactive TUI)")
            .with_bundle(
                Bundle::new("ma-harness-base")
                    .with_version("0.1.1")
                    .with_plugin("ma-harness-plugin-acp-automation"),
            )
            .with_setting("acp.automation", serde_yaml::Value::Bool(true)),
    ]
}

// ============================================================================
// 单元测试 (mod tests) — 7 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn builtin_profiles_returns_5_with_expected_names() {
        let profiles = builtin_profiles();
        assert_eq!(profiles.len(), 5);
        let names: Vec<String> = profiles.iter().map(|p| p.name.clone()).collect();
        assert_eq!(names, vec!["web", "headless", "sdk", "sdk-minimal", "acp"]);
    }

    #[test]
    fn profile_validate_rejects_empty_name() {
        let p = Profile::new("");
        assert!(p.validate().is_err());
    }

    #[test]
    fn profile_validate_rejects_no_bundles_no_settings() {
        let p = Profile::new("empty");
        let err = p.validate().unwrap_err();
        assert!(matches!(err, ProfileError::Validation(_)));
    }

    #[test]
    fn profile_load_from_str_roundtrip() {
        let yaml = r#"
name: my-profile
description: Test profile
bundles:
  - name: ma-harness-base
    version: 0.1.1
    plugins:
      - ma-harness-plugin-bash
  - name: ma-harness-extra
    plugins: []
settings:
  log.level: info
  web.bind: 0.0.0.0:8080
"#;
        let profile = ProfileLoader::load_from_str(yaml).expect("parse");
        assert_eq!(profile.name, "my-profile");
        assert_eq!(profile.bundles.len(), 2);
        assert_eq!(profile.bundles[0].name, "ma-harness-base");
        assert_eq!(profile.bundles[0].plugins.len(), 1);
        assert_eq!(
            profile.settings.get("log.level").unwrap().as_str(),
            Some("info")
        );
    }

    #[tokio::test]
    async fn profile_loader_reads_from_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("test-profile");
        std::fs::create_dir_all(&profile_dir).expect("mkdir");
        let yaml = r#"
name: test-profile
description: Loaded from dir
bundles:
  - name: ma-harness-base
    plugins: []
"#;
        std::fs::write(profile_dir.join("cordis.yml"), yaml).expect("write");

        let loader = ProfileLoader::new();
        let profile = loader.load_from_dir(&profile_dir).await.expect("load");
        assert_eq!(profile.name, "test-profile");
        assert_eq!(profile.description.as_deref(), Some("Loaded from dir"));
    }

    #[tokio::test]
    async fn profile_loader_missing_file_errors_with_require_explicit() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("nonexistent");
        let loader = ProfileLoader::new();
        let err = loader.load_from_dir(&profile_dir).await.unwrap_err();
        assert!(matches!(err, ProfileError::NotFound(_)));
    }

    #[tokio::test]
    async fn profile_registry_register_get_list() {
        let registry = ProfileRegistry::new();
        for p in builtin_profiles() {
            registry.register(p).await;
        }
        assert_eq!(registry.len().await, 5);

        let web = registry.get("web").await.expect("web");
        assert_eq!(web.name, "web");

        let list = registry.list().await;
        assert_eq!(list, vec!["acp", "headless", "sdk", "sdk-minimal", "web"]);
    }

    #[test]
    fn profile_find_bundle_with_plugin() {
        let profile = Profile::new("test")
            .with_bundle(Bundle::new("bundle-a").with_plugin("plugin-x"))
            .with_bundle(Bundle::new("bundle-b").with_plugin("plugin-y"));
        let found = profile.find_bundle_with_plugin("plugin-y").expect("found");
        assert_eq!(found.name, "bundle-b");
        assert!(profile.find_bundle_with_plugin("nonexistent").is_none());
    }
}
