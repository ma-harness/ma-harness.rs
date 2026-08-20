//! # ma-harness Plugin Bundle (P11-8)
//!
//! 业务方一键装多个 plugin (类似 npm package collection).
//!
//! ## v1 简化
//!
//! - `bundle.toml` 文件列出 plugin 列表 + version constraint
//! - `mah bundle install <bundle>` 解析 → 调 registry 拉每个 plugin
//!
//! ## Bundle Manifest
//!
//! ```toml
//! # bundle.toml
//! [bundle]
//! name = "data-science"
//! version = "0.1.0"
//! description = "Data science tools"
//! author = "..."
//!
//! [[bundle.plugins]]
//! name = "bash"
//! version = "^1.0"
//!
//! [[bundle.plugins]]
//! name = "fs"
//! version = "^0.5"
//!
//! [[bundle.plugins]]
//! name = "vision"
//! version = ">= 2.0, < 3.0"
//! ```
//!
//! ## API
//!
//! ```rust
//! use ma_harness_bundle::{load_bundle_from_str, BundleManifest, bundle_summary};
//!
//! let toml = r#"
//! [bundle]
//! name = "data-science"
//! version = "0.1.0"
//!
//! [[bundle.plugins]]
//! name = "bash"
//! version = "^1.0"
//! "#;
//! let bundle: BundleManifest = load_bundle_from_str(toml).unwrap();
//! assert_eq!(bundle.name(), "data-science");
//! assert_eq!(bundle.plugin_count(), 1);
//! // 实际 resolve 需要 registry (有 plugin manifests 时才能解析)
//! // let resolved = bundle.resolve(&registry)?;
//! // println!("{}", bundle_summary(&bundle, &resolved));
//! ```

use ma_harness_registry::Registry;
use semver::VersionReq;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Bundle manifest
///
/// TOML 结构 (业务方写):
/// ```toml
/// [bundle]
/// name = "data-science"
/// version = "0.1.0"
/// description = "..."
/// author = "..."
///
/// [[bundle.plugins]]
/// name = "bash"
/// version = "^1.0"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleManifest {
    /// Bundle 元数据
    pub bundle: Bundle,
}

/// Bundle 核心字段
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bundle {
    /// Bundle 名
    pub name: String,
    /// Bundle version
    pub version: semver::Version,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 作者
    #[serde(default)]
    pub author: String,
    /// 包含的 plugins
    #[serde(default)]
    pub plugins: Vec<BundlePlugin>,
}

/// Bundle plugin spec
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundlePlugin {
    /// Plugin name (in registry)
    pub name: String,
    /// Version constraint (semver range, e.g. "^1.0", ">= 2.0, < 3.0", "0.1.0")
    pub version: String,
    /// Optional feature flag (业务方可以 skip)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

/// 解析后的 plugin (name + concrete version)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedPlugin {
    /// Plugin name
    pub name: String,
    /// Concrete resolved version
    pub version: semver::Version,
    /// Was it optional (info for reporting)
    pub optional: bool,
}

