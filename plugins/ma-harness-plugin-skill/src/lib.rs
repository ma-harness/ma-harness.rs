//! ma_harness_plugin_skill — first-party plugin: skill catalog loader
//!
//! **P14.3.2 重构**: SkillService 内部从直接 yaml loader 改为
//! 走 `ma_harness_skill::SkillProvider` (跟 dsh `ctx.skill` seam 1:1 对等).
//! 公开 API 保持 (`load_skill` / `list_skills` / `Skill` struct 不变).
//!
//! **设计**: skill catalog 在 plugin install 时从 `SKILLS_DIR` 扫描一次,
//! 后续业务方拿 `SkillCatalog` (immutable). 业务方可自己 reload (`refresh` 方法).
//!
//! **背景**: 见 [dsh-feature-parity-table §2] `ctx.skill`.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};
use ma_harness_skill::{LocalSkillProvider, SkillCatalog, SkillError, SkillManifest, SkillProvider};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// 公开 typed key
// ============================================================================

/// Skills 目录路径 (兼容旧 plugin-skill, P14.3.2 也保留).
/// 默认 `./skills`. 业务方 ctx.set(SKILLS_DIR, "~/.ma-harness/skills") 自定义.
pub static SKILLS_DIR: ma_harness_cordis::CtxKey<String> = ctx_key!("skills_dir");

/// Skill catalog (P14.3.2 新): install 后业务方可 ctx.get(SKILL_CATALOG) 拿.
pub static SKILL_CATALOG: ma_harness_cordis::CtxKey<Arc<SkillCatalog>> = ctx_key!("skill_catalog");

// ============================================================================
// 错误
// ============================================================================

/// Skill plugin 错误
#[derive(Debug, Error)]
pub enum PluginError {
    /// Skill 不存在
    #[error("skill not found: {0}")]
    NotFound(String),

    /// ctx.skill 错误 (P14.3.2: 委托给 ma-harness-skill)
    #[error("skill error: {0}")]
    Skill(#[from] SkillError),

    /// IO 错误
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// YAML 解析错误 (legacy yaml 兼容)
    #[error("yaml parse: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

// ============================================================================
// Skill: 旧版 struct, 保留向后兼容
// ============================================================================

/// Skill (旧版 struct, P14.3.2 保留向后兼容).
///
/// 业务方旧代码 `let skill = svc.load_skill(&ctx, "name").await?` 仍能用.
/// 新代码用 `ma_harness_skill::SkillManifest` (有 when_to_use / extra 字段).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill 名
    pub name: String,
    /// 一句话描述
    pub description: String,
    /// Skill body (markdown 文本)
    pub body: String,
}

impl From<SkillManifest> for Skill {
    fn from(m: SkillManifest) -> Self {
        Self {
            name: m.metadata.name,
            description: m.metadata.description,
            body: m.body,
        }
    }
}

// ============================================================================
// SkillService
// ============================================================================

/// Skill service — 加载 skill catalog, 业务方调 `load_skill` / `list_skills`.
///
/// **P14.3.2 重构**: 内部持 `Arc<SkillCatalog>` (plugin install 时从 SKILLS_DIR 扫描一次),
/// 业务方调 `load_skill` 直接查 catalog, 不再读 yaml. 业务方 reload 调 `refresh`.
pub struct SkillService {
    catalog: Arc<SkillCatalog>,
}

impl SkillService {
    /// 创建一个 SkillService (用空 catalog, 业务方可 `refresh` 加载)
    pub fn new() -> Self {
        Self {
            catalog: Arc::new(SkillCatalog::new()),
        }
    }

    /// 重新扫描 SKILLS_DIR 刷新 catalog (业务方运行时新增 skill 后调)
    pub async fn refresh(&mut self, ctx: &Context) -> Result<(), PluginError> {
        let dir = self.resolve_dir(ctx);
        let provider = self.resolve_provider(ctx);
        let catalog = provider.scan_dir(&dir).await?;
        self.catalog = Arc::new(catalog);
        Ok(())
    }

