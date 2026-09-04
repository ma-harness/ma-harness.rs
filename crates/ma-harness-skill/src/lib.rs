//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-skill`
//! **Crate ident** (`use` 路径): `ma_harness_skill`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-skill = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_skill::{LocalSkillProvider, SkillProvider};
//!
//! let provider = LocalSkillProvider::new();
//! let catalog = provider.scan_dir("~/.ma-harness/skills").await?;
//! // 业务方调: provider.invoke("git-commit", json!({})).await?
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-skill
//!
//! # 设计 (Design) — P14.3
//!
//! **目标**: 抽 `ctx.skill` 能力缝 (Service Definition), 让业务方:
//! - 把 `~/.ma-harness/skills/<name>/SKILL.md` 目录的 skill 自动发现
//! - 注册到 `SkillCatalog`
//! - LLM 拿 `use_skill(name) -> body` tool 调
//!
//! **背景**: 见 [dsh-feature-parity-table §2] — `ctx.skill` 在 dsh 是 14 个能力缝之一.
//! ma-harness 之前 `plugin-skill` 是个简易 yaml loader (无 frontmatter / 无 when_to_use / 无 consumer pattern),
//! P14.3 抽出来成独立 host crate, plugin-skill 重构成 thin wrapper.
//!
//! **SKILL.md 格式** (P14.3 新增, 跟 dsh 对齐):
//! ```markdown
//! ---
//! name: git-commit
//! description: Commit staged changes with a message
//! when_to_use: When user asks to commit, save, or checkpoint work
//! ---
//!
//! # Skill body (markdown)
//!
//! Instructions for the LLM to follow.
//! ```
//!
//! **核心抽象**:
//! - [`SkillManifest`] — frontmatter + body + path
//! - [`parse_skill_md`] — markdown → (frontmatter YAML, body markdown)
//! - [`SkillCatalog`] — in-memory 注册表 (类似 [`ShellRegistry`](ma_harness_shell::ShellRegistry))
//! - [`SkillProvider`] trait — scan_dir / register / invoke
//! - [`LocalSkillProvider`] — 默认实现 (P14.3 主交付)
//! - [`SKILL_PROVIDER`] typed key (ctx.skill 注入点, 跟 P14.2.2 SHELL_SERVICE 平行)
//!
//! **6 质量属性**:
//! - 可复用: 跟 [`ShellRegistry`](ma_harness_shell::ShellRegistry) 同样 Consumer pattern
//! - 可维护: 模块化分块, error / manifest / parser / provider 集中在 lib.rs
//! - 鲁棒: frontmatter 解析失败明确报错, 业务方可看 stderr 修 SKILL.md
//! - 安全: 不 eval body, body 静态当 string 给 LLM 读
//! - 可测: tempfile 创建临时 skill dir, 8 个测试覆盖 parse / scan / register / invoke
//! - 可扩展: SkillProvider trait, 未来可加 RemoteSkillProvider (从 URL / DB 加载)
//!
//! # 限制 (Limitations) — P14.3.1
//!
//! - 还没迁移 `plugin-skill` (现有 yaml loader), P14.3.2 拆
//! - 还没 `use_skill` macro (业务方手写 invoke), P14.3.3 加 proc-macro
//! - body 不支持动态参数 ({{ var }}), 业务方 invoke 时直接给 body, 模板 P15+
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// SkillError: 统一的 skill 错误
// ============================================================================

/// Skill 能力缝错误.
#[derive(Debug, Error)]
pub enum SkillError {
    /// IO 错误 (读 / 写 SKILL.md 失败)
    #[error("skill I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML frontmatter 解析失败
    #[error("skill YAML parse error in {path:?}: {source}")]
    Yaml {
        /// 失败的 SKILL.md 路径
        path: PathBuf,
        /// 底层 yaml 错误
        #[source]
        source: serde_yaml::Error,
    },

    /// SKILL.md 格式错误 (没找到 frontmatter, 或缺 name/description)
    #[error("invalid SKILL.md at {path:?}: {reason}")]
    InvalidFormat {
        /// 路径
        path: PathBuf,
        /// 原因
        reason: String,
    },

    /// Skill 不存在
    #[error("skill not found: {0}")]
    NotFound(String),

    /// Provider 不支持此操作
    #[error("provider '{provider}' does not support {operation}: {reason}")]
    Unsupported {
        /// Provider 名
        provider: &'static str,
        /// 操作名
        operation: &'static str,
        /// 原因
        reason: String,
    },
}

