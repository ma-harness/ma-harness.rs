//! ma_harness_plugin_fs ?first-party plugin: 文件系统?/ ?//!
//! **设计**: seam 公开 API 风格.
//!
//! **Week 5-6 实装**: 3 个核心方?(read_file / write_file / list_dir) + 路径白名?//! (READ_ALLOW_LIST / WRITE_ALLOW_LIST typed key,业务?set 控制 sandbox).
//!
//! **Phase 1 简?*:
//! - 白名单用 typed key (Vec<String> 路径前缀),**?* ?landlock syscall
//! - 没有 symlink 防绕?(Phase 2 ?
//! - 路径不允许绝?(相对 cwd, Phase 2 加绝对路径白名单)

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;

// ============================================================================
// 公开 typed key
// ============================================================================

/// 允许读的路径白名?(前缀匹配, 业务?set)
pub static READ_ALLOW_LIST: ma_harness_cordis::CtxKey<Vec<String>> = ctx_key!("read_allow_list");

/// 允许写的路径白名?(前缀匹配)
pub static WRITE_ALLOW_LIST: ma_harness_cordis::CtxKey<Vec<String>> = ctx_key!("write_allow_list");

// ============================================================================
// 错误
// ============================================================================

/// Fs plugin 错误
#[derive(Debug, Error)]
pub enum FsError {
    /// 路径不在白名?    #[error("path {path:?} not in {list:?} allow list")]
    NotInAllowList {
        /// 路径
        path: String,
        /// 白名单名?("read" / "write")
        list: &'static str,
    },

    /// 路径包含 ".." (防穿?
    #[error("path {0:?} contains '..' (forbidden)")]
    PathTraversal(String),

    /// IO 错误
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// 编码错误
    #[error("invalid utf-8 in file: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

// ============================================================================
// Entry / DirEntry
// ============================================================================

/// 目录?#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    /// 文件 / 目录?    pub name: String,
    /// 是否目录
    pub is_dir: bool,
    /// 文件大小 (bytes, 目录?0)
    pub size: u64,
}

// ============================================================================
// FsService
// ============================================================================

/// Fs service ?沙箱文件?/ ?pub struct FsService;

impl FsService {
    /// 读文?(路径必须以白名单任一前缀开?
    pub async fn read_file(&self, ctx: &Context, path: &Path) -> Result<String, FsError> {
        let safe = self.sanitize_path(ctx, path, "read")?;
        self.check_allow_list(ctx, &safe, "read")?;
        let content = fs::read_to_string(&safe).await?;
        Ok(content)
    }

    /// 写文?(覆盖, 路径必须在写白名?
    pub async fn write_file(
        &self,
        ctx: &Context,
        path: &Path,
        content: &str,
    ) -> Result<(), FsError> {
        let safe = self.sanitize_path(ctx, path, "write")?;
        self.check_allow_list(ctx, &safe, "write")?;
        fs::write(&safe, content).await?;
        Ok(())
    }

    /// 列目录
    pub async fn list_dir(&self, ctx: &Context, path: &Path) -> Result<Vec<DirEntry>, FsError> {
        let safe = self.sanitize_path(ctx, path, "read")?;
        self.check_allow_list(ctx, &safe, "read")?;
        let mut entries = Vec::new();
        let mut rd = fs::read_dir(&safe).await?;
        while let Some(e) = rd.next_entry().await? {
            let meta = e.metadata().await?;
            entries.push(DirEntry {
                name: e.file_name().to_string_lossy().to_string(),
                is_dir: meta.is_dir(),
                size: if meta.is_dir() { 0 } else { meta.len() },
            });
        }
        Ok(entries)
    }

