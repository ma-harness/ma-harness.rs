//! ma_harness_plugin_bash ?first-party plugin: 执行 shell 命令
//!
//! **设计**: seam 公开 API 风格, impl cordis::Service/Plugin ?ctx 内部对接.
//!
//! **Week 5-6 实装**: BashService ?subprocess (tokio::process::Command) +
//! 捕获 stdout/stderr/exit_code + timeout via MAX_RUNTIME_MS.
//!
//! **P14.2.2 重构**: BashService 内部从 `tokio::process::Command` 改为
//! 走 `ma_harness_shell::ShellService` (跟 dsh `ctx.shell` seam 1:1 对等).
//! 公开 API 保持 (`run_command` / `run_command_with_timeout` / `CommandOutput` 不变).
//!
//! **Phase 1 简?*:
//! - 没有 landlock sandbox (?docs/code-mode-deferred.md 风格, Phase 2 ?
//! - 没有白名单命?(业务方通过 plugin.toml seam.sandbox.exec ? Phase 2)
//! - Windows / Linux / macOS 跨平台测?(?shell 内置命令验证)

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;
use std::time::Duration;

use ma_harness_cordis::Context;
use ma_harness_cordis::Plugin as CordisPlugin;
use ma_harness_cordis::Service as CordisService;
use ma_harness_seam::{ctx_key, Plugin as SeamPlugin, Service as SeamService};
use ma_harness_shell::{LocalShellProvider, ShellError, ShellService, ShellSpec};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// 公开 typed key
// ============================================================================

/// shell 命令最大执行时?(ms)
pub static MAX_RUNTIME_MS: ma_harness_cordis::CtxKey<u32> = ctx_key!("max_runtime_ms");

/// 默认最大执行时间 (ms)
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

    /// 命令启动失败 (command not found ?
    #[error("failed to spawn command: {0}")]
    Spawn(#[from] std::io::Error),

    /// 命令退出码?0
    #[error("command exited with code {code}: {stderr}")]
    NonZeroExit {
        /// 退出码
        code: i32,
        /// stderr 内容
        stderr: String,
    },

    /// ctx.shell 错误 (P14.2.2: 由 ma-harness-shell 包装, 这里暴露给业务方)
    #[error("shell service error: {0}")]
    Shell(#[from] ShellError),
}

// ============================================================================
// CommandOutput ?命令执行结果
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

/// Bash service ?shell 命令
///
/// **P14.2.2 重构**: 内部从 `tokio::process::Command` 改为
/// 走 [`ma_harness_shell::ShellService`]. 业务方如果有 `ctx.shell` 注入的 provider, 用它;
/// 否则 fallback 到 `LocalShellProvider::new()` (平台默认).
///
/// **关键设计**: service 自身**不存** timeout, 每次 run 时从 ctx ?MAX_RUNTIME_MS.
/// 业务?set 这个 key 立刻生效 (?hello plugin ?"活的 ctx" 设计一?).
pub struct BashService;

impl BashService {
    /// ?shell 命令 (跨平? Linux/macOS ?sh, Windows ?cmd)
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
        let max_runtime_ms = ctx.get(MAX_RUNTIME_MS).unwrap_or(DEFAULT_MAX_RUNTIME_MS);
        self.run_command_with_timeout(ctx, cmd, Duration::from_millis(max_runtime_ms as u64))
            .await
    }

    /// ?shell 命令, 显式指定 timeout
    ///
    /// P14.2.2: 内部走 ctx.shell (从 SHELL_SERVICE typed key 拿, fallback 到 LocalShellProvider).
    pub async fn run_command_with_timeout(
        &self,
        ctx: &Context,
        cmd: &str,
        max_runtime: Duration,
    ) -> Result<CommandOutput, BashError> {
        let start = std::time::Instant::now();

        // 拿 ctx.shell (如果有) 否则 fallback
        let shell: Arc<dyn ShellService> = ctx
            .get(ma_harness_shell::SHELL_SERVICE)
            .unwrap_or_else(|| Arc::new(LocalShellProvider::new()));

        let spec = ShellSpec::new(cmd).timeout(max_runtime);
        let result = shell.execute(&spec).await?;

        let duration_ms = start.elapsed().as_millis() as u64;
        Ok(CommandOutput {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms,
        })
    }
}

impl CordisService for BashService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(BashService)
    }
    fn name(&self) -> &str {
        "bash"
    }
}

impl SeamService for BashService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(BashService)
    }
    fn name(&self) -> &str {
        "bash"
    }
}