// ============================================================================
// SkillManifest: frontmatter + body + path
// ============================================================================

/// Skill frontmatter (YAML 嵌入在 SKILL.md 顶部的 `---` 之间).
///
/// **不包含 body** — body 在 SKILL.md `---` 之后, 业务方用 [`SkillManifest::body`]
/// 读 (单独字段, 跟 metadata 分开).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillMetadata {
    /// Skill 名 (snake_case, e.g. "git_commit"). catalog key.
    pub name: String,
    /// 一句话描述 (LLM 看的, 决定什么时候 invoke)
    pub description: String,
    /// 什么时候用 (可选, 比 description 更具体的触发条件)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
    /// 额外元数据 (业务方自由扩展, e.g. tags / version)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

/// Skill 完整描述 (metadata + body + 路径).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillManifest {
    /// frontmatter (name / description / when_to_use / extra)
    pub metadata: SkillMetadata,
    /// markdown body (frontmatter `---` 之后的内容)
    pub body: String,
    /// SKILL.md 路径 (业务方 debug 用, 跟 ctx.skill 一起显示给 LLM)
    pub path: PathBuf,
}

impl SkillManifest {
    /// 业务方拿 skill 名 (短)
    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    /// 业务方拿 description
    pub fn description(&self) -> &str {
        &self.metadata.description
    }

    /// 业务方拿 when_to_use (可能有)
    pub fn when_to_use(&self) -> Option<&str> {
        self.metadata.when_to_use.as_deref()
    }

    /// LLM tool 参数 schema (P14.3.1: 无参数, body 静态; P15+ 加 `{{ var }}` 模板后扩展)
    pub fn param_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
}

// ============================================================================
// parse_skill_md: markdown → (frontmatter YAML, body markdown)
// ============================================================================

/// 解析 SKILL.md 内容 (frontmatter + body).
///
/// **格式**:
/// ```text
/// ---
/// name: foo
/// description: bar
/// ---
/// <body markdown>
/// ```
///
/// # Errors
/// - 没找到 leading `---` 跟 trailing `---`: `InvalidFormat`
/// - YAML 部分解析失败: `Yaml`
/// - 缺 `name` / `description` 字段: `InvalidFormat`
pub fn parse_skill_md(content: &str, path: &Path) -> Result<SkillManifest, SkillError> {
    // 找 leading `---` (必须从第一行开始)
    let content = content.trim_start_matches('\u{feff}'); // strip BOM

    let after_first = content
        .strip_prefix("---")
        .ok_or_else(|| SkillError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "missing leading '---' for frontmatter".into(),
        })?;

    // 找 closing `---` (允许前后空白)
    let (frontmatter_yaml, body) = match find_closing_fence(after_first) {
        Some((yaml, body)) => (yaml, body),
        None => {
            return Err(SkillError::InvalidFormat {
                path: path.to_path_buf(),
                reason: "missing closing '---' for frontmatter".into(),
            });
        }
    };

    let metadata: SkillMetadata =
        serde_yaml::from_str(frontmatter_yaml).map_err(|e| SkillError::Yaml {
            path: path.to_path_buf(),
            source: e,
        })?;

    if metadata.name.is_empty() {
        return Err(SkillError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "frontmatter 'name' is empty".into(),
        });
    }
    if metadata.description.is_empty() {
        return Err(SkillError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "frontmatter 'description' is empty".into(),
        });
    }

    Ok(SkillManifest {
        metadata,
        body: body.trim().to_string(),
        path: path.to_path_buf(),
    })
}

/// 找 closing `---` (允许前后换行), 返回 (yaml 部分, body 部分).
///
/// "YAML 部分" 不含 leading 换行, "body 部分" trim 起始换行.
fn find_closing_fence(s: &str) -> Option<(&str, &str)> {
    // skip leading \n after first ---
    let s = s.strip_prefix('\n').unwrap_or(s);

    // 找下一行 '---' (允许前导空白)
    let mut idx = 0;
    for line in s.split_inclusive('\n') {
        if line.trim() == "---" {
            // body 从 idx 之后 (skip 换行)
            let yaml = &s[..idx];
            let body_start = idx + line.len();
            // body 跳过起始换行
            let body = s[body_start..]
                .strip_prefix('\n')
                .unwrap_or(&s[body_start..]);
            return Some((yaml, body));
        }
        idx += line.len();
    }
    None
}

