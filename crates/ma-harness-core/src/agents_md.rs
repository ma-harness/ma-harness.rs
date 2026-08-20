//! AGENTS.md 解析 (P10-5 / Day 101)
//!
//! 自动从工作目录读 `AGENTS.md` (或 `.agents.md` / `CLAUDE.md`), 给 model
//! 作 system prompt 的一部分. 跟 dsh 行为对齐.
//!
//! ## 格式
//!
//! AGENTS.md 是 markdown, 业务方写给 model 的指令, 例如:
//!
//! ```markdown
//! # Project: My App
//!
//! You are a helpful coding assistant for the My App project.
//! Use TypeScript for frontend, Rust for backend.
//!
//! ## Conventions
//!
//! - Use snake_case for database columns
//! - Always run tests before committing
//! ```
//!
//! ## 用法
//!
//! ```ignore
//! use ma_harness_core::agents_md::{load_agents_md, AgentsMdConfig};
//!
//! let cfg = AgentsMdConfig::default();
//! let prompt = load_agents_md("./", &cfg)?;
//! // 拼到 model_req.system_prompt
//! ```
//!
//! ## 安全
//!
//! - 单文件 < 64KB (防 OOM)
//! - 走 UTF-8 (拒绝非 UTF-8 binary)
//! - 业务方可禁用 (`enabled: false`)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// AGENTS.md 加载配置 (P10-5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsMdConfig {
    /// 启用 AGENTS.md 加载
    pub enabled: bool,
    /// 自定义文件名 (默认 ["AGENTS.md", ".agents.md", "CLAUDE.md"])
    pub file_names: Vec<String>,
    /// 最大文件大小 (bytes, 默认 64KB)
    pub max_size_bytes: u64,
    /// 是否递归向上找 (../AGENTS.md, ../../AGENTS.md)
    pub search_parents: bool,
    /// system prompt prefix 模板
    pub prompt_prefix: String,
}

impl Default for AgentsMdConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            file_names: vec![
                "AGENTS.md".to_string(),
                ".agents.md".to_string(),
                "CLAUDE.md".to_string(),
            ],
            max_size_bytes: 64 * 1024,
            search_parents: true,
            prompt_prefix: "# Project instructions from AGENTS.md:\n\n".to_string(),
        }
    }
}

/// AGENTS.md 加载结果 (P10-5)
#[derive(Debug, Clone)]
pub struct AgentsMdResult {
    /// 找到的 AGENTS.md 路径 (None = 没找到)
    pub found_path: Option<PathBuf>,
    /// 加载的内容
    pub content: Option<String>,
    /// 拼好的 system prompt (含 prefix, None = 没找到 AGENTS.md)
    pub system_prompt_fragment: Option<String>,
}

/// 从工作目录加载 AGENTS.md (P10-5)
///
/// 搜索顺序:
/// 1. `<cwd>/<file_name>` (按 file_names 顺序)
/// 2. `<parent>/<file_name>` (向上递归, search_parents=true 时)
/// 3. 返 None
pub fn load_agents_md(
    workdir: impl AsRef<Path>,
    cfg: &AgentsMdConfig,
) -> std::io::Result<AgentsMdResult> {
    if !cfg.enabled {
        return Ok(AgentsMdResult {
            found_path: None,
            content: None,
            system_prompt_fragment: None,
        });
    }

    let start = workdir.as_ref();
    if !start.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("workdir not found: {}", start.display()),
        ));
    }

    // 1. 先查当前目录
    if let Some(result) = search_in_dir(start, cfg) {
        return Ok(result);
    }

    // 2. 向上递归
    if cfg.search_parents {
        let mut current = start.parent();
        while let Some(dir) = current {
            if let Some(result) = search_in_dir(dir, cfg) {
                return Ok(result);
            }
            current = dir.parent();
        }
    }

    Ok(AgentsMdResult {
        found_path: None,
        content: None,
        system_prompt_fragment: None,
    })
}