    /// 路径消毒: 阻止 "..", 解析到绝对路?    fn sanitize_path(&self, _ctx: &Context, path: &Path, op: &str) -> Result<PathBuf, FsError> {
        let s = path.to_string_lossy().to_string();
        // 防穿?        if s.contains("..") {
            return Err(FsError::PathTraversal(s));
        }
        // Phase 1: 简?path 拼接, 假设 caller 传相对或绝对路径
        // Phase 2: 强制 relative + ?ctx.cwd() ?        tracing::debug!(path = %s, op, "path sanitized");
        Ok(PathBuf::from(path))
    }

    /// 检查路径是否在白名单内
    fn check_allow_list(&self, ctx: &Context, path: &Path, list: &'static str) -> Result<(), FsError> {
        let allows: Vec<String> = match list {
            "read" => ctx.get(READ_ALLOW_LIST).unwrap_or_default(),
            "write" => ctx.get(WRITE_ALLOW_LIST).unwrap_or_default(),
            _ => return Err(FsError::NotInAllowList {
                path: path.to_string_lossy().to_string(),
                list,
            }),
        };
        if allows.is_empty() {
            // Phase 1: 白名单空 = 拒绝所?(fail-closed)
            return Err(FsError::NotInAllowList {
                path: path.to_string_lossy().to_string(),
                list,
            });
        }
        let path_str = path.to_string_lossy();
        for prefix in &allows {
            if path_str.starts_with(prefix) {
                return Ok(());
            }
        }
        Err(FsError::NotInAllowList {
            path: path_str.to_string(),
            list,
        })
    }
}

impl CordisService for FsService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(FsService)
    }
    fn name(&self) -> &str {
        "fs"
    }
}

impl SeamService for FsService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(FsService)
    }
    fn name(&self) -> &str {
        "fs"
    }
}

// ============================================================================
// Plugin: FsPlugin
// ============================================================================

/// Fs plugin
pub struct FsPlugin;

impl CordisPlugin for FsPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = <FsService as ma_harness_cordis::Service>::install(ctx)?;
        ctx.inject(std::sync::Arc::new(svc));
        // 默认白名单空 (fail-closed, 业务方必须显?set)
        ctx.set(READ_ALLOW_LIST, Vec::<String>::new());
        ctx.set(WRITE_ALLOW_LIST, Vec::<String>::new());
        Ok(())
    }
    fn name(&self) -> &str {
        "fs"
    }
}

impl SeamPlugin for FsPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "fs"
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn ctx_with_temp_allow_list(tmp: &tempfile::TempDir) -> Context {
        let ctx = Context::new();
        // 白名?= temp 目录绝对路径
        let tmp_path = tmp.path().to_string_lossy().to_string();
        ctx.set(READ_ALLOW_LIST, vec![tmp_path.clone()]);
        ctx.set(WRITE_ALLOW_LIST, vec![tmp_path]);
        ctx
    }

    #[tokio::test]
    async fn read_file_in_allow_list() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("hello.txt");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            f.write_all(b"hello world").unwrap();
        }
        let ctx = ctx_with_temp_allow_list(&tmp);
        let svc = FsService;
        let content = svc.read_file(&ctx, &file_path).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn write_file_in_allow_list() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("out.txt");
        let ctx = ctx_with_temp_allow_list(&tmp);
        let svc = FsService;
        svc.write_file(&ctx, &file_path, "written")
            .await
            .unwrap();
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "written");
    }

    #[tokio::test]
    async fn read_file_outside_allow_list_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_temp_allow_list(&tmp);
        let svc = FsService;
        // 试图?/etc/passwd (不在白名?
        let path = std::path::Path::new("/etc/passwd");
        let result = svc.read_file(&ctx, path).await;
        assert!(matches!(result, Err(FsError::NotInAllowList { .. })));
    }

    #[tokio::test]
    async fn read_file_with_empty_allow_list_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = Context::new();
        ctx.set(READ_ALLOW_LIST, Vec::<String>::new());
        ctx.set(WRITE_ALLOW_LIST, Vec::<String>::new());
        let svc = FsService;
        let file_path = tmp.path().join("foo.txt");
        std::fs::write(&file_path, "x").unwrap();
        let result = svc.read_file(&ctx, &file_path).await;
        assert!(matches!(result, Err(FsError::NotInAllowList { .. })));
    }

    #[tokio::test]
    async fn path_traversal_blocked() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_with_temp_allow_list(&tmp);
        let svc = FsService;
        let path = std::path::PathBuf::from("../etc/passwd");
        let result = svc.read_file(&ctx, &path).await;
        assert!(matches!(result, Err(FsError::PathTraversal(_))));
    }

    #[tokio::test]
    async fn list_dir_in_allow_list() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "x").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "y").unwrap();
        std::fs::create_dir(tmp.path().join("subdir")).unwrap();

        let ctx = ctx_with_temp_allow_list(&tmp);
        let svc = FsService;
        let entries = svc.list_dir(&ctx, tmp.path()).await.unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));
        assert!(names.contains(&"subdir"));
        // subdir 应是 dir
        let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
        assert!(subdir.is_dir);
        assert_eq!(subdir.size, 0);
    }
}
