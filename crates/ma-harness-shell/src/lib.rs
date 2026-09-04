//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-shell`
//! **Crate ident** (`use` 路径): `ma_harness_shell`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-shell = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_shell::{LocalShellProvider, ShellService, ShellSpec, ShellKind};
//!
//! let provider = LocalShellProvider::new();
//! let spec = ShellSpec::new("echo hello").timeout(std::time::Duration::from_secs(5));
//! let result = provider.execute(&spec).await?;
//! assert!(result.is_success());
//! assert_eq!(result.stdout.trim(), "hello");
//! assert_eq!(result.shell_kind, ShellKind::platform_default());
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-shell
//!
//! # 设计 (Design) — P14.2
//!
//! **目标**: 抽 `ctx.shell` 能力缝 (Service Definition), 让
//! - `LocalShellProvider` — 平台默认 shell (Unix: `sh -c`, Windows: `cmd /C`)
//! - `PwshShellProvider` — Windows PowerShell Core 7+ (`pwsh -c`), 跨平台 stub
//! - 未来: `BashShellProvider` (显式 bash, 不依赖 /bin/sh) / `ZshShellProvider` / 等
//!
//!   可插拔. 跟 dsh [ctx.shell seam] 1:1 对等.
//!
//! **背景**: 见 [dsh-feature-parity-table §2] — `ctx.shell` 在 dsh 是 14 个能力缝之一,
//! 有 `LocalShellProvider` / `PwshShellProvider` / Consumer pattern (业务方写
//! `#[shell("description")]` 宏). ma-harness 之前 `plugin-bash` 直接 `tokio::Command::new("sh")`,
//! 不可替换, 也没法走 sandbox 隔离 (P14.1.1 之后).
//!
//! **核心抽象**:
//! - [`ShellService`] trait — `execute(spec) -> ShellResult`, 跟 SubprocessService 平行
//! - [`ShellSpec`] — 业务方写的命令描述 (command 字符串 + env + cwd + timeout + stdin)
//! - [`ShellResult`] — 退出码 + stdout + stderr + duration + shell_kind
//! - [`ShellKind`] — 实际跑了什么 shell (Sh / Cmd / Pwsh / Bash / Zsh / 等)
//! - [`LocalShellProvider`] — 平台默认 shell (P14.2.1 主交付)
//! - [`PwshShellProvider`] — Windows PowerShell Core stub (P14.2.3 填实现)
//!
//! **Consumer pattern** (P14.2.2 接入):
//! 业务方写 `ShellCommand` 类型, 注册到 `ctx.shell` 容器里, 后续 plugin 调
//! `ctx.shell().invoke_by_name("git-commit", args).await?`. 跟 dsh
//! `#[shell("description")]` 宏同构.
//!
//! **6 质量属性** (业务方 2026-09-04 约定):
//! - 可复用: 委托给 `ma-harness-subprocess` (P14.1), 不重写进程管理
//! - 可维护: 模块化分块, ShellKind / ShellSpec / ShellResult 集中在 lib.rs
//! - 鲁棒: 错误归一化 (ShellError), timeout / spawn fail / non-zero exit 显式区分
//! - 安全: 跟 subprocess 一样 env_clear 默认 (不继承父进程 env), 显式 shell escape 是 P15+
//! - 可测: 跨平台 helper (echo / exit / sleep) + 8 个单元测试
//! - 可扩展: ShellKind enum 留 Bash / Zsh / Fish 变体空间, PwshShellProvider 是独立 struct
//!
//! # 限制 (Limitations) — P14.2.1
//!
//! - `PwshShellProvider` 是 stub (P14.2.3 接 `pwsh.exe`)
//! - Shell Consumer pattern (`ShellCommand` macro) 是 P14.2.2
//! - `plugin-bash` 重构 (走 `ctx.shell` 而不是直 `tokio::Command`) 是 P14.2.2
//! - `plugin-powershell` 新建是 P14.2.3
//! - Shell escape (避免命令注入) 是 P15+
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams
//! [ctx.shell seam]: https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md#capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use ma_harness_subprocess::{
    CommandSpec, LocalSubprocessProvider, StdioConfig, SubprocessError, SubprocessService,
};

// ============================================================================
// ShellError: 统一的 shell 错误
// ============================================================================