/// Bundle 错误
#[derive(Debug, Error)]
pub enum BundleError {
    /// Bundle 里 plugin 解析失败
    #[error("plugin {plugin}: {message}")]
    PluginNotFound {
        /// Plugin name
        plugin: String,
        /// 错误信息
        message: String,
    },
    /// 没有版本满足 constraint
    #[error("no version of {plugin} satisfies {constraint}")]
    NoMatchingVersion {
        /// Plugin name
        plugin: String,
        /// Version constraint
        constraint: String,
    },
    /// semver 错误
    #[error("semver error: {0}")]
    Semver(#[from] semver::Error),
    /// IO 错误
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// 解析错误 (TOML)
    #[error("parse error: {0}")]
    Parse(#[from] toml::de::Error),
    /// JSON 错误 (lockfile)
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Bundle 结果类型
pub type Result<T> = std::result::Result<T, BundleError>;

impl BundleManifest {
    /// 解析 bundle (按 registry 找每个 plugin 满足 constraint 的最新 version)
    pub fn resolve(&self, registry: &Registry) -> Result<Vec<ResolvedPlugin>> {
        self.bundle.resolve(registry)
    }
    /// Plugin 总数
    pub fn plugin_count(&self) -> usize {
        self.bundle.plugin_count()
    }
    /// 必选 plugin 数量
    pub fn required_count(&self) -> usize {
        self.bundle.required_count()
    }
    /// 可选 plugin 数量
    pub fn optional_count(&self) -> usize {
        self.bundle.optional_count()
    }
    /// 列出所有 missing (没在 registry 找到的) 必选 plugin
    pub fn missing_required(&self, registry: &Registry) -> Vec<String> {
        self.bundle.missing_required(registry)
    }
    /// 名字
    pub fn name(&self) -> &str {
        &self.bundle.name
    }
    /// 版本
    pub fn version(&self) -> &semver::Version {
        &self.bundle.version
    }
    /// 描述
    pub fn description(&self) -> &str {
        &self.bundle.description
    }
}

impl Bundle {
    /// 解析 bundle (按 registry 找每个 plugin 满足 constraint 的最新 version)
    ///
    /// 流程:
    /// 1. 对每个 `BundlePlugin`, parse version constraint (semver)
    /// 2. 调 `registry.list_versions(name)` 拿所有 version
    /// 3. 找满足 constraint 的最新 version
    /// 4. 必选 plugin 找不到 → 报错; 可选 plugin 找不到 → skip (返回 None)
    ///
    /// Returns 解析后的 plugin 列表 (按 bundle 顺序)
    pub fn resolve(&self, registry: &Registry) -> Result<Vec<ResolvedPlugin>> {
        let mut out = Vec::with_capacity(self.plugins.len());
        for p in &self.plugins {
            let req = VersionReq::parse(&p.version)?;
            let versions = registry.list_versions(&p.name);
            if versions.is_empty() {
                if p.optional {
                    continue; // skip optional
                }
                return Err(BundleError::PluginNotFound {
                    plugin: p.name.clone(),
                    message: format!("not in registry, required by bundle '{}'", self.name),
                });
            }
            // 找满足 constraint 的最新 version (registry 是按 version 升序排列)
            let resolved = versions
                .iter()
                .rev()
                .find(|m| req.matches(&m.version))
                .map(|m| ResolvedPlugin {
                    name: m.name.clone(),
                    version: m.version.clone(),
                    optional: p.optional,
                });
            match resolved {
                Some(r) => out.push(r),
                None => {
                    if p.optional {
                        continue;
                    }
                    return Err(BundleError::NoMatchingVersion {
                        plugin: p.name.clone(),
                        constraint: p.version.clone(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// 列出所有 missing (没在 registry 找到的) 必选 plugin
    pub fn missing_required(&self, registry: &Registry) -> Vec<String> {
        let mut out = Vec::new();
        for p in &self.plugins {
            if p.optional {
                continue;
            }
            if registry.list_versions(&p.name).is_empty() {
                out.push(p.name.clone());
            }
        }
        out
    }

    /// Plugin 总数
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// 必选 plugin 数量
    pub fn required_count(&self) -> usize {
        self.plugins.iter().filter(|p| !p.optional).count()
    }

    /// 可选 plugin 数量
    pub fn optional_count(&self) -> usize {
        self.plugins.iter().filter(|p| p.optional).count()
    }
}

/// 从 TOML 文件加载 bundle
pub fn load_bundle_from_file(path: impl AsRef<std::path::Path>) -> Result<BundleManifest> {
    let content = std::fs::read_to_string(path)?;
    let bundle: BundleManifest = toml::from_str(&content)?;
    Ok(bundle)
}

/// 从 TOML 字符串加载 bundle
pub fn load_bundle_from_str(s: &str) -> Result<BundleManifest> {
    let bundle: BundleManifest = toml::from_str(s)?;
    Ok(bundle)
}

/// 摘要 (printable)
pub fn bundle_summary(bundle: &BundleManifest, resolved: &[ResolvedPlugin]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Bundle: {} v{} ({})\n",
        bundle.name(),
        bundle.version(),
        bundle.description()
    ));
    out.push_str(&format!(
        "  Plugins: {} ({} required, {} optional)\n",
        bundle.plugin_count(),
        bundle.required_count(),
        bundle.optional_count()
    ));
    out.push_str("  Resolved:\n");
    for r in resolved {
        let opt = if r.optional { " (optional)" } else { "" };
        out.push_str(&format!("    - {} @ {}{}\n", r.name, r.version, opt));
    }
    out
}

// ============================================================================
// P12-7 v2: Lockfile (业务方 reproducible install)
// ============================================================================

/// Lockfile entry: 业务方 lock 的 plugin version (P12-7 v2)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockEntry {
    /// Plugin name
    pub name: String,
    /// Locked version (concrete, not constraint)
    pub version: semver::Version,
    /// 原始 constraint (业务方 debug 用)
    pub constraint: String,
    /// 可选 flag (业务方 debug 用)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

/// Lockfile (P12-7 v2)
///
/// 业务方 `mah bundle install` 时:
/// 1. 解析 bundle.toml (constraint 版本)
/// 2. 调 registry 找满足 constraint 的 latest version
/// 3. 写 bundle.lock (concrete versions, 保证下次 install 同一版本)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleLock {
    /// 锁定的 bundle 名
    pub bundle_name: String,
    /// 锁定的 bundle version
    pub bundle_version: semver::Version,
    /// 业务方 lock 时间 (unix epoch 秒)
    #[serde(default)]
    pub locked_at: u64,
    /// 业务方 lock 的 plugin 列表
    pub plugins: Vec<LockEntry>,
}

impl BundleLock {
    /// 从 bundle + resolved plugins 构造 lockfile
    pub fn from_resolved(
        bundle: &BundleManifest,
        resolved: &[ResolvedPlugin],
        locked_at: u64,
    ) -> Self {
        let plugins = resolved
            .iter()
            .map(|r| {
                // 业务方从 bundle.bundle.plugins 找原 constraint
                let constraint = bundle
                    .bundle
                    .plugins
                    .iter()
                    .find(|p| p.name == r.name)
                    .map(|p| p.version.clone())
                    .unwrap_or_else(|| r.version.to_string());
                LockEntry {
                    name: r.name.clone(),
                    version: r.version.clone(),
                    constraint,
                    optional: r.optional,
                }
            })
            .collect();
        Self {
            bundle_name: bundle.name().to_string(),
            bundle_version: bundle.version().clone(),
            locked_at,
            plugins,
        }
    }

    /// 业务方按 name 找 lock entry
    pub fn get(&self, name: &str) -> Option<&LockEntry> {
        self.plugins.iter().find(|e| e.name == name)
    }

    /// 业务方按 name 找 concrete version
    pub fn get_version(&self, name: &str) -> Option<&semver::Version> {
        self.get(name).map(|e| &e.version)
    }

    /// 业务方 lockfile entries 数
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// 业务方 lockfile 是否空
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// 业务方序列化到 JSON file
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// 业务方从 JSON file 加载
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let lock: Self = serde_json::from_str(&content)?;
        Ok(lock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_registry::{PluginManifest, PluginSource};
    use chrono::Utc;

    fn sample_manifest(name: &str, version: &str) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: version.parse().unwrap(),
            description: format!("{name} test"),
            author: "test".to_string(),
            source: PluginSource::Local(format!("./{name}")),
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn registry_with_plugins() -> Registry {
        let mut reg = Registry::open_in_memory();
        reg.publish(sample_manifest("bash", "1.0.0")).unwrap();
        reg.publish(sample_manifest("bash", "1.5.0")).unwrap();
        reg.publish(sample_manifest("bash", "2.0.0")).unwrap();
        reg.publish(sample_manifest("fs", "0.5.0")).unwrap();
        reg.publish(sample_manifest("fs", "0.6.0")).unwrap();
        reg.publish(sample_manifest("vision", "2.1.0")).unwrap();
        reg
    }

    #[test]
    fn load_bundle_from_str_minimal() {
        let toml = r#"
[bundle]
name = "minimal"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "^1.0"
"#;
        let b = load_bundle_from_str(toml).unwrap();
        assert_eq!(b.name(), "minimal");
        assert_eq!(b.version().to_string(), "0.1.0");
        assert_eq!(b.bundle.plugins.len(), 1);
        assert_eq!(b.bundle.plugins[0].name, "bash");
    }

    #[test]
    fn load_bundle_from_str_full() {
        let toml = r#"
[bundle]
name = "data-science"
version = "0.2.0"
description = "Data science tools"
author = "alice"

[[bundle.plugins]]
name = "bash"
version = "^1.0"

[[bundle.plugins]]
name = "fs"
version = "^0.5"

[[bundle.plugins]]
name = "vision"
version = ">= 2.0, < 3.0"
optional = true
"#;
        let b = load_bundle_from_str(toml).unwrap();
        assert_eq!(b.name(), "data-science");
        assert_eq!(b.bundle.plugins.len(), 3);
        assert_eq!(b.bundle.plugins[0].name, "bash");
        assert!(!b.bundle.plugins[0].optional);
        assert!(b.bundle.plugins[2].optional);
        assert_eq!(b.plugin_count(), 3);
        assert_eq!(b.required_count(), 2);
        assert_eq!(b.optional_count(), 1);
    }

    #[test]
    fn resolve_caret_constraint() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "^1.0"
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        assert_eq!(resolved.len(), 1);
        // ^1.0 matches 1.x.x, latest 1.x is 1.5.0
        assert_eq!(resolved[0].version.to_string(), "1.5.0");
    }

    #[test]
    fn resolve_tilde_constraint() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "~1.5"
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        assert_eq!(resolved[0].version.to_string(), "1.5.0");
    }

    #[test]
    fn resolve_range_constraint() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "vision"
version = ">= 2.0, < 3.0"
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        assert_eq!(resolved[0].version.to_string(), "2.1.0");
    }

    #[test]
    fn resolve_exact_version() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "=2.0.0"
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        assert_eq!(resolved[0].version.to_string(), "2.0.0");
    }

    #[test]
    fn resolve_missing_required_errors() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "nonexistent"
version = "*"
"#,
        )
        .unwrap();
        let err = b.resolve(&reg).unwrap_err();
        assert!(matches!(err, BundleError::PluginNotFound { .. }));
    }

    #[test]
    fn resolve_optional_missing_skipped() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "^1.0"

[[bundle.plugins]]
name = "extras"
version = "*"
optional = true
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        // 只 bash, extras optional skip
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "bash");
    }