fn search_in_dir(dir: &Path, cfg: &AgentsMdConfig) -> Option<AgentsMdResult> {
    for name in &cfg.file_names {
        let path = dir.join(name);
        if let Ok(content) = read_capped(&path, cfg.max_size_bytes) {
            let prompt = format!("{}{}", cfg.prompt_prefix, content);
            return Some(AgentsMdResult {
                found_path: Some(path),
                content: Some(content),
                system_prompt_fragment: Some(prompt),
            });
        }
    }
    None
}

fn read_capped(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "AGENTS.md too large: {} bytes (max {})",
                metadata.len(),
                max_bytes
            ),
        ));
    }
    std::fs::read_to_string(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_tempdir() -> TempDir {
        TempDir::new().unwrap()
    }

    fn write_file(path: &Path, content: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn load_finds_agents_md_in_cwd() {
        let dir = make_tempdir();
        let agents = dir.path().join("AGENTS.md");
        write_file(&agents, "# My Project\nBe helpful.");

        let cfg = AgentsMdConfig::default();
        let result = load_agents_md(dir.path(), &cfg).unwrap();
        assert!(result.found_path.is_some());
        assert!(result.content.unwrap().contains("My Project"));
        let prompt = result.system_prompt_fragment.unwrap();
        assert!(prompt.contains("Project instructions"));
        assert!(prompt.contains("My Project"));
    }

    #[test]
    fn load_prefers_agents_over_claude() {
        let dir = make_tempdir();
        write_file(&dir.path().join("AGENTS.md"), "from AGENTS");
        write_file(&dir.path().join("CLAUDE.md"), "from CLAUDE");

        let cfg = AgentsMdConfig::default();
        let result = load_agents_md(dir.path(), &cfg).unwrap();
        // AGENTS.md 应该优先
        assert!(result.content.unwrap().contains("from AGENTS"));
    }

    #[test]
    fn load_falls_back_to_claude() {
        let dir = make_tempdir();
        write_file(&dir.path().join("CLAUDE.md"), "from CLAUDE only");

        let cfg = AgentsMdConfig::default();
        let result = load_agents_md(dir.path(), &cfg).unwrap();
        assert!(result.found_path.is_some());
        assert!(result.content.unwrap().contains("from CLAUDE only"));
    }

    #[test]
    fn load_searches_parents() {
        let dir = make_tempdir();
        write_file(&dir.path().join("AGENTS.md"), "parent AGENTS");
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();

        let cfg = AgentsMdConfig::default();
        let result = load_agents_md(&sub, &cfg).unwrap();
        assert!(result.content.unwrap().contains("parent AGENTS"));
    }

    #[test]
    fn load_no_agents_md_returns_none() {
        let dir = make_tempdir();
        let cfg = AgentsMdConfig::default();
        let result = load_agents_md(dir.path(), &cfg).unwrap();
        assert!(result.found_path.is_none());
        assert!(result.content.is_none());
        assert!(result.system_prompt_fragment.is_none());
    }

    #[test]
    fn load_disabled_returns_none() {
        let dir = make_tempdir();
        write_file(&dir.path().join("AGENTS.md"), "should not load");
        let cfg = AgentsMdConfig {
            enabled: false,
            ..Default::default()
        };
        let result = load_agents_md(dir.path(), &cfg).unwrap();
        assert!(result.found_path.is_none());
    }

    #[test]
    fn load_rejects_oversized_file() {
        let dir = make_tempdir();
        let agents = dir.path().join("AGENTS.md");
        // 写 1KB
        write_file(&agents, &"x".repeat(1024));

        let cfg = AgentsMdConfig {
            max_size_bytes: 100, // 限 100 字节
            ..Default::default()
        };
        let result = load_agents_md(dir.path(), &cfg);
        // oversized 返 Err 或 fallback (取决于是否还有其它 file_name 命中)
        // 简化: 任何 file_name oversized 都跳过, 返 None
        assert!(result.is_ok());
        assert!(result.unwrap().found_path.is_none());
    }

    #[test]
    fn load_custom_file_names() {
        let dir = make_tempdir();
        write_file(&dir.path().join("CUSTOM.md"), "custom content");

        let cfg = AgentsMdConfig {
            file_names: vec!["CUSTOM.md".to_string()],
            ..Default::default()
        };
        let result = load_agents_md(dir.path(), &cfg).unwrap();
        assert!(result.content.unwrap().contains("custom content"));
    }
}