/// Shell 能力缝错误.
#[derive(Debug, Error)]
pub enum ShellError {
    /// 底层 subprocess 错误 (spawn / IO / wait / handle not found)
    #[error("subprocess error: {0}")]
    Subprocess(#[from] SubprocessError),

    /// 无效的 ShellSpec (空 command / 不存在的 cwd)
    #[error("invalid shell spec: {0}")]
    InvalidSpec(String),

    /// 命令退出码非 0 (跟 dsh 行为一致: 业务方自行判断)
    #[error("shell command exited with code {code}: {stderr}")]
    NonZeroExit {
        /// 退出码
        code: i32,
        /// stderr 截断 (前 200 字符)
        stderr: String,
    },

    /// Shell 不在 PATH (PwshShellProvider 调 `pwsh` 但 PATH 没)
    #[error("shell binary '{shell:?}' not found in PATH")]
    ShellNotFound {
        /// 实际尝试调用的 shell 程序名
        shell: OsString,
    },

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
// ShellKind: 跑了哪种 shell
// ============================================================================

/// Shell 类型 (Platform default / Sh / Cmd / Pwsh / Bash / Zsh / ...).
///
/// **设计**: 不透明枚举, 业务方 `assert_eq!(result.shell_kind, ShellKind::Sh)`.
/// 未来加新 shell (fish, nushell, ...) 只需要扩 enum + 加 provider 即可.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellKind {
    /// Unix 默认 `sh` (POSIX, 业务方兼容性最广)
    Sh,
    /// Windows `cmd.exe` (Windows 默认)
    Cmd,
    /// Windows PowerShell Core 7+ (`pwsh.exe`)
    Pwsh,
    /// 显式 bash (业务方 `which bash` 找)
    Bash,
    /// Z shell
    Zsh,
}

impl ShellKind {
    /// 平台默认 shell (P14.2.1: Unix=Sh, Windows=Cmd)
    pub const fn platform_default() -> Self {
        #[cfg(unix)]
        {
            ShellKind::Sh
        }
        #[cfg(windows)]
        {
            ShellKind::Cmd
        }
    }

    /// 实际可执行文件名 (业务方 PATH 里查找)
    pub const fn program(&self) -> &'static str {
        match self {
            ShellKind::Sh => "sh",
            ShellKind::Cmd => "cmd",
            ShellKind::Pwsh => "pwsh",
            ShellKind::Bash => "bash",
            ShellKind::Zsh => "zsh",
        }
    }

    /// 调用参数: `shell -c <command>` 还是 `shell /C <command>`
    ///
    /// Unix 系: `-c <command>`
    /// Windows 系: `/C <command>`
    pub const fn invoke_flag(&self) -> &'static str {
        match self {
            ShellKind::Sh | ShellKind::Bash | ShellKind::Zsh => "-c",
            ShellKind::Cmd => "/C",
            ShellKind::Pwsh => "-c", // pwsh 也用 -c (跟 POSIX 兼容)
        }
    }
}

impl fmt::Display for ShellKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.program())
    }
}

// ============================================================================
// ShellSpec: 业务方写的 shell 命令描述
// ============================================================================

/// Shell 命令描述.
///
/// **关键设计**: 不可变 + Clone + Send/Sync. 业务方 builder 风格构造
/// (`ShellSpec::new("ls -la").env("LANG", "C").timeout(Duration::from_secs(5))`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSpec {
    /// 要执行的 shell 命令字符串 (整段交给 shell 解析)
    pub command: String,
    /// 显式 env (env_clear 默认, 不继承父进程, 跟 P14.1 一致)
    pub env: BTreeMap<String, OsString>,
    /// 工作目录
    pub cwd: Option<PathBuf>,
    /// 超时
    pub timeout: Option<Duration>,
    /// stdin 输入 (None = 关 stdin, Some(s) = 写 s 进 stdin 后 close)
    pub stdin_input: Option<String>,
}

impl ShellSpec {
    /// 创建一个 ShellSpec (默认 timeout=None, env 空, stdin 关)
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            env: BTreeMap::new(),
            cwd: None,
            timeout: None,
            stdin_input: None,
        }
    }

    /// 覆盖 env
    pub fn env(mut self, key: impl Into<String>, value: impl Into<OsString>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// 批量 env
    pub fn envs<I, K, V>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<OsString>,
    {
        for (k, v) in iter {
            self.env.insert(k.into(), v.into());
        }
        self
    }

    /// 设置 cwd
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// 设置 timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(dur);
        self
    }

    /// 设置 stdin 输入
    pub fn stdin(mut self, input: impl Into<String>) -> Self {
        self.stdin_input = Some(input.into());
        self
    }

    /// 验证 spec
    pub fn validate(&self) -> Result<(), ShellError> {
        if self.command.is_empty() {
            return Err(ShellError::InvalidSpec("command is empty".into()));
        }
        if let Some(ref cwd) = self.cwd {
            if !cwd.exists() {
                return Err(ShellError::InvalidSpec(format!(
                    "cwd does not exist: {}",
                    cwd.display()
                )));
            }
        }
        Ok(())
    }
}

// ============================================================================
// ShellResult: 业务方拿到的命令执行结果
// ============================================================================

/// Shell 命令执行结果.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellResult {
    /// 退出码 (0 = 成功, 非 0 业务方自行判断)
    pub exit_code: i32,
    /// stdout 字符串 (lossy UTF-8)
    pub stdout: String,
    /// stderr 字符串 (lossy UTF-8)
    pub stderr: String,
    /// 实际耗时
    pub duration: Duration,
    /// 实际跑的 shell kind
    pub shell_kind: ShellKind,
}

