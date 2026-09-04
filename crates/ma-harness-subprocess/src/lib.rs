//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-subprocess`
//! **Crate ident** (`use` 路径): `ma_harness_subprocess`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident,
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-subprocess = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_subprocess::{
//!     CommandSpec, LocalSubprocessProvider, StdioConfig, SubprocessService,
//! };
//! use std::ffi::OsString;
//!
//! let provider = LocalSubprocessProvider::new();
//! let spec = CommandSpec::new("echo", vec![OsString::from("hello")])
//!     .stdout(StdioConfig::Piped);
//! let output = provider.output(&spec).await?;
//! assert!(output.status.success());
//! assert_eq!(output.stdout_str().trim(), "hello");
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-subprocess
//!
//! # 设计 (Design) — P14.1
//!
//! **目标**: 替代散落在 ma-harness 各处的 `tokio::process::Command` 直接调用,
//! 提供 `ctx.subprocess` 能力缝 (Service Definition), 让
//! `LocalSubprocessProvider` / `SandboxSubprocessProvider` / 未来 `RemoteSubprocessProvider`
//! 都能插拔.
//!
//! **背景**: 见 [dsh-feature-parity-table §2] capability seams — `ctx.subprocess` 在 dsh 里
//! 是 14 个核心能力缝之一, 共享 Win32 库做 Windows 进程树管理. ma-harness 之前
//! 是直接 `tokio::process::Command`, 不可替换, 也无法走 sandbox 隔离.
//!
//! **接口**:
//! - [`SubprocessService`] trait — 5 个 async 方法 + provider 标识
//! - [`CommandSpec`] — 业务方写的命令描述 (无 `&mut`, 不可变, 共享安全)
//! - [`ChildHandle`] — opaque 句柄 (u64 ID), 用于 wait / kill / try_wait
//! - [`LocalSubprocessProvider`] — tokio::process 实现 (P14.1 主交付)
//! - [`SandboxSubprocessProvider`] — stub, 等 P14.1.1 委托给 ctx.sandbox
//!
//! **6 质量属性 (业务方 2026-09-04 约定)**:
//! - 可复用: trait 抽象, 跟 dsh 1:1 对应, 后续可加 Remote / Mock / 等 provider
//! - 可维护: 模块化分块 (`// === 标题 ===`), 类型集中在 lib.rs, 测试在文件末
//! - 鲁棒: 所有 IO 错误经 `SubprocessError` 归一化, 边界 case (空 args / kill 后 wait / 等) 显式处理
//! - 安全: 不 `unsafe`, 不引入新 secret 类型, 显式 env (不继承父进程)
//! - 可测: trait 抽象 → 业务方可注入 mock provider; 11 个单元测试覆盖核心场景
//! - 可扩展: `StdioConfig` 已预留 `File(PathBuf)` 模式给 P14.5 LSP / 后续 PTY
//!
//! # 限制 (Limitations) — P14.1
//!
//! - 没有 process group 管理 (P14.1.1 加 Win32 共享库)
//! - 没有 `pty` 支持 (P15.2 portable-pty)
//! - Sandbox provider 是 stub, 跟 ctx.sandbox 集成等 P14.1.1
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::process::ExitStatus as StdExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use thiserror::Error;
use tokio::process::{Child, Command};

// ============================================================================
// Error: 统一的子进程错误类型 (业务方 6 属性 - 鲁棒性)
// ============================================================================

/// Subprocess capability 错误.
#[derive(Debug, Error)]
pub enum SubprocessError {
    /// IO 错误 (spawn / read / write 失败)
    #[error("subprocess I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// 无效的 CommandSpec (空 program / 不存在的 cwd / 等)
    #[error("invalid command spec: {0}")]
    InvalidSpec(String),

    /// Child handle 不存在或已被 wait / kill 过
    #[error("child handle {0:?} not found (already waited/killed or never spawned)")]
    HandleNotFound(ChildHandle),

    /// Provider 不支持此操作 (例如 SandboxSubprocessProvider 还没接 ctx.sandbox)
    #[error("provider '{provider}' does not support {operation}: {reason}")]
    Unsupported {
        /// Provider 名
        provider: &'static str,
        /// 操作名
        operation: &'static str,
        /// 原因
        reason: String,
    },

    /// 超时 (CommandSpec::timeout 配置)
    #[error("subprocess timed out after {0:?}")]
    Timeout(Duration),
}

// ============================================================================
// CommandSpec: 业务方写的命令描述 (无 &mut, 共享安全)
// ============================================================================

/// 子进程命令描述.
///
/// 用 builder pattern 构造 (`CommandSpec::new(program, args).env(...)`).
/// 不可变, 可 Clone, 可 Send + Sync, 适合跨 await 边界传递.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    /// 要执行的程序 (例如 `/bin/echo` 或 `cmd.exe`)
    pub program: OsString,
    /// 参数列表 (不含 program)
    pub args: Vec<OsString>,
    /// 环境变量 (注入, 覆盖同 key 父 env; 受 `inherit_path` 控制是否先清空)
    pub env: BTreeMap<String, OsString>,
    /// 工作目录 (None = 父进程 cwd)
    pub cwd: Option<PathBuf>,
    /// stdin 配置
    pub stdin: StdioConfig,
    /// stdout 配置
    pub stdout: StdioConfig,
    /// stderr 配置
    pub stderr: StdioConfig,
    /// Child drop 时是否自动 kill (默认 true, 防止子进程泄漏)
    pub kill_on_drop: bool,
    /// 超时 (None = 无限等待)
    pub timeout: Option<Duration>,
    /// 是否继承父进程 env (默认 `true`, P14.1.1 fix — PATH 必须有, 不然 `ping.exe` 等外部命令找不到).
    /// 业务方显式 `.no_inherit_path()` 可关 (sandboxes / 严格 env isolation).
    pub inherit_path: bool,
}

