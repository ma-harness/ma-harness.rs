//! # ma-harness Plugin Registry (P11-6)
//!
//! 业务方 publish / install / list 第三方 plugin 的 registry.
//!
//! ## v1 简化 (本地 JSON 文件)
//!
//! - 不连远程 server, 业务方 `mah plugin publish <manifest.json>` 写本地 registry
//! - 后续 v2: 接 GitHub Pages 静态 JSON, 远程 `mah plugin install <name>` 拉
//!
//! ## Plugin Manifest
//!
//! ```toml
//! # plugin.toml (业务方写)
//! [plugin]
//! name = "my-plugin"
//! version = "0.1.0"
//! description = "..."
//! author = "..."
//! source = "../path/to/plugin"  # 暂时 local path, v2 接 git url
//! ```
//!
//! ## API
//!
//! ```rust
//! use ma_harness_registry::{Registry, PluginManifest, PluginSource};
//! use chrono::Utc;
//!
//! let mut reg = Registry::open_in_memory();
//! reg.publish(PluginManifest {
//!     name: "my-plugin".to_string(),
//!     version: "0.1.0".parse().unwrap(),
//!     description: "My plugin".to_string(),
//!     author: "alice".to_string(),
//!     source: PluginSource::Local("../path".to_string()),
//!     tags: vec!["utility".to_string()],
//!     created_at: Utc::now(),
//!     updated_at: Utc::now(),
//! });
//!
//! let m = reg.get("my-plugin").unwrap();
//! assert_eq!(m.version.to_string(), "0.1.0");
//! ```

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Plugin source (P11-6 v1: 限 local, v2 接 git / http)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    /// 本地路径 (相对 registry root 或绝对)
    Local(String),
    /// Git URL (v2)
    Git {
        url: String,
        /// Optional commit / tag / branch
        rev: Option<String>,
    },
    /// HTTP tarball URL (v2)
    Http(String),
}

/// 手动 Serialize: 把每种 variant 序列化成对应的 JSON 形态
impl Serialize for PluginSource {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            PluginSource::Local(path) => {
                let mut s = serializer.serialize_struct("PluginSource", 2)?;
                s.serialize_field("type", "local")?;
                s.serialize_field("path", path)?;
                s.end()
            }
            PluginSource::Git { url, rev } => {
                let mut s = serializer.serialize_struct("PluginSource", 2)?;
                s.serialize_field("type", "git")?;
                s.serialize_field("url", url)?;
                if let Some(r) = rev {
                    s.serialize_field("rev", r)?;
                }
                s.end()
            }
            PluginSource::Http(url) => {
                let mut s = serializer.serialize_struct("PluginSource", 2)?;
                s.serialize_field("type", "http")?;
                s.serialize_field("url", url)?;
                s.end()
            }
        }
    }
}

/// 手动 Deserialize: 从 JSON 形态还原
impl<'de> Deserialize<'de> for PluginSource {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Type,
            Path,
            Url,
            Rev,
        }

        struct PluginSourceVisitor;

        impl<'de> Visitor<'de> for PluginSourceVisitor {
            type Value = PluginSource;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a PluginSource object")
            }

            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<PluginSource, M::Error> {
                let mut ty: Option<String> = None;
                let mut path: Option<String> = None;
                let mut url: Option<String> = None;
                let mut rev: Option<String> = None;
                while let Some(key) = map.next_key::<Field>()? {
                    match key {
                        Field::Type => ty = Some(map.next_value()?),
                        Field::Path => path = Some(map.next_value()?),
                        Field::Url => url = Some(map.next_value()?),
                        Field::Rev => rev = Some(map.next_value()?),
                    }
                }
                let ty = ty.ok_or_else(|| de::Error::missing_field("type"))?;
                match ty.as_str() {
                    "local" => Ok(PluginSource::Local(
                        path.ok_or_else(|| de::Error::missing_field("path"))?,
                    )),
                    "git" => Ok(PluginSource::Git {
                        url: url.ok_or_else(|| de::Error::missing_field("url"))?,
                        rev,
                    }),
                    "http" => Ok(PluginSource::Http(
                        url.ok_or_else(|| de::Error::missing_field("url"))?,
                    )),
                    other => Err(de::Error::custom(format!(
                        "unknown PluginSource type: {other}"
                    ))),
                }
            }
        }

        deserializer.deserialize_map(PluginSourceVisitor)
    }
}