// ============================================================================
// Plugin: BashPlugin
// ============================================================================

/// Bash plugin ?install 时注?BashService + 写默认 typed key
pub struct BashPlugin;

impl CordisPlugin for BashPlugin {
    fn install(&self, ctx: &Context) -> anyhow::Result<()> {
        let svc = <BashService as ma_harness_cordis::Service>::install(ctx)?;
        ctx.inject(std::sync::Arc::new(svc));
        // 默认 timeout (业务方可以覆?
        ctx.set(MAX_RUNTIME_MS, DEFAULT_MAX_RUNTIME_MS);
        // P14.2.2: 如果 ctx 还没装 SHELL_SERVICE, 装 LocalShellProvider
        // (业务方可以 ctx.set(SHELL_SERVICE, ...) 覆盖, 例如装 PwshShellProvider)
        if ctx.get(ma_harness_shell::SHELL_SERVICE).is_none() {
            ctx.set(
                ma_harness_shell::SHELL_SERVICE,
                Arc::new(LocalShellProvider::new()) as Arc<dyn ShellService>,
            );
        }
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
// 单元测试 (跨平? ?shell 内置命令)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_default_timeout() -> Context {
        let ctx = Context::new();
        ctx.set(MAX_RUNTIME_MS, DEFAULT_MAX_RUNTIME_MS);
        ctx
    }

    /// 跨平? 简?echo 命令
    #[tokio::test]
    async fn run_echo_command() {
        let ctx = ctx_with_default_timeout();
        let svc = BashService;
        let out = svc
            .run_command(&ctx, &echo_cmd("hello world"))
            .await
            .unwrap();
        assert!(out.is_success(), "echo 应成? stderr: {}", out.stderr);
        assert!(
            out.stdout.contains("hello world"),
            "stdout 应含 'hello world', got: {}",
            out.stdout
        );
        assert_eq!(out.exit_code, 0);
    }

    /// 跨平? false / non-zero exit
    #[tokio::test]
    async fn run_failing_command_returns_nonzero() {
        let ctx = ctx_with_default_timeout();
        let svc = BashService;
        // false ?Unix 退?1, Windows 没有 false ?exit 1 通过 cmd /C "exit 1"
        let out = svc.run_command(&ctx, &false_cmd()).await.unwrap(); // ?panic, exit 0 也是成功命令
        assert_eq!(out.exit_code, 1, "false 应退?1, got: {}", out.exit_code);
        assert!(!out.is_success());
    }

    /// 跨平? 捕获 stderr
    #[tokio::test]
    async fn run_captures_stderr() {
        let ctx = ctx_with_default_timeout();
        let svc = BashService;
        let out = svc.run_command(&ctx, &stderr_cmd("oops")).await.unwrap();
        // 命令本身退?0, ?stderr 有内?        assert!(out.is_success());
        assert!(
            out.stderr.contains("oops"),
            "stderr 应含 'oops', got: {}",
            out.stderr
        );
    }

    /// 跨平台 timeout (Phase 1 测试 sleep)
    #[tokio::test]
    async fn run_respects_timeout() {
        let ctx = ctx_with_default_timeout();
        let svc = BashService;
        let result = svc
            .run_command_with_timeout(&ctx, &sleep_cmd("5"), Duration::from_millis(100))
            .await;
        // P14.2.2: 走 ctx.shell, 超时返回 ShellError::Subprocess(...) — BashError::Shell 包装
        // 业务方只 assert BashError::Shell, 不需要解 SubprocessError 内部
        assert!(
            matches!(
                result,
                Err(BashError::Shell(ma_harness_shell::ShellError::Subprocess(
                    _
                )))
            ),
            "expected timeout wrapped in Shell, got: {:?}",
            result.map_err(|e| format!("{:?}", e))
        );
    }

    /// 业务方覆盖默?timeout
    #[tokio::test]
    async fn run_uses_ctx_overridden_timeout() {
        let ctx = Context::new();
        ctx.set(MAX_RUNTIME_MS, 100u32); // 100ms
        let svc = BashService;
        let result = svc.run_command(&ctx, &sleep_cmd("5")).await;
        assert!(matches!(
            result,
            Err(BashError::Shell(ma_harness_shell::ShellError::Subprocess(
                _
            )))
        ));
    }

    // ========================================================================
    // 跨平?helper
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
        // Windows 没有 sleep 内置, ?ping -n 凑合
        format!(
            "ping -n {} 127.0.0.1 > nul",
            secs.to_string().parse::<u32>().unwrap_or(5) + 1
        )
    }
}