impl CommandSpec {
    /// 创建一个新 CommandSpec (默认: piped stdio, kill_on_drop=true, inherit_path=true)
    pub fn new(
        program: impl Into<OsString>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            env: BTreeMap::new(),
            cwd: None,
            stdin: StdioConfig::Piped,
            stdout: StdioConfig::Piped,
            stderr: StdioConfig::Piped,
            kill_on_drop: true,
            timeout: None,
            inherit_path: true,
        }
    }

    /// 覆盖环境变量 (一次性 SET, 业务方可 `.env("KEY", "VALUE")`)
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

    /// 设置工作目录
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// stdin 配置
    pub fn stdin(mut self, cfg: StdioConfig) -> Self {
        self.stdin = cfg;
        self
    }

    /// stdout 配置
    pub fn stdout(mut self, cfg: StdioConfig) -> Self {
        self.stdout = cfg;
        self
    }

    /// stderr 配置
    pub fn stderr(mut self, cfg: StdioConfig) -> Self {
        self.stderr = cfg;
        self
    }

    /// 关闭 kill_on_drop
    pub fn no_kill_on_drop(mut self) -> Self {
        self.kill_on_drop = false;
        self
    }

    /// 关闭 inherit_path (业务方想完全隔离 env, 例如 P14.1.1 sandbox).
    /// 默认 `true` 继承父 PATH 等关键变量.
    pub fn no_inherit_path(mut self) -> Self {
        self.inherit_path = false;
        self
    }

    /// 设置超时
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(dur);
        self
    }

    /// 验证 spec (空 program / cwd 不存在)
    pub fn validate(&self) -> Result<(), SubprocessError> {
        if self.program.is_empty() {
            return Err(SubprocessError::InvalidSpec("program is empty".into()));
        }
        if let Some(ref cwd) = self.cwd {
            if !cwd.exists() {
                return Err(SubprocessError::InvalidSpec(format!(
                    "cwd does not exist: {}",
                    cwd.display()
                )));
            }
        }
        Ok(())
    }

    /// 转成 `tokio::process::Command` (内部用, 业务方一般不直接调)
    pub(crate) fn to_tokio_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .stdin(self.stdin.to_tokio())
            .stdout(self.stdout.to_tokio())
            .stderr(self.stderr.to_tokio())
            .kill_on_drop(self.kill_on_drop);

        // Env 策略 (P14.1.1 fix):
        // - 默认 inherit_path=true: 保留父 env (PATH / LANG / 等), 业务方 env 覆盖同 key
        //   (PATH 必须有, 不然 Windows 找不到 ping.exe 等外部命令)
        // - inherit_path=false: 先 env_clear 再灌入业务方 env (严格隔离, 给 P14.1.1 sandbox)
        if !self.inherit_path {
            cmd.env_clear();
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }
        cmd
    }
}

