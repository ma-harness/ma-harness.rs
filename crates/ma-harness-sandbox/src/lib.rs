//! # 命名约定
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-sandbox`
//! **Crate ident** (`use` 路径): `ma_harness_sandbox`
//!
//! Rust 自动从 kebab-case package name 推 snake_case crate ident.
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! ma-harness-sandbox = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_sandbox::*;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-sandbox
//!
//! ma_harness_sandbox — OS-level sandbox (Phase 2.2)
//!
//! **目标**: 在 software-level 沙箱 (typed key 白名单) 之上加 OS syscall 级强制.
//! 即使 attacker 绕开 typed key check, 也撞到 kernel 拦截.
//!
//! **支持平台**:
//! - Linux: `landlock` LSM (5.13+ 内核)
//! - macOS: `sandbox-exec` + Seatbelt profile (Phase 2.3 实现, 当前占位)
//! - Windows / 其他: `StubEnforcer` (no-op, 警告 "no OS sandbox")
//!
//! **设计原则**:
//! - fail-closed: enforce 失败立即 panic (不能 silently bypass)
//! - 业务方写 Policy 描述, Enforcer 强制
//! - 跟 `ma_harness_cordis::Context` 集成, 通过 typed key 注入
//!
//! # 用法
//!
//! ```ignore
//! use ma_harness_sandbox::{Policy, PathRule, LinuxLandlockEnforcer};
//!
//! let policy = Policy {
//!     read_paths: vec![PathRule::Subpath("/tmp".into())],
//!     write_paths: vec![PathRule::Subpath("/tmp/out".into())],
//!     ..Default::default()
//! };
//!
//! LinuxLandlockEnforcer::enforce(&policy).expect("enforce failed");
//! // 后续 read/write 都被 landlock 限制
//! ```
//!
//! **限制 (Phase 2.2)**:
//! - 不支持 syscall 全限制 (只 FS 路径白名单)
//! - 不支持网络沙箱 (用 seccomp + nix unshare, Phase 3)
//! - macOS Seatbelt 暂未实现 (Phase 2.3)

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(missing_docs)] // 2026-08-18: 内部 crate, Phase 2 release 前补 doc

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// Policy: 业务方写的沙箱规则
// ============================================================================

/// 沙箱策略 (业务方通过 typed key 注入 ctx)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// 允许读的路径规则
    pub read_paths: Vec<PathRule>,
    /// 允许写的路径规则
    pub write_paths: Vec<PathRule>,
    /// 允许执行的路径规则 (Phase 2.2 占位, Landlock 暂不区分)
    pub exec_paths: Vec<PathRule>,
    /// 是否允许网络 (Phase 2.2 占位, Landlock 暂不实现)
    pub allow_network: bool,
}

/// 单个路径规则
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathRule {
    /// 子路径匹配 (例如 `/tmp` 匹配 `/tmp/foo/bar`)
    Subpath(PathBuf),
    /// 精确路径匹配
    Exact(PathBuf),
    /// 临时目录 (系统 tmpdir + 子路径)
    TempDir,
}

impl PathRule {
    /// 判断给定路径是否匹配此规则
    pub fn matches(&self, path: &std::path::Path) -> bool {
        match self {
            PathRule::Subpath(base) => path.starts_with(base),
            PathRule::Exact(exact) => path == exact,
            PathRule::TempDir => {
                let tmp = std::env::temp_dir();
                path.starts_with(&tmp)
            }
        }
    }
}

// ============================================================================
// EnforceError
// ============================================================================

/// 沙箱强制错误
#[derive(Debug, Error)]
pub enum EnforceError {
    /// 内核不支持 Landlock (Linux < 5.13)
    #[error("landlock not supported by kernel (need Linux >= 5.13)")]
    LandlockNotSupported,

    /// Landlock ABI 版本不匹配
    #[error("landlock ABI version mismatch: expected {expected}, got {actual}")]
    LandlockAbiMismatch {
        /// 期望 ABI 版本
        expected: u32,
        /// 实际 ABI 版本
        actual: u32,
    },

    /// 添加规则失败
    #[error("failed to add landlock rule for {path:?}: {source}")]
    RuleAddFailed {
        /// 路径
        path: PathBuf,
        /// 底层错误
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },

    /// 应用规则失败 (prctl / LandlockRestrict / sandbox_init)
    #[error("failed to apply sandbox ruleset: {0}")]
    ApplyFailed(String),