// ============================================================================
// SkillCatalog: in-memory 注册表 (Consumer pattern)
// ============================================================================

/// Skill 注册表 (类似 [`ShellRegistry`](ma_harness_shell::ShellRegistry)).
///
/// 业务方 `catalog.add(manifest)`, agent `catalog.get("git-commit")` 或
/// `catalog.invoke("git-commit", args)`.
pub struct SkillCatalog {
    skills: std::collections::HashMap<String, Arc<SkillManifest>>,
}

impl SkillCatalog {
    /// 创建一个空 catalog
    pub fn new() -> Self {
        Self {
            skills: std::collections::HashMap::new(),
        }
    }

    /// 注册一个 skill (重复 name 覆盖前一个 + log warn)
    pub fn add(&mut self, manifest: SkillManifest) {
        let name = manifest.metadata.name.clone();
        if self.skills.contains_key(&name) {
            tracing::warn!(skill = %name, "SkillCatalog::add overrides existing skill");
        }
        tracing::debug!(skill = %name, "skill registered");
        self.skills.insert(name, Arc::new(manifest));
    }

    /// 按名拿 skill
    pub fn get(&self, name: &str) -> Option<Arc<SkillManifest>> {
        self.skills.get(name).cloned()
    }

    /// 列出所有 skill 名 (sorted)
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.skills.keys().cloned().collect();
        names.sort();
        names
    }

    /// 数量
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// 业务方拿所有 manifest (for LLM tool list)
    pub fn manifests(&self) -> Vec<Arc<SkillManifest>> {
        let mut v: Vec<Arc<SkillManifest>> = self.skills.values().cloned().collect();
        v.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
        v
    }

    /// 按名 invoke (返回 body 给 LLM, args 暂不展开 — 模板 P15+)
    ///
    /// # Errors
    /// - skill 不存在: `SkillError::NotFound`
    pub fn invoke(&self, name: &str, _args: serde_json::Value) -> Result<String, SkillError> {
        let manifest = self
            .get(name)
            .ok_or_else(|| SkillError::NotFound(name.to_string()))?;
        Ok(manifest.body.clone())
    }

    /// 给 LLM 用的 tool list (跟 dsh `tools/pre-execute` 走同一格式)
    pub fn tool_list(&self) -> Vec<serde_json::Value> {
        self.manifests()
            .iter()
            .map(|m| {
                serde_json::json!({
                    "name": format!("use_skill_{}", m.metadata.name),
                    "description": m.metadata.description.clone(),
                    "parameters": m.param_schema(),
                })
            })
            .collect()
    }
}

impl Default for SkillCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SkillCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillCatalog")
            .field("skills", &self.list())
            .finish()
    }
}

// ============================================================================
// SkillProvider trait: 能力缝 (跟 dsh ctx.skill 对等)
// ============================================================================

/// Skill 能力缝 (跟 dsh `ctx.skill` seam 对等).
///
/// **核心方法**:
/// - [`scan_dir`](Self::scan_dir) — 扫描目录, 返回 [`SkillCatalog`]
/// - [`register`](Self::register) — 注册单个 manifest
/// - [`invoke`](Self::invoke) — 调 skill (返回 body)
///
/// **实现**:
/// - [`LocalSkillProvider`] — 默认 (P14.3.1 主交付)
/// - 业务方可注入 mock provider (测试用)
/// - 未来: RemoteSkillProvider (从 URL / DB 加载, P15+)
#[async_trait]
pub trait SkillProvider: Send + Sync + 'static {
    /// 扫描指定目录, 返回 SkillCatalog (业务方直接 invoke)
    async fn scan_dir(&self, dir: &Path) -> Result<SkillCatalog, SkillError>;

    /// 注册单个 manifest (业务方手写 / 程序生成)
    fn register(&self, catalog: &mut SkillCatalog, manifest: SkillManifest);

    /// 通过 catalog invoke skill
    async fn invoke(
        &self,
        catalog: &SkillCatalog,
        name: &str,
        args: serde_json::Value,
    ) -> Result<String, SkillError>;

    /// Provider 标识 (日志 / 调试)
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// LocalSkillProvider: 默认实现 (P14.3.1 主交付)
// ============================================================================

/// 本地 skill provider (P14.3.1 主交付).
///
/// **实现**: 扫描 `dir` 下所有 `SKILL.md` 文件, 解析 frontmatter + body,
/// 注册到 [`SkillCatalog`]. 业务方之后调 `invoke(name, args)`.
pub struct LocalSkillProvider;