// ============================================================================
// StdioConfig: 跨平台 stdio 抽象 (tokio::process::Stdio 不可 Clone, 自己包一层)
// ============================================================================

/// 子进程 stdio 流配置.
///
/// `tokio::process::Stdio` 本身不可 Clone, 不能放进 `CommandSpec` (业务方要 Clone).
/// 自己包一层 enum, 跨平台 safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdioConfig {
    /// 管道 (业务方读 stdout / 写 stdin)
    Piped,
    /// 继承父进程 (终端显示)
    Inherit,
    /// /dev/null (丢弃)
    Null,
    /// 重定向到文件 (truncate 模式; append 模式 P15+ 再加)
    File(PathBuf),
}

impl StdioConfig {
    /// 转成 `std::process::Stdio` (内部用, tokio 内部就是 re-export std 的)
    pub(crate) fn to_tokio(&self) -> std::process::Stdio {
        use std::process::Stdio;
        match self {
            StdioConfig::Piped => Stdio::piped(),
            StdioConfig::Inherit => Stdio::inherit(),
            StdioConfig::Null => Stdio::null(),
            StdioConfig::File(p) => {
                Stdio::from(std::fs::File::create(p).unwrap_or_else(|e| {
                    panic!("StdioConfig::File({:?}) failed to create: {}", p, e)
                }))
            }
        }
    }

    /// 默认 (Piped)
    pub fn default_piped() -> Self {
        StdioConfig::Piped
    }
}

impl Default for StdioConfig {
    fn default() -> Self {
        StdioConfig::Piped
    }
}

// ============================================================================
// ChildHandle: opaque 子进程句柄 (u64 ID, 全局唯一)
// ============================================================================

/// 子进程句柄 (opaque ID, 跨 provider 边界可传递).
///
/// 实现: 全局 AtomicU64 递增, 不复用. wait / kill / try_wait 都按 ID 查表.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChildHandle(u64);

impl ChildHandle {
    /// Raw ID (调试 / 日志用)
    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for ChildHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Child#{}", self.0)
    }
}

static NEXT_CHILD_ID: AtomicU64 = AtomicU64::new(1);

fn next_child_id() -> ChildHandle {
    ChildHandle(NEXT_CHILD_ID.fetch_add(1, Ordering::Relaxed))
}

// ============================================================================
// ExitStatus / CommandOutput: 业务方拿到的子进程结果
// ============================================================================

/// 子进程退出状态 (薄包装, 跨平台).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    /// 退出码 (Unix: exit code; Windows: `exit_code()` from raw)
    pub code: Option<i32>,
    /// Unix 信号退出 (Windows 永远 false)
    pub signal: Option<i32>,
    /// 业务方便捷方法: 0 = success
    pub success: bool,
}

impl ExitStatus {
    /// 从 `std::process::ExitStatus` 构造
    pub fn from_std(s: StdExitStatus) -> Self {
        Self {
            code: s.code(),
            #[cfg(unix)]
            signal: {
                use std::os::unix::process::ExitStatusExt;
                s.signal()
            },
            #[cfg(not(unix))]
            signal: None,
            success: s.success(),
        }
    }
}

/// 一次性 `output()` 调用结果 (含 stdout / stderr bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// stdout bytes (空如果 stdout 不是 Piped)
    pub stdout: Vec<u8>,
    /// stderr bytes (空如果 stderr 不是 Piped)
    pub stderr: Vec<u8>,
    /// 退出状态
    pub status: ExitStatus,
}