    #[test]
    fn resolve_no_matching_version_errors() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = ">= 3.0"
"#,
        )
        .unwrap();
        let err = b.resolve(&reg).unwrap_err();
        assert!(matches!(err, BundleError::NoMatchingVersion { .. }));
    }

    #[test]
    fn resolve_empty_registry() {
        let reg = Registry::open_in_memory();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "*"
"#,
        )
        .unwrap();
        let err = b.resolve(&reg).unwrap_err();
        assert!(matches!(err, BundleError::PluginNotFound { .. }));
    }

    #[test]
    fn missing_required_lists_required_only() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "t"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "^1.0"

[[bundle.plugins]]
name = "required-missing"
version = "*"

[[bundle.plugins]]
name = "optional-missing"
version = "*"
optional = true
"#,
        )
        .unwrap();
        let missing = b.missing_required(&reg);
        assert_eq!(missing, vec!["required-missing"]);
    }

    #[test]
    fn load_bundle_from_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bundle.toml");
        std::fs::write(
            &path,
            r#"
[bundle]
name = "test"
version = "0.1.0"
description = "test bundle"

[[bundle.plugins]]
name = "bash"
version = "^1.0"
"#,
        )
        .unwrap();
        let b = load_bundle_from_file(&path).unwrap();
        assert_eq!(b.name(), "test");
        assert_eq!(b.description(), "test bundle");
    }

    #[test]
    fn bundle_summary_format() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "summary-test"