    /// 按名加载 skill (旧 API, 返回 `Skill` 而不是 `SkillManifest`, 向后兼容)
    pub async fn load_skill(&self, _ctx: &Context, name: &str) -> Result<Skill, PluginError> {
        let manifest = self
            .catalog
            .get(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;
        Ok(Skill::from((*manifest).clone()))
    }

    /// 列出所有 skill 名
    pub async fn list_skills(&self, _ctx: &Context) -> Result<Vec<String>, PluginError> {
        Ok(self.catalog.list())
    }

    /// 拿 catalog 的 clone (新 API, 业务方直接用 SkillCatalog)
    pub fn catalog(&self) -> Arc<SkillCatalog> {
        Arc::clone(&self.catalog)
    }

    /// 解析 SKILLS_DIR (业务方 ctx.set(SKILLS_DIR, ...) 优先, fallback `./skills`)
    fn resolve_dir(&self, ctx: &Context) -> PathBuf {
        ctx.get(SKILLS_DIR)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./skills"))
    }

    /// 解析 SkillProvider (业务方 ctx.set(SKILL_PROVIDER, ...) 优先, fallback LocalSkillProvider)
    fn resolve_provider(&self, ctx: &Context) -> Arc<dyn SkillProvider> {
        ctx.get(ma_harness_skill::SKILL_PROVIDER)
            .unwrap_or_else(|| Arc::new(LocalSkillProvider::new()) as Arc<dyn SkillProvider>)
    }
}

impl Default for SkillService {
    fn default() -> Self {
        Self::new()
    }
}

impl CordisService for SkillService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(SkillService::new())
    }
    fn name(&self) -> &str {
        "skill"
    }
}

impl SeamService for SkillService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(SkillService::new())
    }
    fn name(&self) -> &str {
        "skill"
    }
}

// ============================================================================
// Plugin: SkillPlugin
// ============================================================================

/// Skill plugin — install 时 scan SKILLS_DIR + 注 SkillService + 写默认 typed key
pub struct SkillPlugin;

impl CordisPlugin for SkillPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = <SkillService as ma_harness_cordis::Service>::install(ctx)?;
        // P14.3.2: 默认 SKILLS_DIR = "./skills" (兼容旧 plugin-skill 默认值)
        ctx.set(SKILLS_DIR, "./skills".to_string());
        // P14.3.2: 装 LocalSkillProvider 到 ctx.skill (业务方可覆盖)
        if ctx.get(ma_harness_skill::SKILL_PROVIDER).is_none() {
            ctx.set(
                ma_harness_skill::SKILL_PROVIDER,
                Arc::new(LocalSkillProvider::new()) as Arc<dyn SkillProvider>,
            );
        }
        ctx.inject(Arc::new(svc));
        Ok(())
    }
    fn name(&self) -> &str {
        "skill"
    }
}

impl SeamPlugin for SkillPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "skill"
    }
}

// ============================================================================
// 单元测试 (P14.3.2: plugin-skill 走 ctx.skill, 旧 API 兼容)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用 SKILL.md (P14.3.1 格式)
    const HELLO_SKILL_MD: &str = r#"---
name: hello
description: say hello to the user
---

print("hi")
"#;

    const GIT_COMMIT_SKILL_MD: &str = r#"---
name: git_commit
description: commit staged changes
when_to_use: when user asks to commit
---