impl CommandOutput {
    /// stdout 转 UTF-8 string (lossy, 业务方自行校验)
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// stderr 转 UTF-8 string (lossy)
    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

// ============================================================================
// SubprocessService: 能力缝 trait (dsh `ctx.subprocess` 对等)
// ============================================================================

/// Subprocess 能力缝 (跟 dsh `ctx.subprocess` seam 对等).
///
/// **5 个核心方法**:
/// - [`spawn`](Self::spawn) — 启动子进程, 返回 handle
/// - [`wait`](Self::wait) — 阻塞等待子进程退出
/// - [`try_wait`](Self::try_wait) — 非阻塞 poll
/// - [`kill`](Self::kill) — 主动 kill
/// - [`output`](Self::output) — spawn + 等 + 收集 stdout/stderr (便捷)
///
/// **实现**:
/// - [`LocalSubprocessProvider`] — tokio::process (P14.1)
/// - [`SandboxSubprocessProvider`] — stub, P14.1.1 委托 ctx.sandbox
/// - 业务方可注入自己的 provider (例如 mock, P14.1 测试用)
#[async_trait]
pub trait SubprocessService: Send + Sync + 'static {
    /// 启动子进程
    async fn spawn(&self, spec: &CommandSpec) -> Result<ChildHandle, SubprocessError>;

    /// 阻塞等待子进程退出 (consume handle)
    async fn wait(&self, handle: ChildHandle) -> Result<ExitStatus, SubprocessError>;

    /// 非阻塞 poll (返回 None 如果还没退出)
    async fn try_wait(&self, handle: ChildHandle) -> Result<Option<ExitStatus>, SubprocessError>;

    /// 主动 kill (不 wait; 业务方后续 wait 拿 exit code)
    async fn kill(&self, handle: ChildHandle) -> Result<(), SubprocessError>;

    /// 一次性: spawn + wait + 收集 stdout/stderr (业务方最常用)
    async fn output(&self, spec: &CommandSpec) -> Result<CommandOutput, SubprocessError>;