/// Plugin manifest (业务方写 plugin.toml 或 manifest.json)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    /// Plugin 名 (unique key in registry)
    pub name: String,
    /// Semantic version (e.g. "0.1.0")
    pub version: Version,
    /// 人类描述
    pub description: String,
    /// 作者
    pub author: String,
    /// 来源 (local / git / http)
    pub source: PluginSource,
    /// 标签 (search 用, e.g. ["utility", "vision"])
    #[serde(default)]
    pub tags: Vec<String>,
    /// 创建时间 (registry 自动填, 业务方不用写)
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    /// 更新时间 (registry 自动填)
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

/// Registry 错误
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Plugin 已存在 (publish 时)
    #[error("plugin already exists: {0} (use `update` to overwrite)")]
    AlreadyExists(String),
    /// Plugin 不存在 (get / install 时)
    #[error("plugin not found: {0}")]
    NotFound(String),
    /// 同名 plugin 版本冲突
    #[error("version conflict: {0} already has version {1}, new version must be greater")]
    VersionConflict(String, Version),
    /// IO 错误
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON 错误
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// semver 错误
    #[error("semver error: {0}")]
    Semver(#[from] semver::Error),
}

/// Registry 结果类型
pub type Result<T> = std::result::Result<T, RegistryError>;

/// Plugin Registry
///
/// v1: in-memory + 序列化到 JSON 文件
/// v2: 接 HTTP / GitHub Pages
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registry {
    /// plugins 按 name 索引, value 是该 name 的所有 version
    plugins: BTreeMap<String, Vec<PluginManifest>>,
}

impl Registry {
    /// 构造空 registry
    pub fn new() -> Self {
        Self::default()
    }