    /// I/O 错误
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// Enforcer trait
// ============================================================================

/// 沙箱强制器 (跨平台抽象)
pub trait Enforcer: Send + Sync + 'static {
    /// 强制应用策略 (失败立即 panic, fail-closed)
    fn enforce(&self, policy: &Policy) -> Result<(), EnforceError>;

    /// 当前是否启用了 OS-level 强制 (用于日志 / 调试)
    fn is_active(&self) -> bool;

    /// 平台名 (例如 "linux-landlock", "macos-seatbelt", "stub")
    fn platform_name(&self) -> &'static str;
}

// ============================================================================
// Linux Landlock Enforcer
// ============================================================================

/// Linux Landlock 强制器 (Linux 5.13+)
#[cfg(target_os = "linux")]
pub struct LinuxLandlockEnforcer;

#[cfg(target_os = "linux")]
impl LinuxLandlockEnforcer {
    /// 创建一个新的 Landlock enforcer
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl Default for LinuxLandlockEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "linux")]
impl Enforcer for LinuxLandlockEnforcer {
    fn enforce(&self, policy: &Policy) -> Result<(), EnforceError> {
        use landlock::{
            Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, ABI,
        };

        // 1. 检查内核 ABI
        let abi = ABI::new_current();
        if abi < ABI::V1 {
            return Err(EnforceError::LandlockNotSupported);
        }

        // 2. 构造 ruleset (允许的 FS 操作: read / write / execute / make dir / remove 等)
        let mut ruleset = Ruleset::default()
            .handle_access(Access::from(AccessFs::ReadFile))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access ReadFile: {e}")))?
            .handle_access(Access::from(AccessFs::ReadDir))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access ReadDir: {e}")))?
            .handle_access(Access::from(AccessFs::WriteFile))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access WriteFile: {e}")))?
            .handle_access(Access::from(AccessFs::RemoveDir))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access RemoveDir: {e}")))?
            .handle_access(Access::from(AccessFs::RemoveFile))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access RemoveFile: {e}")))?
            .handle_access(Access::from(AccessFs::MakeChar))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access MakeChar: {e}")))?
            .handle_access(Access::from(AccessFs::MakeDir))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access MakeDir: {e}")))?
            .handle_access(Access::from(AccessFs::MakeReg))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access MakeReg: {e}")))?
            .handle_access(Access::from(AccessFs::MakeSock))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access MakeSock: {e}")))?
            .handle_access(Access::from(AccessFs::MakeFifo))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access MakeFifo: {e}")))?
            .handle_access(Access::from(AccessFs::MakeSym))
            .map_err(|e| EnforceError::ApplyFailed(format!("handle_access MakeSym: {e}")))?
            .create()
            .map_err(|e| EnforceError::ApplyFailed(format!("create ruleset: {e}")))?;

        // 3. 添加 read 规则
        for rule in &policy.read_paths {
            if let Some(path) = rule_to_path(rule) {
                let fd = PathFd::new(&path).map_err(|e| EnforceError::RuleAddFailed {
                    path: path.clone(),
                    source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}"))),
                })?;
                ruleset = ruleset
                    .add_rule(PathBeneath::new(fd, Access::from(AccessFs::ReadFile)))
                    .map_err(|e| EnforceError::RuleAddFailed {
                        path: path.clone(),
                        source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}"))),
                    })?
                    .add_rule(PathBeneath::new(fd, Access::from(AccessFs::ReadDir)))
                    .map_err(|e| EnforceError::RuleAddFailed {
                        path: path.clone(),
                        source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}"))),
                    })?;
            }
        }

        // 4. 添加 write 规则 (write 隐含 read)
        for rule in &policy.write_paths {
            if let Some(path) = rule_to_path(rule) {
                let fd = PathFd::new(&path).map_err(|e| EnforceError::RuleAddFailed {
                    path: path.clone(),
                    source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}"))),
                })?;
                ruleset = ruleset
                    .add_rule(PathBeneath::new(fd, Access::from(AccessFs::WriteFile)))
                    .map_err(|e| EnforceError::RuleAddFailed {
                        path: path.clone(),
                        source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}"))),
                    })?;
            }
        }

        // 5. 应用 ruleset (prctl LandlockRestrict, 不可逆)
        ruleset
            .restrict_self()
            .map_err(|e| EnforceError::ApplyFailed(format!("restrict_self: {e}")))?;

        tracing::info!(
            read_paths = policy.read_paths.len(),
            write_paths = policy.write_paths.len(),
            "landlock sandbox enforced (Linux {}.{}.{})",
            abi.major,
            abi.minor,
            abi.patch
        );
        Ok(())
    }

    fn is_active(&self) -> bool {
        true
    }

    fn platform_name(&self) -> &'static str {
        "linux-landlock"
    }
}

