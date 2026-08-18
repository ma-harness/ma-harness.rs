//! ma_harness_plugin_bash — first-party plugin: 执行 shell 命令
//!
//! **设计**: seam 公开 API 风格, impl cordis::Service/Plugin 跟 ctx 内部对接.
//!
//! **Week 5-6 实装**: BashService 跑 subprocess (tokio::process::Command) +
//! 捕获 stdout/stderr/exit_code + timeout via MAX_RUNTIME_MS.
//!
//! **Phase 1 简化**:
//! - 没有 landlock sandbox (见 docs/code-mode-deferred.md 风格, Phase 2 加)
//! - 没有白名单命令 (业务方通过 plugin.toml seam.sandbox.exec 配, Phase 2)
//! - Windows / Linux / macOS 跨平台测试 (用 shell 内置命令验证)

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::process::Stdio;
use std::time::Duration;

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

// ============================================================================
// 公开 typed key
// ============================================================================

/// shell 命令最大执行时间 (ms)
pub static MAX_RUNTIME_MS: ma_harness_cordis::CtxKey<u32> = ctx_key!("max_runtime_ms");

/// 默认最大执行时间: 30 秒
pub const DEFAULT_MAX_RUNTIME_MS: u32 = 30_000;

// ============================================================================
// 错误
// ============================================================================

/// Bash plugin 错误
#[derive(Debug, Error)]
pub enum BashError {
    /// 命令超时
    #[error("command timed out after {0}ms")]
    Timeout(u32),

    /// 命令启动失败 (command not found 等)
    #[error("failed to spawn command: {0}")]
    Spawn(#[from] std::io::Error),

    /// 命令退出码非 0
    #[error("command exited with code {code}: {stderr}")]
    NonZeroExit {
        /// 退出码
        code: i32,
        /// stderr 内容
        stderr: String,
    },
}

// ============================================================================
// CommandOutput — 命令执行结果
// ============================================================================

/// 命令输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutput {
    /// 退出码
    pub exit_code: i32,
    /// stdout 内容
    pub stdout: String,
    /// stderr 内容
    pub stderr: String,
    /// 实际执行时间 (ms)
    pub duration_ms: u64,
}

impl CommandOutput {
    /// 是否成功 (exit_code == 0)
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

// ============================================================================
// BashService
// ============================================================================

/// Bash service — 跑 shell 命令
///
/// **关键设计**: service 自身**不存** timeout, 每次 run 时从 ctx 读 MAX_RUNTIME_MS.
/// 业务方 set 这个 key 立刻生效 (跟 hello plugin 的 "活的 ctx" 设计一致).
pub struct BashService;

impl BashService {
    /// 跑 shell 命令 (跨平台: Linux/macOS 用 sh, Windows 用 cmd)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let ctx = Context::new();
    /// let svc = BashService;
    /// let output = svc.run_command(&ctx, "echo hello").await?;
    /// assert!(output.is_success());
    /// assert!(output.stdout.contains("hello"));
    /// ```
    pub async fn run_command(&self, ctx: &Context, cmd: &str) -> Result<CommandOutput, BashError> {
        let max_runtime_ms = ctx
            .get(MAX_RUNTIME_MS)
            .unwrap_or(DEFAULT_MAX_RUNTIME_MS);
        self.run_command_with_timeout(cmd, Duration::from_millis(max_runtime_ms as u64))
            .await
    }

    /// 跑 shell 命令, 显式指定 timeout
    pub async fn run_command_with_timeout(
        &self,
        cmd: &str,
        max_runtime: Duration,
    ) -> Result<CommandOutput, BashError> {
        let start = std::time::Instant::now();

        // 跨平台: 用 sh -c (Unix) 或 cmd /C (Windows)
        let mut command = build_shell_command(cmd);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        tracing::debug!(cmd = %cmd, timeout_ms = max_runtime.as_millis(), "running command");

        let child = command.spawn().map_err(BashError::Spawn)?;
        let output = match timeout(max_runtime, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(BashError::Spawn(e)),
            Err(_elapsed) => {
                // 超时: child 已被 drop (wait_with_output 是 self-consuming)
                // 实际子进程可能仍在跑 (在 Unix 是 zombie), 但 tokio 杀进程
                tracing::warn!(cmd = %cmd, timeout_ms = max_runtime.as_millis(), "command timed out");
                return Err(BashError::Timeout(max_runtime.as_millis() as u32));
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(CommandOutput {
            exit_code,
            stdout,
            stderr,
            duration_ms,
        })
    }
}

#[cfg(target_family = "unix")]
fn build_shell_command(cmd: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd);
    c
}

#[cfg(target_family = "windows")]
fn build_shell_command(cmd: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(cmd);
    c
}

impl CordisService for BashService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(BashService)
    }
    fn name(&self) -> &str {
        "bash"
    }
}