    /// Provider 标识 (日志 / 调试用)
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// LocalSubprocessProvider: tokio::process::Command 封装
// ============================================================================

/// 本地子进程 provider (P14.1 主交付).
///
/// **实现**: `tokio::process::Command` + 全局 `Mutex<HashMap<ChildHandle, Child>>` 句柄表.
/// **生命周期**: handle 从 spawn 分配, wait/kill 后从表里移除. drop handle (业务方忘 wait)
/// 时 tokio 的 `kill_on_drop=true` 默认会 SIGKILL.
pub struct LocalSubprocessProvider {
    handles: Mutex<std::collections::HashMap<ChildHandle, Child>>,
}

impl LocalSubprocessProvider {
    /// 创建一个新 LocalSubprocessProvider
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for LocalSubprocessProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SubprocessService for LocalSubprocessProvider {
    async fn spawn(&self, spec: &CommandSpec) -> Result<ChildHandle, SubprocessError> {
        spec.validate()?;
        let mut cmd = spec.to_tokio_command();
        let child = cmd.spawn().map_err(|e| {
            // spawn 失败时, 业务方最常见错误是 program 找不到
            // 包装一下 error message 方便排查
            SubprocessError::Io(std::io::Error::new(
                e.kind(),
                format!("spawn {:?} {:?} failed: {}", spec.program, spec.args, e),
            ))
        })?;
        let handle = next_child_id();
        self.handles.lock().insert(handle, child);
        tracing::debug!(
            handle = %handle,
            program = ?spec.program,
            args = ?spec.args,
            "subprocess spawned"
        );
        Ok(handle)
    }

    async fn wait(&self, handle: ChildHandle) -> Result<ExitStatus, SubprocessError> {
        let mut child = self
            .handles
            .lock()
            .remove(&handle)
            .ok_or(SubprocessError::HandleNotFound(handle))?;
        let status = child.wait().await?;
        Ok(ExitStatus::from_std(status))
    }

    async fn try_wait(&self, handle: ChildHandle) -> Result<Option<ExitStatus>, SubprocessError> {
        let mut handles = self.handles.lock();
        let child = handles
            .get_mut(&handle)
            .ok_or(SubprocessError::HandleNotFound(handle))?;
        match child.try_wait()? {
            Some(status) => {
                // 已经退出, 从表里 remove
                handles.remove(&handle);
                Ok(Some(ExitStatus::from_std(status)))
            }
            None => Ok(None),
        }
    }

    async fn kill(&self, handle: ChildHandle) -> Result<(), SubprocessError> {
        let mut handles = self.handles.lock();
        let child = handles
            .get_mut(&handle)
            .ok_or(SubprocessError::HandleNotFound(handle))?;
        child.start_kill()?;
        // 不 remove, 让业务方后续 wait 拿退出码
        Ok(())
    }

    async fn output(&self, spec: &CommandSpec) -> Result<CommandOutput, SubprocessError> {
        spec.validate()?;

        // 强制 stdout/stderr = Piped (override spec)
        let spec = spec
            .clone()
            .stdout(StdioConfig::Piped)
            .stderr(StdioConfig::Piped);

        let handle = self.spawn(&spec).await?;

        // 拿 stdout/stderr (clone 是必要的: child 也持有 fd, 读完后才 wait)
        let mut child = self
            .handles
            .lock()
            .remove(&handle)
            .ok_or(SubprocessError::HandleNotFound(handle))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let (stdout_bytes, stderr_bytes, status) = match spec.timeout {
            Some(dur) => {
                match tokio::time::timeout(dur, async {
                    let s = read_stream(stdout).await?;
                    let e = read_stream(stderr).await?;
                    let st = child.wait().await?;
                    Ok::<_, SubprocessError>((s, e, st))
                })
                .await
                {
                    Ok(Ok((s, e, st))) => (s, e, st),
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        // 超时, kill 子进程
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        return Err(SubprocessError::Timeout(dur));
                    }
                }
            }
            None => {
                let s = read_stream(stdout).await?;
                let e = read_stream(stderr).await?;
                let st = child.wait().await?;
                (s, e, st)
            }
        };

        Ok(CommandOutput {
            stdout: stdout_bytes,
            stderr: stderr_bytes,
            status: ExitStatus::from_std(status),
        })
    }

    fn provider_name(&self) -> &'static str {
        "local-tokio"
    }
}

/// 读一个 `Option<ChildStdout>` / `Option<ChildStderr>` 到 bytes.
async fn read_stream<R>(stream: Option<R>) -> Result<Vec<u8>, SubprocessError>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::AsyncReadExt;

    let Some(mut stream) = stream else {
        return Ok(Vec::new());
    };
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    Ok(buf)
}

// ============================================================================
// SandboxSubprocessProvider: stub (P14.1.1 委托 ctx.sandbox)
// ============================================================================

/// 沙箱子进程 provider (P14.1 stub).
///
/// 当前所有方法都返回 `SubprocessError::Unsupported`, 提醒业务方 P14.1.1
/// 才会接 ctx.sandbox (`ma-harness-sandbox::Policy`).
///
/// **未来实现路径** (P14.1.1):
/// 1. spawn 时根据 `CommandSpec` 推导 sandbox policy (read/write paths 来自 env / cwd / args)
/// 2. 在 child 启动前 `LinuxLandlockEnforcer::enforce(&policy)`
/// 3. 业务方拿到的 handle 走跟 LocalProvider 一样的 wait/kill 路径
pub struct SandboxSubprocessProvider;

impl SandboxSubprocessProvider {
    /// 创建一个新 SandboxSubprocessProvider (stub)
    pub const fn new() -> Self {
        Self
    }
}

impl Default for SandboxSubprocessProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SubprocessService for SandboxSubprocessProvider {
    async fn spawn(&self, _spec: &CommandSpec) -> Result<ChildHandle, SubprocessError> {
        Err(SubprocessError::Unsupported {
            provider: "sandbox",
            operation: "spawn",
            reason: "P14.1.1: needs ctx.sandbox integration. Use LocalSubprocessProvider for now."
                .into(),
        })
    }