impl LocalSkillProvider {
    /// 创建一个新 LocalSkillProvider
    pub const fn new() -> Self {
        Self
    }
}

impl Default for LocalSkillProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SkillProvider for LocalSkillProvider {
    async fn scan_dir(&self, dir: &Path) -> Result<SkillCatalog, SkillError> {
        let mut catalog = SkillCatalog::new();

        if !dir.exists() {
            tracing::debug!(dir = %dir.display(), "skills dir does not exist, returning empty catalog");
            return Ok(catalog);
        }
        if !dir.is_dir() {
            return Err(SkillError::InvalidFormat {
                path: dir.to_path_buf(),
                reason: format!("path is not a directory: {}", dir.display()),
            });
        }

        // 扫描所有 SKILL.md (递归 1 层, 业务方子目录也支持)
        // 业务方布局: ~/.ma-harness/skills/<name>/SKILL.md
        //          ~/.ma-harness/skills/<name>.md (legacy 兼容)
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if path.is_dir() {
                // 子目录: 找 SKILL.md
                let skill_md = path.join("SKILL.md");
                if skill_md.is_file() {
                    match load_one(&skill_md).await {
                        Ok(manifest) => catalog.add(manifest),
                        Err(e) => {
                            tracing::warn!(path = %skill_md.display(), error = %e, "skip invalid skill")
                        }
                    }
                }
            } else if name == "SKILL.md" || name.ends_with(".md") {
                // 直接 SKILL.md 或 .md 文件 (legacy 兼容 plugin-skill 的 .yaml 模式)
                match load_one(&path).await {
                    Ok(manifest) => catalog.add(manifest),
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "skip invalid skill")
                    }
                }
            }
        }

        tracing::debug!(count = catalog.len(), dir = %dir.display(), "skill catalog built");
        Ok(catalog)
    }

    fn register(&self, catalog: &mut SkillCatalog, manifest: SkillManifest) {
        catalog.add(manifest);
    }

    async fn invoke(
        &self,
        catalog: &SkillCatalog,
        name: &str,
        args: serde_json::Value,
    ) -> Result<String, SkillError> {
        catalog.invoke(name, args)
    }

    fn provider_name(&self) -> &'static str {
        "local-skill"
    }
}

/// 加载 1 个 SKILL.md 文件 (内部用, 业务方一般不直接调)
async fn load_one(path: &Path) -> Result<SkillManifest, SkillError> {
    let content = tokio::fs::read_to_string(path).await?;
    parse_skill_md(&content, path)
}

// ============================================================================
// SKILL_PROVIDER typed key (P14.3.1: 跟 ctx.skill 接入点, 跟 SHELL_SERVICE 平行)
// ============================================================================

/// Typed key: `ctx.skill` 注入的 SkillProvider.
///
/// 业务方:
/// ```ignore
/// use ma_harness_skill::{SKILL_PROVIDER, LocalSkillProvider, SkillProvider};
///
/// ctx.set(SKILL_PROVIDER, Arc::new(LocalSkillProvider::new()) as Arc<dyn SkillProvider>);
/// ```
///
/// 消费者 (例如 `plugin-skill` 重构后):
/// ```ignore
/// let provider: Arc<dyn SkillProvider> = ctx
///     .get(SKILL_PROVIDER)
///     .unwrap_or_else(|| Arc::new(LocalSkillProvider::new()));
/// ```
pub static SKILL_PROVIDER: ma_harness_cordis::CtxKey<Arc<dyn SkillProvider>> =
    ma_harness_seam::ctx_key!("skill_provider");

// ============================================================================
// DefaultSkillProvider: 平台默认 (P14.3.1: LocalSkillProvider)
// ============================================================================

/// 平台默认 skill provider (P14.3.1: LocalSkillProvider)
pub type DefaultSkillProvider = LocalSkillProvider;

// ============================================================================
// 单元测试 (mod tests) — 8 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    /// 测试用 SKILL.md (P14.3.1 格式)
    const GIT_COMMIT_SKILL: &str = r#"---
name: git_commit
description: Commit staged changes with a message
when_to_use: When user asks to commit or save work
---

# Git Commit Skill

Run `git commit -m "<message>"` to commit staged changes.