    /// 打开本地 JSON registry (业务方 ~/.ma-harness/registry.json)
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        if !path.as_ref().exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        if content.trim().is_empty() {
            return Ok(Self::new());
        }
        let reg: Self = serde_json::from_str(&content)?;
        Ok(reg)
    }

    /// 打开 in-memory (测试用)
    pub fn open_in_memory() -> Self {
        Self::new()
    }

    /// 持久化到 JSON 文件
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Publish 一个 plugin
    ///
    /// - name 不存在 → 直接 add
    /// - name 存在 + version 更大 → append 到 version list
    /// - name 存在 + version 更小或相等 → VersionConflict
    pub fn publish(&mut self, manifest: PluginManifest) -> Result<()> {
        let name = manifest.name.clone();
        let versions = self.plugins.entry(name.clone()).or_default();

        if let Some(latest) = versions.last() {
            if manifest.version <= latest.version {
                return Err(RegistryError::VersionConflict(name, latest.version.clone()));
            }
        }

        versions.push(manifest);
        Ok(())
    }

    /// 强制覆盖 (update)
    pub fn upsert(&mut self, manifest: PluginManifest) -> Result<()> {
        let name = manifest.name.clone();
        let versions = self.plugins.entry(name).or_default();
        // 移除同 version
        versions.retain(|m| m.version != manifest.version);
        versions.push(manifest);
        Ok(())
    }

    /// Get latest version of plugin by name
    pub fn get(&self, name: &str) -> Option<&PluginManifest> {
        self.plugins.get(name).and_then(|v| v.last())
    }

    /// Get specific version
    pub fn get_version(&self, name: &str, version: &Version) -> Option<&PluginManifest> {
        self.plugins
            .get(name)
            .and_then(|v| v.iter().find(|m| &m.version == version))
    }

    /// List all plugins (latest version only)
    pub fn list(&self) -> Vec<&PluginManifest> {
        self.plugins.values().filter_map(|v| v.last()).collect()
    }

    /// List all versions of a plugin
    pub fn list_versions(&self, name: &str) -> Vec<&PluginManifest> {
        self.plugins
            .get(name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Search by tag
    pub fn search_by_tag(&self, tag: &str) -> Vec<&PluginManifest> {
        self.list()
            .into_iter()
            .filter(|m| m.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// Total plugin count (latest versions)
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Total version count (across all plugins)
    pub fn version_count(&self) -> usize {
        self.plugins.values().map(|v| v.len()).sum()
    }

    /// 移除 plugin (所有版本)
    pub fn remove(&mut self, name: &str) -> Result<()> {
        self.plugins
            .remove(name)
            .ok_or_else(|| RegistryError::NotFound(name.to_string()))?;
        Ok(())
    }

    /// P12-5: 按 author 搜索
    pub fn search_by_author(&self, author: &str) -> Vec<&PluginManifest> {
        self.list()
            .into_iter()
            .filter(|m| m.author == author)
            .collect()
    }

    /// P12-5: 按 name 模糊搜索 (case-insensitive substring)
    pub fn search_by_name(&self, query: &str) -> Vec<&PluginManifest> {
        let q = query.to_lowercase();
        self.list()
            .into_iter()
            .filter(|m| m.name.to_lowercase().contains(&q))
            .collect()
    }

    /// P12-5: 列出所有 author (去重)
    pub fn list_authors(&self) -> Vec<String> {
        let mut authors: Vec<String> = self
            .plugins
            .values()
            .filter_map(|v| v.last())
            .map(|m| m.author.clone())
            .collect();
        authors.sort();
        authors.dedup();
        authors
    }

    /// P12-5: 列出所有 tag (去重, 跨所有 plugin)
    pub fn list_all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .plugins
            .values()
            .filter_map(|v| v.last())
            .flat_map(|m| m.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// P12-5: 导出 registry 到 JSON file (供业务方发布到 GitHub Pages 静态站)
    pub fn export(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// P12-5: 合并另一个 registry (业务方从多个 source 合并)
    pub fn merge(&mut self, other: Registry) -> Result<()> {
        for (name, manifests) in other.plugins {
            for m in manifests {
                // 跳过已存在的同 version
                let existing_versions: Vec<_> = self
                    .plugins
                    .get(&name)
                    .map(|v| v.iter().map(|x| x.version.clone()).collect())
                    .unwrap_or_default();
                if existing_versions.contains(&m.version) {
                    continue;
                }
                self.publish(m)?;
            }
        }
        Ok(())
    }

    /// P12-5: 公开 manifest schema 文档 (返回 markdown 字符串, 业务方塞进 docs)
    pub fn manifest_schema_doc() -> &'static str {
        include_str!("../docs/manifest-schema.md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest(name: &str, version: &str) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: version.parse().unwrap(),
            description: format!("{name} test plugin"),
            author: "test".to_string(),
            source: PluginSource::Local(format!("./{name}")),
            tags: vec!["test".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn registry_new_is_empty() {
        let reg = Registry::new();
        assert_eq!(reg.count(), 0);
        assert_eq!(reg.version_count(), 0);
        assert!(reg.list().is_empty());
    }

    #[test]
    fn registry_publish_first_version() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "0.1.0")).unwrap();
        assert_eq!(reg.count(), 1);
        let m = reg.get("alpha").unwrap();
        assert_eq!(m.version.to_string(), "0.1.0");
    }

    #[test]
    fn registry_publish_higher_version_appends() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "0.1.0")).unwrap();
        reg.publish(sample_manifest("alpha", "0.2.0")).unwrap();
        reg.publish(sample_manifest("alpha", "1.0.0")).unwrap();
        assert_eq!(reg.count(), 1);
        assert_eq!(reg.version_count(), 3);
        let m = reg.get("alpha").unwrap();
        assert_eq!(m.version.to_string(), "1.0.0");
    }

    #[test]
    fn registry_publish_lower_version_conflicts() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "1.0.0")).unwrap();
        let err = reg.publish(sample_manifest("alpha", "0.5.0")).unwrap_err();
        assert!(matches!(err, RegistryError::VersionConflict(_, _)));
    }

    #[test]
    fn registry_publish_equal_version_conflicts() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "1.0.0")).unwrap();
        let err = reg.publish(sample_manifest("alpha", "1.0.0")).unwrap_err();
        assert!(matches!(err, RegistryError::VersionConflict(_, _)));
    }

    #[test]
    fn registry_upsert_overwrites() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "1.0.0")).unwrap();
        reg.upsert(sample_manifest("alpha", "1.0.0")).unwrap();
        assert_eq!(reg.version_count(), 1);
    }

    #[test]
    fn registry_get_specific_version() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "0.1.0")).unwrap();
        reg.publish(sample_manifest("alpha", "0.2.0")).unwrap();
        let v = Version::parse("0.1.0").unwrap();
        let m = reg.get_version("alpha", &v).unwrap();
        assert_eq!(m.version.to_string(), "0.1.0");
    }

    #[test]
    fn registry_get_not_found() {
        let reg = Registry::new();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_list_sorted_by_name() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("zebra", "0.1.0")).unwrap();
        reg.publish(sample_manifest("alpha", "0.1.0")).unwrap();
        reg.publish(sample_manifest("middle", "0.1.0")).unwrap();
        let names: Vec<&str> = reg.list().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn registry_search_by_tag() {
        let mut reg = Registry::new();
        let mut m1 = sample_manifest("alpha", "0.1.0");
        m1.tags = vec!["utility".to_string(), "fs".to_string()];
        let mut m2 = sample_manifest("beta", "0.1.0");
        m2.tags = vec!["vision".to_string()];
        reg.publish(m1).unwrap();
        reg.publish(m2).unwrap();

        let utility = reg.search_by_tag("utility");
        assert_eq!(utility.len(), 1);
        assert_eq!(utility[0].name, "alpha");

        let vision = reg.search_by_tag("vision");
        assert_eq!(vision.len(), 1);
        assert_eq!(vision[0].name, "beta");

        let unknown = reg.search_by_tag("unknown");
        assert!(unknown.is_empty());
    }

    #[test]
    fn registry_remove_all_versions() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "0.1.0")).unwrap();
        reg.publish(sample_manifest("alpha", "0.2.0")).unwrap();
        assert_eq!(reg.version_count(), 2);
        reg.remove("alpha").unwrap();
        assert_eq!(reg.count(), 0);
        assert_eq!(reg.version_count(), 0);
    }

    #[test]
    fn registry_remove_not_found() {
        let mut reg = Registry::new();
        let err = reg.remove("nonexistent").unwrap_err();
        assert!(matches!(err, RegistryError::NotFound(_)));
    }

    #[test]
    fn registry_persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");

        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "0.1.0")).unwrap();
        reg.publish(sample_manifest("beta", "0.5.0")).unwrap();
        reg.save(&path).unwrap();

        let loaded = Registry::open(&path).unwrap();
        assert_eq!(loaded.count(), 2);
        assert_eq!(loaded.get("alpha").unwrap().version.to_string(), "0.1.0");
        assert_eq!(loaded.get("beta").unwrap().version.to_string(), "0.5.0");
    }

    #[test]
    fn registry_open_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.json");
        let reg = Registry::open(&path).unwrap();
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn registry_open_empty_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.json");
        std::fs::write(&path, "").unwrap();
        let reg = Registry::open(&path).unwrap();
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn plugin_source_local_serialize() {
        let src = PluginSource::Local("./plugin".to_string());
        let json = serde_json::to_string(&src).unwrap();
        assert!(json.contains("\"type\":\"local\""));
    }

    #[test]
    fn plugin_source_git_serialize() {
        let src = PluginSource::Git {
            url: "https://github.com/foo/bar".to_string(),
            rev: Some("v0.1.0".to_string()),
        };
        let json = serde_json::to_string(&src).unwrap();
        assert!(json.contains("\"type\":\"git\""));
        assert!(json.contains("\"url\":\"https://github.com/foo/bar\""));
    }

    #[test]
    fn plugin_manifest_full_serialize() {
        let m = sample_manifest("alpha", "0.1.0");
        let json = serde_json::to_string_pretty(&m).unwrap();
        let parsed: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, m);
    }

    // === P12-5 Registry v2 tests ===

    #[test]
    fn search_by_author_returns_matches() {
        let mut reg = Registry::new();
        let mut m1 = sample_manifest("alpha", "0.1.0");
        m1.author = "alice".to_string();
        let mut m2 = sample_manifest("beta", "0.1.0");
        m2.author = "bob".to_string();
        let mut m3 = sample_manifest("gamma", "0.1.0");
        m3.author = "alice".to_string();
        reg.publish(m1).unwrap();
        reg.publish(m2).unwrap();
        reg.publish(m3).unwrap();

        let alice = reg.search_by_author("alice");
        assert_eq!(alice.len(), 2);
        let names: Vec<&str> = alice.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"gamma"));

        let bob = reg.search_by_author("bob");
        assert_eq!(bob.len(), 1);
        assert_eq!(bob[0].name, "beta");

        let unknown = reg.search_by_author("nobody");
        assert!(unknown.is_empty());
    }

    #[test]
    fn search_by_name_substring_case_insensitive() {
        let mut reg = Registry::new();
        reg.publish(sample_manifest("awesome-tool", "0.1.0"))
            .unwrap();
        reg.publish(sample_manifest("my-thing", "0.1.0")).unwrap();
        reg.publish(sample_manifest("AwesomeOther", "0.1.0"))
            .unwrap();

        // 业务方搜 "awesome" 应匹配 awesome-tool + AwesomeOther
        let awesome = reg.search_by_name("awesome");
        assert_eq!(awesome.len(), 2);

        // 业务方搜 "AWESOME" 大小写不敏感
        let awesome_upper = reg.search_by_name("AWESOME");
        assert_eq!(awesome_upper.len(), 2);

        // 业务方搜 "thing" 只 my-thing
        let thing = reg.search_by_name("thing");
        assert_eq!(thing.len(), 1);

        // 不匹配
        let empty = reg.search_by_name("xyz");
        assert!(empty.is_empty());
    }

    #[test]
    fn list_authors_dedup_sorted() {
        let mut reg = Registry::new();
        let mut m1 = sample_manifest("a", "0.1.0");
        m1.author = "bob".to_string();
        let mut m2 = sample_manifest("b", "0.1.0");
        m2.author = "alice".to_string();
        let mut m3 = sample_manifest("c", "0.1.0");
        m3.author = "bob".to_string();
        reg.publish(m1).unwrap();
        reg.publish(m2).unwrap();
        reg.publish(m3).unwrap();

        let authors = reg.list_authors();
        assert_eq!(authors, vec!["alice", "bob"]);
    }

    #[test]
    fn list_all_tags_dedup_sorted() {
        let mut reg = Registry::new();
        let mut m1 = sample_manifest("a", "0.1.0");
        m1.tags = vec!["utility".to_string(), "fs".to_string()];
        let mut m2 = sample_manifest("b", "0.1.0");
        m2.tags = vec!["utility".to_string(), "vision".to_string()];
        let mut m3 = sample_manifest("c", "0.1.0");
        m3.tags = vec!["fs".to_string()];
        reg.publish(m1).unwrap();
        reg.publish(m2).unwrap();
        reg.publish(m3).unwrap();

        let tags = reg.list_all_tags();
        assert_eq!(tags, vec!["fs", "utility", "vision"]);
    }

    #[test]
    fn export_to_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.json");

        let mut reg = Registry::new();
        reg.publish(sample_manifest("alpha", "0.1.0")).unwrap();
        reg.publish(sample_manifest("beta", "0.5.0")).unwrap();
        reg.export(&path).unwrap();

        let loaded = Registry::open(&path).unwrap();
        assert_eq!(loaded.count(), 2);
        assert_eq!(loaded.get("alpha").unwrap().version.to_string(), "0.1.0");
        assert_eq!(loaded.get("beta").unwrap().version.to_string(), "0.5.0");
    }

    #[test]
    fn merge_two_registries_combines_unique_versions() {
        let mut reg_a = Registry::new();
        reg_a.publish(sample_manifest("alpha", "0.1.0")).unwrap();
        reg_a.publish(sample_manifest("alpha", "0.2.0")).unwrap();

        let mut reg_b = Registry::new();
        reg_b.publish(sample_manifest("alpha", "0.2.0")).unwrap(); // 重复
        reg_b.publish(sample_manifest("alpha", "0.3.0")).unwrap(); // 新
        reg_b.publish(sample_manifest("beta", "0.1.0")).unwrap(); // 新

        reg_a.merge(reg_b).unwrap();
        assert_eq!(reg_a.count(), 2); // alpha + beta
        assert_eq!(reg_a.version_count(), 4); // 0.1.0, 0.2.0, 0.3.0 (alpha) + 0.1.0 (beta)
        assert_eq!(reg_a.get("alpha").unwrap().version.to_string(), "0.3.0");
    }

    #[test]
    fn manifest_schema_doc_loads() {
        // 业务方 fetch manifest schema 文档 (v2: 跟 dsh docs 对齐)
        let doc = Registry::manifest_schema_doc();
        assert!(!doc.is_empty());
        assert!(doc.contains("Plugin Manifest Schema"));
        assert!(doc.contains("Local"));
        assert!(doc.contains("Git"));
        assert!(doc.contains("Http"));
    }
}