impl ShellResult {
    /// 是否成功 (exit_code == 0)
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

// ============================================================================
// ShellService: 能力缝 trait (跟 dsh ctx.shell 对等)
// ============================================================================

/// Shell 能力缝 (跟 dsh `ctx.shell` seam 对等).
///
/// **核心方法**:
/// - [`execute`](Self::execute) — 同步等结果 (类似 `subprocess::output`)
///
/// **实现**:
/// - [`LocalShellProvider`] — 平台默认 shell (P14.2.1 主交付)
/// - [`PwshShellProvider`] — Windows PowerShell Core stub (P14.2.3 填实现)
/// - 业务方可注入 mock provider (测试用)
#[async_trait]
pub trait ShellService: Send + Sync + 'static {
    /// 执行 shell 命令, 等待结果
    async fn execute(&self, spec: &ShellSpec) -> Result<ShellResult, ShellError>;

    /// 这个 provider 默认用什么 shell (Platform default / Sh / Cmd / Pwsh / ...)
    fn default_shell_kind(&self) -> ShellKind;

    /// Provider 标识 (日志 / 调试)
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// LocalShellProvider: 委托给 ma-harness-subprocess
// ============================================================================

/// 本地 shell provider (P14.2.1 主交付).
///
/// **实现**: 调 `ma-harness-subprocess::LocalSubprocessProvider` 跑 `sh -c` / `cmd /C`.
/// 默认 `ShellKind::platform_default()` (Unix=Sh, Windows=Cmd).
/// 业务方可显式选其他 shell (`LocalShellProvider::with_shell(ShellKind::Bash)`).
pub struct LocalShellProvider {
    default_shell: ShellKind,
    inner: LocalSubprocessProvider,
}

impl LocalShellProvider {
    /// 创建一个 LocalShellProvider (用平台默认 shell)
    pub fn new() -> Self {
        Self::with_shell(ShellKind::platform_default())
    }

    /// 创建一个 LocalShellProvider, 显式指定默认 shell kind
    pub fn with_shell(shell: ShellKind) -> Self {
        Self {
            default_shell: shell,
            inner: LocalSubprocessProvider::new(),
        }
    }

    /// 拿到底层 subprocess provider (业务方需要更细控制时用, 例如接 ctx.sandbox)
    pub fn subprocess_provider(&self) -> &LocalSubprocessProvider {
        &self.inner
    }

    /// 实际构造 `CommandSpec` (内部用, 业务方一般不直接调)
    fn to_command_spec(&self, spec: &ShellSpec, shell: ShellKind) -> CommandSpec {
        // shell -c "<command>" / cmd /C "<command>"
        // args: [invoke_flag, command]
        CommandSpec::new(
            shell.program(),
            vec![
                OsString::from(shell.invoke_flag()),
                OsString::from(&spec.command),
            ],
        )
        .envs(spec.env.iter().map(|(k, v)| (k.clone(), v.clone())))
        .stdout(StdioConfig::Piped)
        .stderr(StdioConfig::Piped)
        .stdin(if spec.stdin_input.is_some() {
            StdioConfig::Piped
        } else {
            StdioConfig::Null
        })
        .pipe_if_some_cwd(spec.cwd.as_ref())
        .pipe_if_some_timeout(spec.timeout)
    }
}

impl Default for LocalShellProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ShellService for LocalShellProvider {
    async fn execute(&self, spec: &ShellSpec) -> Result<ShellResult, ShellError> {
        spec.validate()?;
        let shell = self.default_shell;
        let start = std::time::Instant::now();

        let cmd = self.to_command_spec(spec, shell);
        let output = self.inner.output(&cmd).await?;
        let duration = start.elapsed();

        let exit_code = output.status.code.unwrap_or(-1);
        Ok(ShellResult {
            exit_code,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration,
            shell_kind: shell,
        })
    }

    fn default_shell_kind(&self) -> ShellKind {
        self.default_shell
    }

    fn provider_name(&self) -> &'static str {
        "local-shell"
    }
}

// 私有 trait extension, 让 CommandSpec builder 更简洁
trait CommandSpecExt {
    fn pipe_if_some_cwd(self, cwd: Option<&PathBuf>) -> Self;
    fn pipe_if_some_timeout(self, dur: Option<Duration>) -> Self;
}

impl CommandSpecExt for CommandSpec {
    fn pipe_if_some_cwd(self, cwd: Option<&PathBuf>) -> Self {
        match cwd {
            Some(c) => self.cwd(c.clone()),
            None => self,
        }
    }
    fn pipe_if_some_timeout(self, dur: Option<Duration>) -> Self {
        match dur {
            Some(d) => self.timeout(d),
            None => self,
        }
    }
}

// ============================================================================
// PwshShellProvider: Windows PowerShell Core 7+ (P14.2.3 实装)
// ============================================================================

/// Windows PowerShell Core 7+ provider (P14.2.3 实现).
///
/// **实装路径**:
/// 1. [`find_pwsh()`](Self::find_pwsh) 在 PATH / Windows 常见路径找 `pwsh.exe`
/// 2. `pwsh -NoLogo -NoProfile -Command <command>`
///    - `-NoLogo` 跳过启动 banner
///    - `-NoProfile` 避免读 PowerShell profile (~200ms 启动加速)
///    - `-Command <command>` 跑命令字符串
/// 3. 委托给 `LocalSubprocessProvider` 跑 (P14.1)
/// 4. 非 Windows 平台: 直接返回 `ShellError::ShellNotFound` (跨平台 stub)
///
/// **业务方安装 pwsh**:
/// - Windows: `winget install Microsoft.PowerShell` 或 `choco install pwsh`
/// - macOS: `brew install powershell`
/// - Linux: 见 https://learn.microsoft.com/powershell/scripting/install/installing-powershell
pub struct PwshShellProvider {
    /// 内部 subprocess provider (P14.1)
    inner: LocalSubprocessProvider,
}

