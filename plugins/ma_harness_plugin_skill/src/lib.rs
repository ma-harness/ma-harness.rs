//! ma_harness_plugin_skill — 加载 .skill 目录的 skill 描述

#![deny(unsafe_code)]
#![warn(missing_docs)]

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;

pub static SKILLS_DIR: ma_harness_cordis::CtxKey<String> = ctx_key!("skills_dir");

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml parse: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("skill not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// SkillService — 加载 .skill 目录
pub struct SkillService;

impl SkillService {
    pub async fn load_skill(&self, ctx: &Context, name: &str) -> Result<Skill, SkillError> {
        let dir = ctx.get(SKILLS_DIR).unwrap_or_else(|| "./skills".to_string());
        let path = PathBuf::from(&dir).join(format!("{}.yaml", name));
        if !path.exists() {
            return Err(SkillError::NotFound(name.to_string()));
        }
        let content = fs::read_to_string(&path).await?;
        let skill: Skill = serde_yaml::from_str(&content)?;
        Ok(skill)
    }

    pub async fn list_skills(&self, ctx: &Context) -> Result<Vec<String>, SkillError> {
        let dir = ctx.get(SKILLS_DIR).unwrap_or_else(|| "./skills".to_string());
        let path = Path::new(&dir);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut skills = Vec::new();
        let mut entries = fs::read_dir(path).await?;
        while let Some(e) = entries.next_entry().await? {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".yaml") {
                skills.push(name.trim_end_matches(".yaml").to_string());
            }
        }
        Ok(skills)
    }
}

impl CordisService for SkillService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(SkillService)
    }
    fn name(&self) -> &str {
        "skill"
    }
}

impl SeamService for SkillService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(SkillService)
    }
    fn name(&self) -> &str {
        "skill"
    }
}

pub struct SkillPlugin;

impl CordisPlugin for SkillPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = SkillService::install(ctx)?;
        ctx.inject(Arc::new(svc));
        ctx.set(SKILLS_DIR, "./skills".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = Context::new();
        ctx.set(SKILLS_DIR, tmp.path().to_string_lossy().to_string());
        let svc = SkillService;
        let skills = svc.list_skills(&ctx).await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn load_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml = r#"
name: hello
description: say hello
body: print("hi")
"#;
        std::fs::write(tmp.path().join("hello.yaml"), yaml).unwrap();
        let ctx = Context::new();
        ctx.set(SKILLS_DIR, tmp.path().to_string_lossy().to_string());
        let svc = SkillService;
        let skill = svc.load_skill(&ctx, "hello").await.unwrap();
        assert_eq!(skill.name, "hello");
        assert!(skill.body.contains("hi"));
    }

    #[tokio::test]
    async fn load_nonexistent_errors() {
        let ctx = Context::new();
        ctx.set(SKILLS_DIR, "/nonexistent".to_string());
        let svc = SkillService;
        let result = svc.load_skill(&ctx, "nope").await;
        assert!(matches!(result, Err(SkillError::NotFound(_))));
    }
}