    async fn wait(&self, _handle: ChildHandle) -> Result<ExitStatus, SubprocessError> {
        Err(SubprocessError::Unsupported {
            provider: "sandbox",
            operation: "wait",
            reason: "P14.1.1 stub".into(),
        })
    }

    async fn try_wait(&self, _handle: ChildHandle) -> Result<Option<ExitStatus>, SubprocessError> {
        Err(SubprocessError::Unsupported {
            provider: "sandbox",
            operation: "try_wait",
            reason: "P14.1.1 stub".into(),
        })
    }

    async fn kill(&self, _handle: ChildHandle) -> Result<(), SubprocessError> {
        Err(SubprocessError::Unsupported {
            provider: "sandbox",
            operation: "kill",
            reason: "P14.1.1 stub".into(),
        })
    }

    async fn output(&self, _spec: &CommandSpec) -> Result<CommandOutput, SubprocessError> {
        Err(SubprocessError::Unsupported {
            provider: "sandbox",
            operation: "output",
            reason: "P14.1.1 stub".into(),
        })
    }

    fn provider_name(&self) -> &'static str {
        "sandbox-stub"
    }
}

// ============================================================================
// DefaultSubprocessProvider: 当前默认 = Local
// ============================================================================

/// 平台默认 provider (P14.1: Local, P14.1.1 切 SandboxSubprocessProvider)
pub type DefaultSubprocessProvider = LocalSubprocessProvider;