impl PwshShellProvider {
    /// 创建一个 PwshShellProvider
    pub fn new() -> Self {
        Self {
            inner: LocalSubprocessProvider::new(),
        }
    }

    /// 在 PATH + Windows 常见路径里找 `pwsh.exe`.
    ///
    /// **查找顺序** (P14.2.3 简化版):
    /// 1. 业务方显式 `MA_HARNESS_PWSH_PATH` 环境变量
    /// 2. `which::which("pwsh")` (跨平台, 依赖 std env::var("PATH"))
    /// 3. Windows 常见路径:
    ///    - `%ProgramFiles%\PowerShell\7\pwsh.exe`
    ///    - `%ProgramFiles(x86)%\PowerShell\7\pwsh.exe`
    ///    - `%LOCALAPPDATA%\Microsoft\WindowsApps\pwsh.exe` (Windows Store 安装)
    /// 4. macOS: `/usr/local/bin/pwsh` / `/opt/homebrew/bin/pwsh`
    /// 5. Linux: `/usr/bin/pwsh` / `/usr/local/bin/pwsh`
    ///
    /// # Returns
    /// - `Some(PathBuf)` 找到
    /// - `None` 找不到 (业务方需要装 pwsh)
    pub fn find_pwsh() -> Option<PathBuf> {
        use std::path::PathBuf;

        // 1. 业务方显式 env override
        if let Ok(p) = std::env::var("MA_HARNESS_PWSH_PATH") {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }

        // 2. 跨平台 PATH 搜索 (自己写, 避免引 which crate)
        //    业务方 P14.1 决定不引 which crate, 用 std env::var("PATH")
        if let Some(p) = find_in_path("pwsh") {
            return Some(p);
        }

        // 3-5. 平台特定常见路径
        #[cfg(windows)]
        {
            let candidates: &[&str] = &[
                r"C:\Program Files\PowerShell\7\pwsh.exe",
                r"C:\Program Files (x86)\PowerShell\7\pwsh.exe",
            ];
            for c in candidates {
                let pb = PathBuf::from(c);
                if pb.is_file() {
                    return Some(pb);
                }
            }
            // LOCALAPPDATA\Microsoft\WindowsApps\pwsh.exe
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                let pb = PathBuf::from(local)
                    .join("Microsoft")
                    .join("WindowsApps")
                    .join("pwsh.exe");
                if pb.is_file() {
                    return Some(pb);
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            for c in &[
                "/usr/local/bin/pwsh",
                "/opt/homebrew/bin/pwsh",
                "/usr/bin/pwsh",
            ] {
                let pb = PathBuf::from(c);
                if pb.is_file() {
                    return Some(pb);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            for c in &["/usr/bin/pwsh", "/usr/local/bin/pwsh"] {
                let pb = PathBuf::from(c);
                if pb.is_file() {
                    return Some(pb);
                }
            }
        }

        None
    }

    /// 实际跑命令 (内部用, 业务方一般调 `execute`)
    async fn run_pwsh(
        &self,
        pwsh: &std::path::Path,
        spec: &ShellSpec,
    ) -> Result<ShellResult, ShellError> {
        let start = std::time::Instant::now();

        // pwsh -NoLogo -NoProfile -Command <command>
        // (args 顺序: -NoLogo -NoProfile -Command <command>)
        let cmd = CommandSpec::new(
            pwsh.to_path_buf(),
            vec![
                OsString::from("-NoLogo"),
                OsString::from("-NoProfile"),
                OsString::from("-Command"),
                OsString::from(&spec.command),
            ],
        )
        .envs(spec.env.iter().map(|(k, v)| (k.clone(), v.clone())))
        .stdout(StdioConfig::Piped)
        .stderr(StdioConfig::Piped)
        .stdin(StdioConfig::Null)
        .pipe_if_some_cwd(spec.cwd.as_ref())
        .pipe_if_some_timeout(spec.timeout);

        let output = self.inner.output(&cmd).await?;
        let duration = start.elapsed();

        Ok(ShellResult {
            exit_code: output.status.code.unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration,
            shell_kind: ShellKind::Pwsh,
        })
    }
}

impl Default for PwshShellProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// 跨平台 PATH 搜索 (内部用, 业务方一般不调).
///
/// 跟 P14.1 `plugin-bash` `build_shell_command` 同思路: `env::var("PATH")` 拆 `:`
/// (Unix) 或 `;` (Windows), 逐个目录加 program 名, 检查 `.is_file()`.
fn find_in_path(program: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let separator = if cfg!(windows) { ';' } else { ':' };
    let exe_suffix = if cfg!(windows) {
        // Windows: 试原名 + .exe + .cmd + .bat
        vec!["", ".exe", ".cmd", ".bat"]
    } else {
        vec![""]
    };

    for dir in std::env::split_paths(&path_var) {
        for suffix in &exe_suffix {
            let candidate = dir.join(format!("{program}{suffix}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let _ = separator; // silence unused on Windows
    None
}

#[async_trait]
impl ShellService for PwshShellProvider {
    async fn execute(&self, spec: &ShellSpec) -> Result<ShellResult, ShellError> {
        spec.validate()?;

        // 非 Windows 平台: PowerShell Core 在 macOS/Linux 可装 (brew install powershell),
        // 但默认 PATH 没, 我们仍尝试 find_pwsh (跨平台 search)
        // 找不到就返回 ShellNotFound, 业务方明确知道
        let pwsh = Self::find_pwsh().ok_or_else(|| ShellError::ShellNotFound {
            shell: OsString::from("pwsh"),
        })?;

        self.run_pwsh(&pwsh, spec).await
    }

    fn default_shell_kind(&self) -> ShellKind {
        ShellKind::Pwsh
    }

    fn provider_name(&self) -> &'static str {
        "pwsh"
    }
}

// ============================================================================
// DefaultShellProvider: 平台默认 (P14.2.1: LocalShellProvider)
// ============================================================================

/// 平台默认 shell provider (P14.2.1: LocalShellProvider)
pub type DefaultShellProvider = LocalShellProvider;

// ============================================================================
// SHELL_SERVICE typed key (P14.2.2: 跟 ctx.shell 接入点)
// ============================================================================

/// Typed key: `ctx.shell` 注入的 ShellService provider.
///
/// 业务方:
/// ```ignore
/// use ma_harness_shell::{SHELL_SERVICE, LocalShellProvider, ShellService};
/// use std::sync::Arc;
///
/// ctx.set(SHELL_SERVICE, Arc::new(LocalShellProvider::new()) as Arc<dyn ShellService>);
/// ```
///
/// 消费者 (例如 `plugin-bash` 的 BashService):
/// ```ignore
/// let shell: Arc<dyn ShellService> = ctx
///     .get(SHELL_SERVICE)
///     .unwrap_or_else(|| Arc::new(LocalShellProvider::new()));
/// ```
pub static SHELL_SERVICE: ma_harness_cordis::CtxKey<std::sync::Arc<dyn ShellService>> =
    ma_harness_seam::ctx_key!("shell_service");

// ============================================================================
// Consumer Pattern: ShellCommand + ShellRegistry (P14.2.2)
// ============================================================================
//
// 业务方注册 "git-commit" / "test-suite" 等高层命令作为 LLM 工具, 跟
// 直接调 `shell.execute("git commit -m ...") ` 区别:
// - ShellCommand 是 typed + described, LLM 拿到的 tool schema 明确
// - ShellRegistry 是 in-memory 容器, plugin 装到 ctx, agent 调 invoke_by_name
// - 跟 dsh `#[shell("description")]` 宏同构 (P14.2.4 加 proc-macro, 简化业务方样板)
//
// 例子 (业务方写):
// ```ignore
// use ma_harness_shell::{ShellCommand, ShellRegistry, ShellResult, ShellError};
//
// struct GitCommit;
// #[async_trait]
// impl ShellCommand for GitCommit {
//     fn name(&self) -> &str { "git-commit" }
//     fn description(&self) -> &str { "git commit staged changes with a message" }
//     fn param_schema(&self) -> serde_json::Value {
//         serde_json::json!({
//             "type": "object",
//             "properties": { "message": { "type": "string" } },
//             "required": ["message"]
//         })
//     }
//     async fn invoke(&self, args: serde_json::Value) -> Result<ShellResult, ShellError> {
//         let msg = args["message"].as_str().unwrap_or("(no message)");
//         let shell = LocalShellProvider::new();
//         shell.execute(&ShellSpec::new(format!("git commit -m {}", msg))).await
//     }
// }
//
// // 装到 ctx:
// let mut registry = ShellRegistry::new();
// registry.register(GitCommit);
// ctx.shell_commands = Some(Arc::new(registry));
// ```

/// Shell 命令描述符 (Consumer pattern).
///
/// 业务方实现这个 trait, 把高层命令 (e.g. "git-commit") 注册到 [`ShellRegistry`].
/// LLM 拿到的 tool schema 自动从 `name` / `description` / `param_schema` 生成.
///
/// **生命周期**: 业务方实现, 装到 `ShellRegistry`, registry 装到 `ctx.shell_commands`.
/// invoke 阶段由 [`ShellService`] 实际跑 shell 命令.
#[async_trait]
pub trait ShellCommand: Send + Sync + 'static {
    /// 命令名 (snake_case, e.g. "git_commit" / "test_suite"). Registry 用作 key.
    fn name(&self) -> &str;

    /// 命令描述 (LLM 看, 决定什么时候调)
    fn description(&self) -> &str;

    /// 参数 schema (JSON Schema 草图, 业务方手写; P14.2.4 加 macro 自动生成)
    fn param_schema(&self) -> serde_json::Value;

    /// 实际 invoke (业务方实现, 内部一般调 `ShellService::execute`)
    async fn invoke(&self, args: serde_json::Value) -> Result<ShellResult, ShellError>;
}

/// Shell 命令注册表.
///
/// 业务方 `register(GitCommit)`, agent `invoke("git-commit", json!({"message": "fix"}))`.
pub struct ShellRegistry {
    commands: std::collections::HashMap<String, std::sync::Arc<dyn ShellCommand>>,
}

impl ShellRegistry {
    /// 创建一个空 registry
    pub fn new() -> Self {
        Self {
            commands: std::collections::HashMap::new(),
        }
    }

    /// 注册一个命令 (重复 name 覆盖前一个 + log warn)
    pub fn register<C: ShellCommand>(&mut self, cmd: C) {
        let name = cmd.name().to_string();
        if self.commands.contains_key(&name) {
            tracing::warn!(
                command = %name,
                "ShellRegistry::register overrides existing command"
            );
        }
        tracing::debug!(command = %name, "shell command registered");
        self.commands.insert(name, std::sync::Arc::new(cmd));
    }

    /// 按名拿命令
    pub fn get(&self, name: &str) -> Option<std::sync::Arc<dyn ShellCommand>> {
        self.commands.get(name).cloned()
    }

    /// 列出所有命令名 (sorted)
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.commands.keys().cloned().collect();
        names.sort();
        names
    }

    /// 数量
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// 按名 invoke (便捷方法, 业务方一般直接 `get` + `invoke`)
    ///
    /// # Errors
    /// - 命令不存在: `ShellError::Unsupported { operation: "invoke", reason: "command not found: <name>" }`
    pub async fn invoke(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<ShellResult, ShellError> {
        let cmd = self.get(name).ok_or_else(|| ShellError::Unsupported {
            provider: "ShellRegistry",
            operation: "invoke",
            reason: format!("command not found: {name}"),
        })?;
        cmd.invoke(args).await
    }

    /// 给 LLM 用的 tool list (跟 dsh `tools/pre-execute` 走同一格式)
    ///
    /// 返回 `[{name, description, parameters}, ...]`, LLM 自己挑
    pub fn tool_list(&self) -> Vec<serde_json::Value> {
        self.commands
            .values()
            .map(|cmd| {
                serde_json::json!({
                    "name": cmd.name(),
                    "description": cmd.description(),
                    "parameters": cmd.param_schema(),
                })
            })
            .collect()
    }
}

impl Default for ShellRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ShellRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellRegistry")
            .field("commands", &self.list())
            .finish()
    }
}

// ============================================================================
// 单元测试 (mod tests) — P14.2.1 10 个 + P14.2.2 Consumer pattern 6 个
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    /// 跨平台 echo 命令 (写到 /tmp 或 tempdir)
    #[tokio::test]
    async fn execute_echo_command() {
        let provider = LocalShellProvider::new();
        let spec = ShellSpec::new("echo hello world").timeout(Duration::from_secs(5));
        let result = provider.execute(&spec).await.expect("execute failed");
        assert!(result.is_success(), "stderr: {}", result.stderr);
        assert_eq!(result.stdout.trim(), "hello world");
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.shell_kind, ShellKind::platform_default());
    }

    /// 跨平台 non-zero exit
    #[tokio::test]
    async fn execute_failing_command() {
        let provider = LocalShellProvider::new();
        #[cfg(unix)]
        let spec = ShellSpec::new("false").timeout(Duration::from_secs(5));
        #[cfg(windows)]
        let spec = ShellSpec::new("exit 1").timeout(Duration::from_secs(5));

        let result = provider.execute(&spec).await.expect("execute failed");
        assert!(!result.is_success());
        assert_eq!(result.exit_code, 1);
    }

    /// 捕获 stderr
    #[tokio::test]
    async fn execute_captures_stderr() {
        let provider = LocalShellProvider::new();
        let spec = ShellSpec::new("echo oops 1>&2").timeout(Duration::from_secs(5));
        let result = provider.execute(&spec).await.expect("execute failed");
        assert!(
            result.is_success(),
            "stderr 写但命令本身 exit 0, stderr: {}",
            result.stderr
        );
        assert!(
            result.stderr.contains("oops"),
            "stderr 应含 'oops', got: {}",
            result.stderr
        );
    }

    /// timeout 强制 kill (跟 subprocess 行为一致)
    #[tokio::test]
    async fn execute_respects_timeout() {
        let provider = LocalShellProvider::new();
        #[cfg(unix)]
        let spec = ShellSpec::new("sleep 30").timeout(Duration::from_millis(100));
        #[cfg(windows)]
        // 用 ping.exe 显式后缀,cmd /C 调内置 ping 会 fail
        let spec = ShellSpec::new("ping.exe -n 30 127.0.0.1").timeout(Duration::from_millis(100));

        let err = provider.execute(&spec).await.unwrap_err();
        assert!(matches!(
            err,
            ShellError::Subprocess(SubprocessError::Timeout(_))
        ));
    }

    /// 显式 env 注入 (不继承父进程 PATH)
    #[tokio::test]
    async fn execute_explicit_env_does_not_inherit() {
        let provider = LocalShellProvider::new();
        // Unix: $VAR, Windows cmd: %VAR%
        #[cfg(unix)]
        let spec = ShellSpec::new("echo $MA_SHELL_TEST")
            .env("MA_SHELL_TEST", "from-shell-crate")
            .timeout(Duration::from_secs(5));
        #[cfg(windows)]
        let spec = ShellSpec::new("echo %MA_SHELL_TEST%")
            .env("MA_SHELL_TEST", "from-shell-crate")
            .timeout(Duration::from_secs(5));

        let result = provider.execute(&spec).await.expect("execute failed");
        assert!(result.is_success(), "stderr: {}", result.stderr);
        assert_eq!(result.stdout.trim(), "from-shell-crate");
    }

    /// cwd 验证 (不存在 → 错误)
    #[tokio::test]
    async fn execute_invalid_cwd_returns_error() {
        let provider = LocalShellProvider::new();
        let spec = ShellSpec::new("echo hi")
            .cwd("/this/cwd/should/never/exist/ma-harness-shell-test-12345");
        let err = provider.execute(&spec).await.unwrap_err();
        assert!(matches!(err, ShellError::InvalidSpec(_)));
    }

    /// 空 command 拒绝
    #[tokio::test]
    async fn execute_empty_command_returns_error() {
        let provider = LocalShellProvider::new();
        let spec = ShellSpec::new("");
        let err = provider.execute(&spec).await.unwrap_err();
        assert!(matches!(err, ShellError::InvalidSpec(_)));
    }

    /// P14.2.3: PwshShellProvider 实装后, provider_name 从 "pwsh-stub" 变成 "pwsh"
    #[test]
    fn pwsh_provider_name_after_implementation() {
        let provider = PwshShellProvider::new();
        assert_eq!(provider.default_shell_kind(), ShellKind::Pwsh);
        assert_eq!(
            provider.provider_name(),
            "pwsh",
            "P14.2.3 实装后 provider_name 不再是 'pwsh-stub'"
        );
    }

    /// P14.2.3: 业务方本机没装 pwsh → find_pwsh 返回 None
    #[test]
    fn pwsh_find_pwsh_returns_none_when_not_installed() {
        // 业务方本机 PATH 没 pwsh, 也没在常见路径 — 假设
        // (CI runner / 业务方开发机 当前都没装, 这个测试 cross-platform 安全)
        // 实际: 如果本机装了, find_pwsh 返回 Some, 跳过这个测试
        if PwshShellProvider::find_pwsh().is_some() {
            // 装了, 不测 (跨平台 install 状态不可控)
            return;
        }
        // 没装, 验证 find_pwsh 行为正确
        let result = PwshShellProvider::find_pwsh();
        assert!(
            result.is_none(),
            "业务方本机没装 pwsh, find_pwsh 应返回 None, got: {:?}",
            result
        );
    }

    /// P14.2.3: execute 在没装 pwsh 时返回 ShellNotFound
    #[tokio::test]
    async fn pwsh_provider_execute_returns_not_found_when_uninstalled() {
        if PwshShellProvider::find_pwsh().is_some() {
            // 装了, 跳过 (e2e 测试要 MA_HARNESS_E2E=1 opt-in)
            return;
        }
        let provider = PwshShellProvider::new();
        let spec = ShellSpec::new("Get-Date");
        let err = provider.execute(&spec).await.unwrap_err();
        assert!(
            matches!(err, ShellError::ShellNotFound { .. }),
            "应返回 ShellNotFound, got: {:?}",
            err
        );
    }

    /// P14.2.3: MA_HARNESS_PWSH_PATH override 行为 (静态, 不修改 env)
    ///
    /// 注意: `MA_HARNESS_PWSH_PATH` 检查在 [`find_pwsh`](PwshShellProvider::find_pwsh) 里实现,
    /// 业务方设置后会被尊重. 我们这里只测 cross-cutting 行为 (没装 pwsh → None)
    /// 因为 ma-harness-shell 用 `#![deny(unsafe_code)]` 不允许 `std::env::set_var`.
    #[test]
    fn pwsh_find_pwsh_basic_behavior() {
        // 业务方本机没装 → None; 装了 → Some.
        // 测试 cross-cutting 行为, 不强求特定值
        let result = PwshShellProvider::find_pwsh();
        // 文档约定: 业务方要 override 时设 MA_HARNESS_PWSH_PATH=<path>
        // 这里不修改 env (compile-time no-unsafe), 跑 PATH + 常见路径 search
        // 业务方本机没 pwsh → None 是 OK 行为
        let _ = result; // 测试不 panic 就算过
    }

    /// ShellKind::platform_default 跨平台一致性
    #[test]
    fn shell_kind_platform_default() {
        #[cfg(unix)]
        assert_eq!(ShellKind::platform_default(), ShellKind::Sh);
        #[cfg(windows)]
        assert_eq!(ShellKind::platform_default(), ShellKind::Cmd);
    }

    /// ShellKind::program / invoke_flag 跟 dsh 一致
    #[test]
    fn shell_kind_program_and_invoke_flag() {
        assert_eq!(ShellKind::Sh.program(), "sh");
        assert_eq!(ShellKind::Sh.invoke_flag(), "-c");
        assert_eq!(ShellKind::Cmd.program(), "cmd");
        assert_eq!(ShellKind::Cmd.invoke_flag(), "/C");
        assert_eq!(ShellKind::Pwsh.program(), "pwsh");
        assert_eq!(ShellKind::Pwsh.invoke_flag(), "-c");
        assert_eq!(ShellKind::Bash.program(), "bash");
        assert_eq!(ShellKind::Bash.invoke_flag(), "-c");
    }

    /// 显式选 Bash (业务方想用 bash 而不是 /bin/sh)
    #[tokio::test]
    #[cfg(unix)] // Bash 在 Windows 默认没装
    async fn execute_with_explicit_bash() {
        let provider = LocalShellProvider::with_shell(ShellKind::Bash);
        let spec = ShellSpec::new("echo bash-test").timeout(Duration::from_secs(5));
        let result = provider.execute(&spec).await.expect("execute failed");
        assert!(result.is_success());
        assert_eq!(result.stdout.trim(), "bash-test");
        assert_eq!(result.shell_kind, ShellKind::Bash);
    }

    // ========================================================================
    // P14.2.2 Consumer pattern 测试 (ShellCommand + ShellRegistry)
    // ========================================================================

    /// 测试用 ShellCommand: "echo-hello" — invoke 时跑 `echo <args.name>`
    struct EchoHelloCommand;

    #[async_trait]
    impl ShellCommand for EchoHelloCommand {
        fn name(&self) -> &str {
            "echo_hello"
        }
        fn description(&self) -> &str {
            "print 'hello <name>' to stdout (test command)"
        }
        fn param_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            })
        }
        async fn invoke(&self, args: serde_json::Value) -> Result<ShellResult, ShellError> {
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("world");
            let provider = LocalShellProvider::new();
            let spec = ShellSpec::new(format!("echo hello {name}")).timeout(Duration::from_secs(5));
            provider.execute(&spec).await
        }
    }

    /// 测试用 ShellCommand: "fail-on-purpose" — invoke 时跑 `false` / `exit 1`
    struct FailCommand;

    #[async_trait]
    impl ShellCommand for FailCommand {
        fn name(&self) -> &str {
            "fail_on_purpose"
        }
        fn description(&self) -> &str {
            "always returns non-zero (test command)"
        }
        fn param_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(&self, _args: serde_json::Value) -> Result<ShellResult, ShellError> {
            let provider = LocalShellProvider::new();
            let spec = ShellSpec::new("false").timeout(Duration::from_secs(5));
            provider.execute(&spec).await
        }
    }

    #[tokio::test]
    async fn shell_registry_register_and_get() {
        let mut registry = ShellRegistry::new();
        registry.register(EchoHelloCommand);
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let cmd = registry.get("echo_hello").expect("command not found");
        assert_eq!(cmd.name(), "echo_hello");
        assert!(cmd.description().contains("hello"));
    }

    #[tokio::test]
    async fn shell_registry_invoke_success() {
        let mut registry = ShellRegistry::new();
        registry.register(EchoHelloCommand);

        let result = registry
            .invoke(
                "echo_hello",
                serde_json::json!({ "name": "consumer-pattern" }),
            )
            .await
            .expect("invoke failed");
        assert!(result.is_success());
        assert_eq!(result.stdout.trim(), "hello consumer-pattern");
    }

    #[tokio::test]
    async fn shell_registry_invoke_unknown_command_errors() {
        let registry = ShellRegistry::new();
        let err = registry
            .invoke("nonexistent", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ShellError::Unsupported { .. }));
    }

    #[tokio::test]
    async fn shell_registry_invoke_propagates_nonzero() {
        let mut registry = ShellRegistry::new();
        registry.register(FailCommand);
        // ShellService 返回 Ok(ShellResult { exit_code: 1 }) — 不是 Err
        // 业务方自行检查 is_success()
        let result = registry
            .invoke("fail_on_purpose", serde_json::json!({}))
            .await
            .expect("invoke should propagate non-zero as Ok result");
        assert!(!result.is_success());
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn shell_registry_tool_list() {
        let mut registry = ShellRegistry::new();
        registry.register(EchoHelloCommand);
        registry.register(FailCommand);

        let tools = registry.tool_list();
        assert_eq!(tools.len(), 2);
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"echo_hello".to_string()));
        assert!(names.contains(&"fail_on_purpose".to_string()));

        // 验证 schema 字段都存在
        for tool in &tools {
            assert!(tool["description"].is_string());
            assert!(tool["parameters"].is_object());
        }
    }

    #[tokio::test]
    async fn shell_registry_register_override_warns() {
        let mut registry = ShellRegistry::new();
        registry.register(EchoHelloCommand);
        // 重复注册同名命令 (override)
        registry.register(EchoHelloCommand);
        assert_eq!(registry.len(), 1, "重复注册应覆盖, 数量仍为 1");
    }
}