#[cfg(target_os = "linux")]
fn rule_to_path(rule: &PathRule) -> Option<PathBuf> {
    match rule {
        PathRule::Subpath(p) | PathRule::Exact(p) => Some(p.clone()),
        PathRule::TempDir => Some(std::env::temp_dir()),
    }
}

// ============================================================================
// macOS Seatbelt Enforcer (Phase 2.3 占位)
// ============================================================================

/// macOS Seatbelt 强制器 (Phase 2.3 占位)
#[cfg(target_os = "macos")]
pub struct MacosSeatbeltEnforcer;

#[cfg(target_os = "macos")]
impl MacosSeatbeltEnforcer {
    /// 创建一个新的 Seatbelt enforcer
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
impl Default for MacosSeatbeltEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl Enforcer for MacosSeatbeltEnforcer {
    fn enforce(&self, _policy: &Policy) -> Result<(), EnforceError> {
        // Phase 2.3: 调 sandbox-exec + profile string
        // 当前占位: no-op + warn
        tracing::warn!("macOS Seatbelt enforcer not yet implemented (Phase 2.3)");
        Ok(())
    }

    fn is_active(&self) -> bool {
        false
    }

    fn platform_name(&self) -> &'static str {
        "macos-seatbelt (stub)"
    }
}

// ============================================================================
// Stub Enforcer (其他平台, fail-open with warning)
// ============================================================================

/// Stub enforcer (Windows / 其他平台)
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub struct StubEnforcer;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl StubEnforcer {
    /// 创建一个新的 stub enforcer
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl Default for StubEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl Enforcer for StubEnforcer {
    fn enforce(&self, policy: &Policy) -> Result<(), EnforceError> {
        tracing::warn!(
            "no OS-level sandbox on this platform; software-level whitelist still in effect. \
             policy: {} read paths, {} write paths",
            policy.read_paths.len(),
            policy.write_paths.len()
        );
        Ok(())
    }

    fn is_active(&self) -> bool {
        false
    }

    fn platform_name(&self) -> &'static str {
        "stub (no OS sandbox)"
    }
}

// ============================================================================
// Platform-default enforcer
// ============================================================================

/// 平台默认 enforcer (Linux: Landlock, macOS: Seatbelt stub, 其他: Stub)
#[cfg(target_os = "linux")]
pub type DefaultEnforcer = LinuxLandlockEnforcer;

#[cfg(target_os = "macos")]
pub type DefaultEnforcer = MacosSeatbeltEnforcer;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub type DefaultEnforcer = StubEnforcer;

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn path_rule_subpath_matches_prefix() {
        let rule = PathRule::Subpath(PathBuf::from("/tmp"));
        assert!(rule.matches(Path::new("/tmp/foo")));
        assert!(rule.matches(Path::new("/tmp")));
        assert!(!rule.matches(Path::new("/etc/passwd")));
    }

    #[test]
    fn path_rule_exact_matches_only_self() {
        let rule = PathRule::Exact(PathBuf::from("/tmp/foo"));
        assert!(rule.matches(Path::new("/tmp/foo")));
        assert!(!rule.matches(Path::new("/tmp/foo/bar")));
        assert!(!rule.matches(Path::new("/tmp/baz")));
    }

    #[test]
    fn path_rule_tempdir_matches_system_tmp() {
        let rule = PathRule::TempDir;
        let tmp = std::env::temp_dir();
        assert!(rule.matches(&tmp));
        assert!(rule.matches(&tmp.join("subpath")));
        assert!(!rule.matches(Path::new("/etc/passwd")));
    }

    #[test]
    fn policy_default_is_empty_fail_closed() {
        let policy = Policy::default();
        assert!(policy.read_paths.is_empty());
        assert!(policy.write_paths.is_empty());
        assert!(!policy.allow_network);
    }

    #[test]
    fn policy_serde_roundtrip() {
        let policy = Policy {
            read_paths: vec![PathRule::TempDir, PathRule::Subpath("/data".into())],
            write_paths: vec![PathRule::Subpath("/data/out".into())],
            exec_paths: vec![],
            allow_network: true,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let back: Policy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }

    #[test]
    fn default_enforcer_is_constructible() {
        // 不真 enforce, 只验 DefaultEnforcer 可构造 (跨平台)
        let _e: DefaultEnforcer = DefaultEnforcer::new();
    }
}