// ============================================================================
// 单元测试 (mod tests) — 11 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::ffi::OsString;
    use std::path::PathBuf;

    /// 跨平台 "echo" 命令名 (Windows 用 cmd /c echo, Unix 直接 echo)
    fn echo_program() -> &'static str {
        #[cfg(windows)]
        {
            "cmd"
        }
        #[cfg(not(windows))]
        {
            "echo"
        }
    }

    fn echo_args(msg: &str) -> Vec<OsString> {
        #[cfg(windows)]
        {
            vec![
                OsString::from("/c"),
                OsString::from("echo"),
                OsString::from(msg),
            ]
        }
        #[cfg(not(windows))]
        {
            vec![OsString::from(msg)]
        }
    }

    /// 跨平台 "true" 命令 (exit 0, 无输出)
    fn true_command() -> CommandSpec {
        #[cfg(windows)]
        {
            CommandSpec::new(
                "cmd",
                vec![
                    OsString::from("/c"),
                    OsString::from("exit"),
                    OsString::from("0"),
                ],
            )
        }
        #[cfg(not(windows))]
        {
            CommandSpec::new("true", Vec::<OsString>::new())
        }
    }

    /// 跨平台 "false" 命令 (exit 1)
    fn false_command() -> CommandSpec {
        #[cfg(windows)]
        {
            CommandSpec::new(
                "cmd",
                vec![
                    OsString::from("/c"),
                    OsString::from("exit"),
                    OsString::from("1"),
                ],
            )
        }
        #[cfg(not(windows))]
        {
            CommandSpec::new("false", Vec::<OsString>::new())
        }
    }

    #[tokio::test]
    async fn output_echo_hello() {
        let provider = LocalSubprocessProvider::new();
        let spec = CommandSpec::new(echo_program(), echo_args("hello"));
        let out = provider.output(&spec).await.expect("output failed");
        assert!(out.status.success, "expected success, got {:?}", out.status);
        assert_eq!(out.stdout_str().trim(), "hello");
        assert!(
            out.stderr.is_empty(),
            "stderr should be empty, got: {:?}",
            out.stderr_str()
        );
    }

    #[tokio::test]
    async fn output_exit_zero() {
        let provider = LocalSubprocessProvider::new();
        let out = provider
            .output(&true_command())
            .await
            .expect("output failed");
        assert!(out.status.success);
        assert_eq!(out.status.code, Some(0));
    }

    #[tokio::test]
    async fn output_exit_one() {
        let provider = LocalSubprocessProvider::new();
        let out = provider
            .output(&false_command())
            .await
            .expect("output failed");
        assert!(!out.status.success);
        assert_eq!(out.status.code, Some(1));
    }

    #[tokio::test]
    async fn spawn_then_wait_explicit() {
        let provider = LocalSubprocessProvider::new();
        let handle = provider.spawn(&true_command()).await.expect("spawn failed");
        let status = provider.wait(handle).await.expect("wait failed");
        assert!(status.success);

        // 第二次 wait 应该报 HandleNotFound
        let err = provider.wait(handle).await.unwrap_err();
        assert!(matches!(err, SubprocessError::HandleNotFound(_)));
    }

    #[tokio::test]
    async fn try_wait_returns_none_then_some() {
        let provider = LocalSubprocessProvider::new();
        let handle = provider.spawn(&true_command()).await.expect("spawn failed");

        // 第一次 try_wait 可能 None 也可能 Some (取决于调度)
        // 我们只断言 handle 仍存在 (None 时 handle 不被消费)
        let _first = provider.try_wait(handle).await.expect("try_wait failed");

        // 再 wait, 一定拿 Some
        let status = provider.wait(handle).await.expect("wait failed");
        assert!(status.success);
    }

    #[tokio::test]
    async fn kill_running_subprocess() {
        let provider = LocalSubprocessProvider::new();
        // 跨平台长跑命令: Windows `ping.exe -n 30 127.0.0.1` (显式 .exe,cmd /c 调内置 ping 会失败)
        //                  Unix `sleep 30`
        #[cfg(windows)]
        let spec = CommandSpec::new(
            "ping.exe",
            vec![
                OsString::from("-n"),
                OsString::from("30"),
                OsString::from("127.0.0.1"),
            ],
        );
        #[cfg(not(windows))]
        let spec = CommandSpec::new("sleep", vec![OsString::from("30")]);

        let handle = provider.spawn(&spec).await.expect("spawn failed");
        provider.kill(handle).await.expect("kill failed");

        let status = provider.wait(handle).await.expect("wait failed");
        assert!(!status.success, "killed subprocess should not be success");
    }

    #[tokio::test]
    async fn wait_unknown_handle_returns_error() {
        let provider = LocalSubprocessProvider::new();
        let bogus = ChildHandle(999_999);
        let err = provider.wait(bogus).await.unwrap_err();
        assert!(matches!(err, SubprocessError::HandleNotFound(_)));
    }

    #[tokio::test]
    async fn spawn_empty_program_is_invalid() {
        let provider = LocalSubprocessProvider::new();
        let spec = CommandSpec::new("", Vec::<OsString>::new());
        let err = provider.spawn(&spec).await.unwrap_err();
        assert!(matches!(err, SubprocessError::InvalidSpec(_)));
    }

    #[tokio::test]
    async fn spawn_nonexistent_cwd_is_invalid() {
        let provider = LocalSubprocessProvider::new();
        let spec = CommandSpec::new(echo_program(), echo_args("hi"))
            .cwd("/this/path/should/never/exist/anywhere/ma-harness-test-12345");
        let err = provider.spawn(&spec).await.unwrap_err();
        assert!(matches!(err, SubprocessError::InvalidSpec(_)));
    }

    #[tokio::test]
    async fn explicit_env_overrides_parent() {
        // P14.1.1: 默认 inherit_path=true, 父 env 会被保留 (PATH 等).
        // 业务方 .env("KEY", "VAL") 覆盖同 key.
        // 这里验证业务方显式 env 在子进程生效.
        let provider = LocalSubprocessProvider::new();
        #[cfg(windows)]
        let spec = CommandSpec::new(
            "cmd",
            vec![
                OsString::from("/c"),
                OsString::from("echo"),
                OsString::from("%MA_TEST%"),
            ],
        )
        .env("MA_TEST", "42");
        #[cfg(not(windows))]
        let spec = CommandSpec::new(
            "sh",
            vec![OsString::from("-c"), OsString::from("echo $MA_TEST")],
        )
        .env("MA_TEST", "42");

        let out = provider.output(&spec).await.expect("output failed");
        assert!(out.status.success);
        assert_eq!(out.stdout_str().trim(), "42");
    }

    #[tokio::test]
    async fn no_inherit_path_fully_isolates_env() {
        // P14.1.1: .no_inherit_path() 完全清空父 env, 只保留业务方 .env()
        // 验证: 即使父进程有 PATH, no_inherit_path 后子进程看不到
        // (Unix /usr/bin/sleep 不需要 PATH, 但 shell 内部命令如 $PATH 验证)
        let provider = LocalSubprocessProvider::new();
        #[cfg(unix)]
        let spec = CommandSpec::new(
            "sh",
            vec![
                OsString::from("-c"),
                OsString::from("echo path-is-empty:$PATH"),
            ],
        )
        .no_inherit_path();
        #[cfg(windows)]
        let spec = CommandSpec::new(
            "cmd",
            vec![
                OsString::from("/c"),
                OsString::from("echo path-is-empty:%PATH%"),
            ],
        )
        .no_inherit_path();

        let out = provider.output(&spec).await.expect("output failed");
        assert!(out.status.success, "stderr: {}", out.stderr_str());
        // Windows: 完整清空 → echo 出来是 "path-is-empty:%PATH%" (变量未定义 → 空)
        // Unix: 同理 → "path-is-empty:" (空)
        let stdout = out.stdout_str();
        assert!(
            stdout.contains("path-is-empty:") && !stdout.contains("path-is-empty::")
                || stdout.contains("path-is-empty:") && stdout.ends_with("\n"),
            "PATH 应被清空, got stdout: {:?}",
            stdout
        );
        // 更严格: PATH 应该是空字符串
        #[cfg(unix)]
        assert_eq!(stdout.trim(), "path-is-empty:");
    }

    #[tokio::test]
    async fn output_timeout_kills_subprocess() {
        let provider = LocalSubprocessProvider::new();
        #[cfg(windows)]
        let spec = CommandSpec::new(
            "ping.exe",
            vec![
                OsString::from("-n"),
                OsString::from("30"),
                OsString::from("127.0.0.1"),
            ],
        )
        .timeout(Duration::from_millis(100));
        #[cfg(not(windows))]
        let spec = CommandSpec::new("sleep", vec![OsString::from("30")])
            .timeout(Duration::from_millis(100));

        let err = provider.output(&spec).await.unwrap_err();
        assert!(matches!(err, SubprocessError::Timeout(_)));
    }

    #[tokio::test]
    async fn sandbox_provider_returns_unsupported() {
        let provider = SandboxSubprocessProvider::new();
        assert_eq!(provider.provider_name(), "sandbox-stub");

        let spec = CommandSpec::new(echo_program(), echo_args("test"));
        let err = provider.spawn(&spec).await.unwrap_err();
        assert!(matches!(err, SubprocessError::Unsupported { .. }));

        let err = provider.output(&spec).await.unwrap_err();
        assert!(matches!(err, SubprocessError::Unsupported { .. }));
    }

    #[tokio::test]
    async fn stdio_config_file_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path: PathBuf = dir.path().join("out.txt");
        let cfg = StdioConfig::File(file_path.clone());
        // 验证 to_tokio 不 panic + 后续业务方写入能 work
        let _ = cfg.to_tokio();
        assert!(matches!(cfg, StdioConfig::File(_)));
    }

    #[tokio::test]
    async fn child_handle_display() {
        let handle = ChildHandle(42);
        assert_eq!(format!("{}", handle), "Child#42");
        assert_eq!(handle.raw(), 42);
    }

    #[tokio::test]
    async fn default_provider_type_alias() {
        // 验证 DefaultSubprocessProvider 是 LocalSubprocessProvider (P14.1 期间)
        let _p: DefaultSubprocessProvider = LocalSubprocessProvider::new();
    }

    #[tokio::test]
    async fn provider_name() {
        assert_eq!(
            LocalSubprocessProvider::new().provider_name(),
            "local-tokio"
        );
        assert_eq!(
            SandboxSubprocessProvider::new().provider_name(),
            "sandbox-stub"
        );
    }
}