impl SeamService for BashService {
    type Error = anyhow::Error;
    fn install(_ctx: &Context) -> anyhow::Result<Self> {
        Ok(BashService)
    }
    fn name(&self) -> &str {
        "bash"
    }
}

// ============================================================================
// Plugin: BashPlugin
// ============================================================================

/// Bash plugin — install 时注入 BashService + 写默认 typed key
pub struct BashPlugin;

impl CordisPlugin for BashPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = BashService::install(ctx)?;
        ctx.inject(std::sync::Arc::new(svc));
        // 默认 timeout (业务方可以覆盖)
        ctx.set(MAX_RUNTIME_MS, DEFAULT_MAX_RUNTIME_MS);
        Ok(())
    }
    fn name(&self) -> &str {
        "bash"
    }
}

impl SeamPlugin for BashPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        <Self as CordisPlugin>::install(self, ctx)
    }
    fn name(&self) -> &str {
        "bash"
    }
}

// ============================================================================
// 单元测试 (跨平台, 用 shell 内置命令)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_default_timeout() -> Context {
        let ctx = Context::new();
        ctx.set(MAX_RUNTIME_MS, DEFAULT_MAX_RUNTIME_MS);
        ctx
    }

    /// 跨平台: 简单 echo 命令
    #[tokio::test]
    async fn run_echo_command() {
        let ctx = ctx_with_default_timeout();
        let svc = BashService;
        let out = svc
            .run_command(&ctx, echo_cmd("hello world"))
            .await
            .unwrap();
        assert!(out.is_success(), "echo 应成功, stderr: {}", out.stderr);
        assert!(
            out.stdout.contains("hello world"),
            "stdout 应含 'hello world', got: {}",
            out.stdout
        );
        assert_eq!(out.exit_code, 0);
    }

    /// 跨平台: false / non-zero exit
    #[tokio::test]
    async fn run_failing_command_returns_nonzero() {
        let ctx = ctx_with_default_timeout();
        let svc = BashService;
        // false 在 Unix 退出 1, Windows 没有 false 但 exit 1 通过 cmd /C "exit 1"
        let out = svc
            .run_command(&ctx, false_cmd())
            .await
            .unwrap(); // 不 panic, exit 0 也是成功命令
        assert_eq!(out.exit_code, 1, "false 应退出 1, got: {}", out.exit_code);
        assert!(!out.is_success());
    }

    /// 跨平台: 捕获 stderr
    #[tokio::test]
    async fn run_captures_stderr() {
        let ctx = ctx_with_default_timeout();
        let svc = BashService;
        let out = svc
            .run_command(&ctx, stderr_cmd("oops"))
            .await
            .unwrap();
        // 命令本身退出 0, 但 stderr 有内容
        assert!(out.is_success());
        assert!(
            out.stderr.contains("oops"),
            "stderr 应含 'oops', got: {}",
            out.stderr
        );
    }

    /// 跨平台: timeout (Phase 1 测试用 sleep)
    #[tokio::test]
    async fn run_respects_timeout() {
        let ctx = ctx_with_default_timeout();
        let svc = BashService;
        let result = svc
            .run_command_with_timeout(sleep_cmd("5"), Duration::from_millis(100))
            .await;
        assert!(matches!(result, Err(BashError::Timeout(100))));
    }

    /// 业务方覆盖默认 timeout
    #[tokio::test]
    async fn run_uses_ctx_overridden_timeout() {
        let ctx = Context::new();
        ctx.set(MAX_RUNTIME_MS, 100u32); // 100ms
        let svc = BashService;
        let result = svc.run_command(&ctx, sleep_cmd("5")).await;
        assert!(matches!(result, Err(BashError::Timeout(_))));
    }

    // ========================================================================
    // 跨平台 helper
    // ========================================================================

    #[cfg(target_family = "unix")]
    fn echo_cmd(s: &str) -> String {
        format!("echo {}", s)
    }

    #[cfg(target_family = "windows")]
    fn echo_cmd(s: &str) -> String {
        format!("echo {}", s)
    }

    #[cfg(target_family = "unix")]
    fn false_cmd() -> String {
        "false".to_string()
    }

    #[cfg(target_family = "windows")]
    fn false_cmd() -> String {
        "exit 1".to_string()
    }

    #[cfg(target_family = "unix")]
    fn stderr_cmd(s: &str) -> String {
        format!("echo {} 1>&2", s)
    }

    #[cfg(target_family = "windows")]
    fn stderr_cmd(s: &str) -> String {
        format!("echo {} 1>&2", s)
    }

    #[cfg(target_family = "unix")]
    fn sleep_cmd(secs: &str) -> String {
        format!("sleep {}", secs)
    }

    #[cfg(target_family = "windows")]
    fn sleep_cmd(secs: &str) -> String {
        // Windows 没有 sleep 内置, 用 ping -n 凑合
        format!("ping -n {} 127.0.0.1 > nul", secs.to_string().parse::<u32>().unwrap_or(5) + 1)
    }
}