Args:
- message: commit message
"#;

    /// 测试用 SKILL.md (minimal)
    const MINIMAL_SKILL: &str = r#"---
name: hello
description: Say hello to the user
---

Just say hello.
"#;

    #[test]
    fn parse_skill_md_minimal() {
        let manifest = parse_skill_md(MINIMAL_SKILL, Path::new("hello.md")).expect("parse failed");
        assert_eq!(manifest.metadata.name, "hello");
        assert_eq!(manifest.metadata.description, "Say hello to the user");
        assert!(manifest.metadata.when_to_use.is_none());
        assert!(manifest.metadata.extra.is_empty());
        assert_eq!(manifest.body, "Just say hello.");
    }

    #[test]
    fn parse_skill_md_with_when_to_use_and_extra() {
        let manifest =
            parse_skill_md(GIT_COMMIT_SKILL, Path::new("git_commit.md")).expect("parse failed");
        assert_eq!(manifest.metadata.name, "git_commit");
        assert_eq!(
            manifest.metadata.description,
            "Commit staged changes with a message"
        );
        assert_eq!(
            manifest.metadata.when_to_use.as_deref(),
            Some("When user asks to commit or save work")
        );
        assert!(manifest.body.contains("git commit -m"));
    }

    #[test]
    fn parse_skill_md_missing_leading_fence_errors() {
        let content = "name: foo\ndescription: bar\n---\nbody";
        let err = parse_skill_md(content, Path::new("bad.md")).unwrap_err();
        assert!(matches!(err, SkillError::InvalidFormat { .. }));
    }

    #[test]
    fn parse_skill_md_missing_closing_fence_errors() {
        let content = "---\nname: foo\ndescription: bar\nbody without closing fence";
        let err = parse_skill_md(content, Path::new("bad.md")).unwrap_err();
        assert!(matches!(err, SkillError::InvalidFormat { .. }));
    }

    #[test]
    fn parse_skill_md_empty_name_errors() {
        let content = "---\nname: \"\"\ndescription: bar\n---\nbody";
        let err = parse_skill_md(content, Path::new("bad.md")).unwrap_err();
        assert!(matches!(err, SkillError::InvalidFormat { .. }));
    }

    #[test]
    fn parse_skill_md_strips_bom() {
        // BOM + 正常 frontmatter
        let mut content = std::ffi::OsString::from("\u{feff}");
        content.push("---\nname: foo\ndescription: bar\n---\nbody");
        let content_str = content.to_string_lossy();
        let manifest = parse_skill_md(&content_str, Path::new("bom.md")).expect("parse failed");
        assert_eq!(manifest.metadata.name, "foo");
    }

    #[tokio::test]
    async fn local_provider_scan_dir_finds_skill_md_subdirs() {
        let dir = tempdir().expect("tempdir");
        let skills_root = dir.path().join("skills");
        std::fs::create_dir_all(&skills_root).expect("mkdir");

        // 业务方布局: skills/<name>/SKILL.md
        let git_dir = skills_root.join("git_commit");
        std::fs::create_dir_all(&git_dir).expect("mkdir");
        std::fs::write(git_dir.join("SKILL.md"), GIT_COMMIT_SKILL).expect("write");

        let provider = LocalSkillProvider::new();
        let catalog = provider.scan_dir(&skills_root).await.expect("scan failed");
        assert_eq!(catalog.len(), 1);
        let skill = catalog.get("git_commit").expect("not found");
        assert_eq!(skill.name(), "git_commit");
        assert!(skill.body.contains("git commit -m"));
    }

    #[tokio::test]
    async fn local_provider_scan_dir_empty_when_dir_missing() {
        let dir = tempdir().expect("tempdir");
        let nonexistent = dir.path().join("does_not_exist");
        let provider = LocalSkillProvider::new();
        let catalog = provider
            .scan_dir(&nonexistent)
            .await
            .expect("scan should succeed with empty catalog for missing dir");
        assert!(catalog.is_empty());
    }

    #[tokio::test]
    async fn local_provider_scan_dir_handles_multiple_skills() {
        let dir = tempdir().expect("tempdir");
        let skills_root = dir.path().join("skills");
        std::fs::create_dir_all(&skills_root).expect("mkdir");

        // 2 个 skill
        for (name, content) in [("git_commit", GIT_COMMIT_SKILL), ("hello", MINIMAL_SKILL)] {
            let sub = skills_root.join(name);
            std::fs::create_dir_all(&sub).expect("mkdir");
            std::fs::write(sub.join("SKILL.md"), content).expect("write");
        }

        let provider = LocalSkillProvider::new();
        let catalog = provider.scan_dir(&skills_root).await.expect("scan");
        assert_eq!(catalog.len(), 2);
        let names = catalog.list();
        assert_eq!(names, vec!["git_commit".to_string(), "hello".to_string()]);
    }

    #[tokio::test]
    async fn local_provider_invoke_returns_body() {
        let dir = tempdir().expect("tempdir");
        let skills_root = dir.path().join("skills");
        std::fs::create_dir_all(&skills_root).expect("mkdir");
        let sub = skills_root.join("hello");
        std::fs::create_dir_all(&sub).expect("mkdir");
        std::fs::write(sub.join("SKILL.md"), MINIMAL_SKILL).expect("write");

        let provider = LocalSkillProvider::new();
        let catalog = provider.scan_dir(&skills_root).await.expect("scan");
        let body = provider
            .invoke(&catalog, "hello", serde_json::json!({}))
            .await
            .expect("invoke");
        assert_eq!(body, "Just say hello.");
    }

    #[tokio::test]
    async fn local_provider_invoke_unknown_errors() {
        let dir = tempdir().expect("tempdir");
        let provider = LocalSkillProvider::new();
        let catalog = provider.scan_dir(dir.path()).await.expect("scan (empty)");
        let err = provider
            .invoke(&catalog, "nonexistent", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::NotFound(_)));
    }

    #[test]
    fn skill_catalog_register_and_get_and_list() {
        let manifest = SkillManifest {
            metadata: SkillMetadata {
                name: "test".to_string(),
                description: "test skill".to_string(),
                when_to_use: None,
                extra: BTreeMap::new(),
            },
            body: "test body".to_string(),
            path: PathBuf::from("test.md"),
        };
        let mut catalog = SkillCatalog::new();
        catalog.add(manifest);
        assert_eq!(catalog.len(), 1);

        let got = catalog.get("test").expect("get");
        assert_eq!(got.body, "test body");

        let listed = catalog.list();
        assert_eq!(listed, vec!["test".to_string()]);
    }

    #[test]
    fn skill_catalog_register_override_warns() {
        let manifest1 = SkillManifest {
            metadata: SkillMetadata {
                name: "dup".to_string(),
                description: "first".to_string(),
                when_to_use: None,
                extra: BTreeMap::new(),
            },
            body: "first body".to_string(),
            path: PathBuf::from("dup1.md"),
        };
        let manifest2 = SkillManifest {
            metadata: SkillMetadata {
                name: "dup".to_string(),
                description: "second".to_string(),
                when_to_use: None,
                extra: BTreeMap::new(),
            },
            body: "second body".to_string(),
            path: PathBuf::from("dup2.md"),
        };
        let mut catalog = SkillCatalog::new();
        catalog.add(manifest1);
        catalog.add(manifest2);
        assert_eq!(catalog.len(), 1, "同名 skill 应被覆盖, 数量仍为 1");
        assert_eq!(catalog.get("dup").unwrap().body, "second body");
    }

    #[test]
    fn skill_catalog_tool_list_format() {
        let manifest = SkillManifest {
            metadata: SkillMetadata {
                name: "git_commit".to_string(),
                description: "Commit changes".to_string(),
                when_to_use: None,
                extra: BTreeMap::new(),
            },
            body: "...".to_string(),
            path: PathBuf::from("gc.md"),
        };
        let mut catalog = SkillCatalog::new();
        catalog.add(manifest);
        let tools = catalog.tool_list();
        assert_eq!(tools.len(), 1);
        let tool = &tools[0];
        assert_eq!(tool["name"], "use_skill_git_commit");
        assert_eq!(tool["description"], "Commit changes");
        assert!(tool["parameters"].is_object());
    }

    #[test]
    fn skill_manifest_accessors() {
        let manifest = SkillManifest {
            metadata: SkillMetadata {
                name: "foo".to_string(),
                description: "foo desc".to_string(),
                when_to_use: Some("when foo".to_string()),
                extra: BTreeMap::new(),
            },
            body: "body".to_string(),
            path: PathBuf::from("foo.md"),
        };
        assert_eq!(manifest.name(), "foo");
        assert_eq!(manifest.description(), "foo desc");
        assert_eq!(manifest.when_to_use(), Some("when foo"));
    }
}