git commit -m "$message"
"#;

    /// 业务方写 1 个 SKILL.md 到临时 dir
    async fn write_skill(dir: &std::path::Path, name: &str, content: &str) {
        let sub = dir.join(name);
        std::fs::create_dir_all(&sub).expect("mkdir");
        std::fs::write(sub.join("SKILL.md"), content).expect("write");
    }

    /// 创建 ctx 装 SKILLS_DIR + SkillProvider
    async fn setup_ctx(skills_dir: &std::path::Path) -> Context {
        let ctx = Context::new();
        ctx.set(SKILLS_DIR, skills_dir.to_string_lossy().to_string());
        ctx.set(
            ma_harness_skill::SKILL_PROVIDER,
            Arc::new(LocalSkillProvider::new()) as Arc<dyn SkillProvider>,
        );
        ctx
    }

    #[tokio::test]
    async fn load_skill_via_ctx_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        std::fs::create_dir_all(&skills_root).unwrap();
        write_skill(&skills_root, "hello", HELLO_SKILL_MD).await;

        let ctx = setup_ctx(&skills_root).await;
        let mut svc = SkillService::new();
        svc.refresh(&ctx).await.expect("refresh failed");

        // 旧 API: load_skill (返回 Skill struct, 向后兼容)
        let skill = svc.load_skill(&ctx, "hello").await.expect("load failed");
        assert_eq!(skill.name, "hello");
        assert_eq!(skill.description, "say hello to the user");
        assert!(skill.body.contains("print(\"hi\")"));
    }

    #[tokio::test]
    async fn list_skills_via_ctx_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        std::fs::create_dir_all(&skills_root).unwrap();
        write_skill(&skills_root, "hello", HELLO_SKILL_MD).await;
        write_skill(&skills_root, "git_commit", GIT_COMMIT_SKILL_MD).await;

        let ctx = setup_ctx(&skills_root).await;
        let mut svc = SkillService::new();
        svc.refresh(&ctx).await.expect("refresh failed");

        let skills = svc.list_skills(&ctx).await.expect("list failed");
        assert_eq!(skills, vec!["git_commit".to_string(), "hello".to_string()]);
    }

    #[tokio::test]
    async fn load_nonexistent_skill_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        std::fs::create_dir_all(&skills_root).unwrap();
        // 不写任何 skill

        let ctx = setup_ctx(&skills_root).await;
        let mut svc = SkillService::new();
        svc.refresh(&ctx).await.expect("refresh");

        let err = svc.load_skill(&ctx, "nope").await.unwrap_err();
        assert!(matches!(err, PluginError::NotFound(_)));
    }

    #[tokio::test]
    async fn refresh_picks_up_new_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        std::fs::create_dir_all(&skills_root).unwrap();

        let ctx = setup_ctx(&skills_root).await;
        let mut svc = SkillService::new();
        svc.refresh(&ctx).await.expect("refresh 1");
        assert_eq!(svc.list_skills(&ctx).await.unwrap().len(), 0);

        // 业务方运行时加 1 个 skill
        write_skill(&skills_root, "hello", HELLO_SKILL_MD).await;
        svc.refresh(&ctx).await.expect("refresh 2");

        let skills = svc.list_skills(&ctx).await.unwrap();
        assert_eq!(skills, vec!["hello".to_string()]);
    }

    #[tokio::test]
    async fn catalog_get_returns_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        std::fs::create_dir_all(&skills_root).unwrap();
        write_skill(&skills_root, "git_commit", GIT_COMMIT_SKILL_MD).await;

        let ctx = setup_ctx(&skills_root).await;
        let mut svc = SkillService::new();
        svc.refresh(&ctx).await.expect("refresh");

        // 新 API: catalog().get() 拿 SkillManifest (有 when_to_use)
        let manifest = svc.catalog().get("git_commit").expect("not found");
        assert_eq!(manifest.name(), "git_commit");
        assert_eq!(manifest.when_to_use(), Some("when user asks to commit"));
    }

    #[tokio::test]
    async fn empty_dir_yields_empty_catalog() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_root = tmp.path().join("skills");
        std::fs::create_dir_all(&skills_root).unwrap();
        // 不写 skill

        let ctx = setup_ctx(&skills_root).await;
        let mut svc = SkillService::new();
        svc.refresh(&ctx).await.expect("refresh");
        assert!(svc.list_skills(&ctx).await.unwrap().is_empty());
    }
}