version = "0.5.0"
description = "for testing"

[[bundle.plugins]]
name = "bash"
version = "^1.0"
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        let s = bundle_summary(&b, &resolved);
        assert!(s.contains("summary-test v0.5.0"));
        assert!(s.contains("for testing"));
        assert!(s.contains("bash @ 1.5.0"));
    }

    // === P12-7 v2: lockfile tests ===

    #[test]
    fn lockfile_from_resolved() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "lock-test"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "^1.0"

[[bundle.plugins]]
name = "fs"
version = "^0.5"
optional = true
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        let lock = BundleLock::from_resolved(&b, &resolved, 1234567890);

        assert_eq!(lock.bundle_name, "lock-test");
        assert_eq!(lock.bundle_version.to_string(), "0.1.0");
        assert_eq!(lock.locked_at, 1234567890);
        assert_eq!(lock.len(), 2);
        assert_eq!(lock.plugins[0].name, "bash");
        assert_eq!(lock.plugins[0].version.to_string(), "1.5.0");
        assert_eq!(lock.plugins[0].constraint, "^1.0");
        assert!(!lock.plugins[0].optional);
        assert!(lock.plugins[1].optional);
    }

    #[test]
    fn lockfile_save_load_roundtrip() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "lock-rt"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "^1.0"
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        let lock = BundleLock::from_resolved(&b, &resolved, 1234567890);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bundle.lock");
        lock.save(&path).unwrap();
        let loaded = BundleLock::load(&path).unwrap();

        assert_eq!(lock, loaded);
    }

    #[test]
    fn lockfile_get_version() {
        let reg = registry_with_plugins();
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "lock-get"
version = "0.1.0"

[[bundle.plugins]]
name = "bash"
version = "^1.0"
"#,
        )
        .unwrap();
        let resolved = b.resolve(&reg).unwrap();
        let lock = BundleLock::from_resolved(&b, &resolved, 0);

        let v = lock.get_version("bash").unwrap();
        assert_eq!(v.to_string(), "1.5.0");
        assert!(lock.get_version("nonexistent").is_none());
    }

    #[test]
    fn lockfile_empty_bundle() {
        let b = load_bundle_from_str(
            r#"
[bundle]
name = "empty"
version = "0.1.0"
"#,
        )
        .unwrap();
        let lock = BundleLock::from_resolved(&b, &[], 0);
        assert!(lock.is_empty());
        assert_eq!(lock.len(), 0);
    }
}
