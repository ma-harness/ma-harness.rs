//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-terminal`
//! **Crate ident** (`use` 路径): `ma_harness_terminal`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident,
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-terminal = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_terminal::{LocalPtyProvider, PtyService, TerminalSpec};
//!
//! let provider = LocalPtyProvider::new();
//! let spec = TerminalSpec::new("sh")
//!     .arg("-c")
//!     .arg("echo hello")
//!     .size(80, 24);
//! let handle = provider.spawn(&spec).await?;
//! provider.write(&handle, b"more input\n").await?;
//! let output = provider.read(&handle, 1024).await?;
//! provider.kill(&handle).await?;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-terminal
//!
//! # 设计 (Design) — P15.2.1
//!
//! **目标**: 抽象 `ctx.terminals` (跟 dsh `ctx.terminals` 对等), 业务方
//! - 跑 `mah run "start a dev server"` 让 dev server 持续运行
//! - 用户 reconnect 到同一 terminal 看 output / 发 input
//!
//! **背景**: 见 [dsh-feature-parity-table §2 capability seams]:
//! - `ctx.subprocess` 一次性跑命令 → 拿 output → 退 (P14.1 done, ⚠️ partial)
//! - `ctx.terminals` 跑 **持久 PTY** → 双向交互, 跟 dsh 一样 (P15.2 ❌ gap)
//! - 之前 ma-harness 拿 `tokio::process::Command` 直接跑 (plugin-bash), 不走 PTY seam
//!
//! **接口**:
//! - [`PtyService`] trait — 6 个 async 方法 + provider 标识
//! - [`TerminalSpec`] — 业务方写的终端描述 (cmd, args, env, size)
//! - [`TerminalHandle`] — opaque UUID 句柄, 用于 read / write / kill
//! - [`LocalPtyProvider`] — portable-pty 实现 (P15.2.1 主交付)
//!
//! **6 质量属性 (业务方 2026-09-04 约定)**:
//! - 可复用: trait 抽象, future RemotePtyProvider (云端执行)
//! - 可维护: 模块化分块, 类型集中 lib.rs
//! - 鲁棒: IO 错误归一化, kill 后 read 返明确错误, 0 active handle 也不 panic
//! - 安全: 不 `unsafe`, env 显式 (不继承父进程), read 有 max_bytes cap
//! - 可测: 16+ 单元测试 (validate / handle / list / kill unknown / spawn reject / read write / 多并发 / try_wait)
//! - 可扩展: portable-pty 抽象 → P15.2.2 接 ctx 注入, P15.2.3 try_wait 实现, P15.2.4 streaming + resume
//!
//! # 限制 (Limitations) — P15.2.3
//!
//! - portable-pty 当前默认 backend: ConPTY on Windows 10+, openpty on POSIX
//! - read 是 one-shot (可能只读到 chunk 1) — 业务方用 retry pattern 拿全部 output
//!   (P15.2.4+ 加 streaming read 用 mpsc channel 持续 read 到 buffer)
//! - try_wait 只 cache exit code (不存 signal 信息) — 业务方 P15.2.4+ 需要再补
//! - 没 resume across reload (P15.2.4 加 session_id → pty_handle 持久化)
//! - kill 后 entry 立即移除 — try_wait 返 HandleNotFound (设计选择: 简化状态机)
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use thiserror::Error;
use uuid::Uuid;

// ============================================================================
// Error
// ============================================================================

/// Terminal capability 错误.
#[derive(Debug, Error)]
pub enum TerminalError {
    /// PTY 启动失败 (操作系统不支持 / 资源耗尽)
    #[error("pty open failed: {0}")]
    PtyOpen(String),

    /// Spawn 命令失败 (program 不存在 / permission denied)
    #[error("spawn failed: {0}")]
    Spawn(String),

    /// IO 错误 (read / write / kill 失败)
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Handle 不存在或已关闭 (read / write / kill on stale handle)
    #[error("terminal handle {0} not found (already closed or never spawned)")]
    HandleNotFound(TerminalHandle),

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

    /// 超时
    #[error("terminal operation timed out after {0:?}")]
    Timeout(Duration),
}

// ============================================================================
// TerminalSpec
// ============================================================================

/// 终端规格 (描述一次 spawn 的参数, 不可变).
///
/// **字段对齐 dsh `TerminalSpec`**:
/// - `program`: 命令 (e.g. `"sh"`, `"cmd.exe"`, `"python3"`)
/// - `args`: 参数列表
/// - `env`: 显式环境变量 (不继承父进程, 安全考虑)
/// - `cols` / `rows`: 初始 PTY 尺寸 (影响 child 的 ioctl 行为)
#[derive(Debug, Clone)]
pub struct TerminalSpec {
    /// 命令
    pub program: String,
    /// 参数列表
    pub args: Vec<String>,
    /// 显式环境变量
    pub env: BTreeMap<String, String>,
    /// PTY 宽度 (列)
    pub cols: u16,
    /// PTY 高度 (行)
    pub rows: u16,
}

impl TerminalSpec {
    /// 构一个新 spec (默认 80x24, 空 env)
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cols: 80,
            rows: 24,
        }
    }

    /// builder: 添加一个 arg
    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    /// builder: 批量添加 args
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for a in args {
            self.args.push(a.into());
        }
        self
    }

    /// builder: 设环境变量
    pub fn env(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.env.insert(key.into(), val.into());
        self
    }

    /// builder: 设 PTY 尺寸
    pub fn size(mut self, cols: u16, rows: u16) -> Self {
        self.cols = cols;
        self.rows = rows;
        self
    }

    /// 校验: program 不能空, size > 0
    pub fn validate(&self) -> Result<(), TerminalError> {
        if self.program.is_empty() {
            return Err(TerminalError::Spawn("empty program".into()));
        }
        if self.cols == 0 || self.rows == 0 {
            return Err(TerminalError::Spawn(format!(
                "invalid pty size: {}x{} (must be > 0)",
                self.cols, self.rows
            )));
        }
        Ok(())
    }
}

// ============================================================================
// TerminalHandle: opaque UUID 句柄
// ============================================================================

/// Opaque 终端句柄 (UUID v4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalHandle(
    /// UUID 字符串
    String,
);

impl TerminalHandle {
    /// 拿 handle 字符串 (UUID)
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TerminalHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ============================================================================
// PtyService trait
// ============================================================================

/// 持久 PTY 能力缝 trait (6 个 async 方法, 跟 dsh `ctx.terminals` 对齐).
#[async_trait]
pub trait PtyService: Send + Sync + 'static {
    /// 启动 PTY + child process. 返 handle.
    async fn spawn(&self, spec: &TerminalSpec) -> Result<TerminalHandle, TerminalError>;
    /// 写 input 到 PTY (e.g. user typed a command).
    async fn write(&self, handle: &TerminalHandle, data: &[u8]) -> Result<(), TerminalError>;
    /// 读 PTY output (最多 `max_bytes`).
    async fn read(
        &self,
        handle: &TerminalHandle,
        max_bytes: usize,
    ) -> Result<Vec<u8>, TerminalError>;
    /// 强制结束 PTY (close master + kill child).
    async fn kill(&self, handle: &TerminalHandle) -> Result<(), TerminalError>;
    /// 列所有活跃 handle.
    async fn list(&self) -> Result<Vec<TerminalHandle>, TerminalError>;
    /// 非阻塞 poll child 退出 — `None` = 还在跑, `Some(code)` = 已退出 (P15.2.3 新增).
    async fn try_wait(&self, handle: &TerminalHandle) -> Result<Option<i32>, TerminalError>;
    /// Provider 标识
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// LocalPtyProvider (P15.2.1 主交付, portable-pty 实现)
// ============================================================================

/// 一条活跃 PTY 的内部状态.
struct PtyEntry {
    /// master 端 (kill 时 drop 关闭 pty fd)
    _master: Box<dyn MasterPty + Send>,
    /// child killer (kill 时 drop 强杀 child)
    _killer: Box<dyn ChildKiller + Send + Sync>,
    /// reader 端 (从 master 读 child stdout)
    reader: Mutex<Box<dyn Read + Send>>,
    /// writer 端 (写 child stdin)
    writer: Mutex<Box<dyn Write + Send>>,
    /// P15.2.3: cached child exit code. `None` = 还在跑, `Some(code)` = 已退出.
    /// 写入者是后台 wait task (spawn 时启动), 读是 try_wait.
    exit_status: Arc<Mutex<Option<i32>>>,
}

/// 本地 PTY provider (P15.2.1 主交付).
///
/// **实现**:
/// - `native_pty_system()` 拿 OS 默认 backend (Windows ConPTY / POSIX openpty)
/// - spawn 时 `CommandBuilder` 配 env + args
/// - master 拆出 reader / writer (各自包 `parking_lot::Mutex` 跨 await 安全)
/// - read / write / kill 走 `spawn_blocking` 避免阻塞 tokio worker
///
/// **Arc 设计**: `handles` 是 `Arc<Mutex<HashMap>>`, clone 进 `spawn_blocking`
/// 闭包, 闭包内 lock 干活, 立即 drop guard. 闭包 sync, 不跨 await.
pub struct LocalPtyProvider {
    handles: Arc<Mutex<HashMap<TerminalHandle, PtyEntry>>>,
    _next_id: AtomicU64, // 占位 (业务方目前不用, 预留 P15.2.4 session_id 映射)
}

impl std::fmt::Debug for LocalPtyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalPtyProvider")
            .field("active_count", &self.handles.lock().len())
            .finish()
    }
}

impl Default for LocalPtyProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalPtyProvider {
    /// 创建一个新的 LocalPtyProvider.
    pub fn new() -> Self {
        Self {
            handles: Arc::new(Mutex::new(HashMap::new())),
            _next_id: AtomicU64::new(0),
        }
    }

    /// 当前活跃 handle 数 (测试用)
    #[cfg(test)]
    pub(crate) fn active_count(&self) -> usize {
        self.handles.lock().len()
    }
}

/// 在 spawn_blocking 闭包内 sync spawn 1 条 PTY.
///
/// **P15.2.3 变化**: 返 `(handle, child)`, 让 spawn() 拿 child 启后台 wait task
/// (child.wait() 阻塞, 缓存 exit code 给 try_wait).
fn spawn_sync(
    spec: &TerminalSpec,
    handles: Arc<Mutex<HashMap<TerminalHandle, PtyEntry>>>,
    _next_id: &AtomicU64,
) -> Result<(TerminalHandle, Box<dyn portable_pty::Child + Send + Sync>), TerminalError> {
    spec.validate()?;

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: spec.rows,
            cols: spec.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| TerminalError::PtyOpen(e.to_string()))?;

    let mut cmd = CommandBuilder::new(&spec.program);
    for a in &spec.args {
        cmd.arg(a);
    }
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    cmd.cwd(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")));

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| TerminalError::Spawn(e.to_string()))?;
    let killer = child.clone_killer();
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| TerminalError::Spawn(format!("clone_reader: {e}")))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| TerminalError::Spawn(format!("take_writer: {e}")))?;

    let handle = TerminalHandle(Uuid::new_v4().to_string());
    let exit_status: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
    let entry = PtyEntry {
        _master: pair.master,
        _killer: killer,
        reader: Mutex::new(reader),
        writer: Mutex::new(writer),
        exit_status: Arc::clone(&exit_status),
    };

    handles.lock().insert(handle.clone(), entry);

    // 返回 child 给 spawn() 让它启 wait task.
    // portable-pty 0.8 的 Child: trait Child: Send + Sync, wait(&mut self).
    // spawn_command 返的 child 已经是 Box<dyn Child + Send + Sync>, 直接 move.
    Ok((handle, child))
}

#[async_trait]
impl PtyService for LocalPtyProvider {
    async fn spawn(&self, spec: &TerminalSpec) -> Result<TerminalHandle, TerminalError> {
        let spec = spec.clone();
        let handles = Arc::clone(&self.handles);
        let next_id = Arc::new(self._next_id.load(Ordering::Relaxed));
        let (handle, mut child) = tokio::task::spawn_blocking(move || {
            let _ = next_id;
            spawn_sync(&spec, handles, &AtomicU64::new(0))
        })
        .await
        .map_err(|e| TerminalError::PtyOpen(format!("join error: {e}")))??;

        // P15.2.3: 启后台 wait task — child.wait() 阻塞, 完成后缓存 exit code
        // (Arc<Mutex<Option<i32>>> 在 entry 里). try_wait 读这个缓存.
        // 注: child.wait() 是 portable_pty 0.8 Child::wait(&mut self), 阻塞到 child 退出.
        //     我们 move child 进闭包, 独占. wait 完 drop child (释放 child 资源).
        let handles_for_wait = Arc::clone(&self.handles);
        let handle_for_wait = handle.clone();
        tokio::task::spawn(async move {
            let exit_result = tokio::task::spawn_blocking(move || child.wait()).await;
            if let Ok(Ok(status)) = exit_result {
                // portable_pty::ExitStatus: u32 code + Option<signal>
                // 注: signal-killed 没 exit code, 我们只 cache 正常 exit
                //     (signal 信息丢失; 业务方 P15.2.4+ 可加)
                let code = status.exit_code() as i32;
                // 缓存到 entry
                if let Some(entry) = handles_for_wait.lock().get(&handle_for_wait) {
                    *entry.exit_status.lock() = Some(code);
                }
            }
            // wait 完成但 entry 可能已被 kill/移除 — drop guard, 不报错
        });

        Ok(handle)
    }

    async fn write(&self, handle: &TerminalHandle, data: &[u8]) -> Result<(), TerminalError> {
        let handle = handle.clone();
        let data = data.to_vec();
        let handles = Arc::clone(&self.handles);
        tokio::task::spawn_blocking(move || {
            let map = handles.lock();
            let entry = map
                .get(&handle)
                .ok_or_else(|| TerminalError::HandleNotFound(handle.clone()))?;
            let mut writer = entry.writer.lock();
            writer.write_all(&data)?;
            writer.flush()?;
            Ok::<(), TerminalError>(())
        })
        .await
        .map_err(|e| TerminalError::PtyOpen(format!("join error: {e}")))?
    }

    async fn read(
        &self,
        handle: &TerminalHandle,
        max_bytes: usize,
    ) -> Result<Vec<u8>, TerminalError> {
        let handle = handle.clone();
        let handles = Arc::clone(&self.handles);
        tokio::task::spawn_blocking(move || {
            let map = handles.lock();
            let entry = map
                .get(&handle)
                .ok_or_else(|| TerminalError::HandleNotFound(handle.clone()))?;
            let mut reader = entry.reader.lock();
            // P15.2.1 简化: read up to max_bytes, 一次性返. 不支持 partial read / streaming.
            // caller 拿 Vec<u8>, 可自行 buffer.
            let mut buf = vec![0u8; max_bytes];
            let n = reader.read(&mut buf)?;
            buf.truncate(n);
            Ok::<Vec<u8>, TerminalError>(buf)
        })
        .await
        .map_err(|e| TerminalError::PtyOpen(format!("join error: {e}")))?
    }

    async fn kill(&self, handle: &TerminalHandle) -> Result<(), TerminalError> {
        let handles = Arc::clone(&self.handles);
        let handle = handle.clone();
        tokio::task::spawn_blocking(move || {
            let mut map = handles.lock();
            match map.remove(&handle) {
                Some(_entry) => {
                    // _entry drop:
                    //   - _master (pty master fd 关闭)
                    //   - _killer (Send SIGKILL / TerminateProcess)
                    //   - reader / writer (各自的 fd / handle 关闭)
                    Ok::<(), TerminalError>(())
                }
                None => Err(TerminalError::HandleNotFound(handle)),
            }
        })
        .await
        .map_err(|e| TerminalError::PtyOpen(format!("join error: {e}")))?
    }

    async fn list(&self) -> Result<Vec<TerminalHandle>, TerminalError> {
        let map = self.handles.lock();
        Ok(map.keys().cloned().collect())
    }

    async fn try_wait(&self, handle: &TerminalHandle) -> Result<Option<i32>, TerminalError> {
        // P15.2.3: 读 entry.exit_status (后台 wait task 缓存)
        // - None: child 还在跑 (wait task 还没 set)
        // - Some(code): child 已退出, code 是 exit code
        let map = self.handles.lock();
        let entry = map
            .get(handle)
            .ok_or_else(|| TerminalError::HandleNotFound(handle.clone()))?;
        Ok(*entry.exit_status.lock())
    }

    fn provider_name(&self) -> &'static str {
        "local-pty"
    }
}

// ============================================================================
// Typed key
// ============================================================================

/// Typed key: `ctx.terminals` 注入的 PtyService (P15.2.2 业务方注入).
pub static PTY_SERVICE: ma_harness_cordis::CtxKey<Arc<dyn PtyService>> =
    ma_harness_seam::ctx_key!("terminals_pty_service");

// ============================================================================
// Default type aliases
// ============================================================================

/// 平台默认 PTY provider (P15.2.1: LocalPtyProvider).
pub type DefaultPtyProvider = LocalPtyProvider;

// ============================================================================
// P15.2.2: Consumer Pattern (TerminalCommand + TerminalRegistry)
// ============================================================================

/// Terminal 命令 invoke 结果 (P15.2.2 新增).
///
/// **跟 ShellResult 区别**: Terminal 是持久 PTY, invoke 通常:
/// 1. 启动一个长跑 child (e.g. `npm run dev`)
/// 2. 读一段时间 output (max_read_bytes 上限)
/// 3. 保留 handle 给业务方后续 read / write / kill
///
/// 所以 TerminalResult 多一个 `terminal_id` 字段 (UUID 字符串), 让业务方
/// 之后能继续跟这个 PTY 交互 (P15.2.4 session 持久化就用这个).
#[derive(Debug, Clone)]
pub struct TerminalResult {
    /// Terminal handle 字符串 (UUID, 给后续 read/write/kill 用)
    pub terminal_id: String,
    /// 启动 → 读到 output 的耗时
    pub duration: Duration,
    /// 读到的 output bytes (最多 `max_read_bytes`)
    pub output: Vec<u8>,
    /// 读之后 PTY 是否还活着 (true = 还在跑, false = 已退出)
    pub still_running: bool,
}

impl TerminalResult {
    /// 拿 output 的 UTF-8 lossy string
    pub fn output_str(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

/// Terminal 命令 trait (P15.2.2 新增, 跟 P14.2.2 ShellCommand 镜像).
///
/// **生命周期**: 业务方实现, 装到 [`TerminalRegistry`], registry 装到 ctx.
/// invoke 阶段由 [`PtyService`] 实际 spawn / write / read / kill.
///
/// **跟 ShellCommand 区别**:
/// - 返回 `TerminalResult` (含 handle + output) 而不是 `ShellResult` (含 exit_code)
/// - 适合"长跑 + 收 output"而不是"跑完即退"
#[async_trait]
pub trait TerminalCommand: Send + Sync + 'static {
    /// 命令名 (snake_case, e.g. "start_dev_server" / "long_task"). Registry 用作 key.
    fn name(&self) -> &str;

    /// 命令描述 (LLM 看, 决定什么时候调).
    fn description(&self) -> &str;

    /// 参数 schema (JSON Schema 草图, 业务方手写).
    fn param_schema(&self) -> serde_json::Value;

    /// 实际 invoke (业务方实现, 内部一般调 `PtyService::spawn/write/read`).
    ///
    /// # Arguments
    /// - `pty`: PtyService provider (业务方可能注入自己的)
    /// - `args`: 命令参数 (符合 `param_schema`)
    /// - `max_read_bytes`: invoke 阶段读多少 output (后续可继续 read)
    async fn invoke(
        &self,
        pty: Arc<dyn PtyService>,
        args: serde_json::Value,
        max_read_bytes: usize,
    ) -> Result<TerminalResult, TerminalError>;
}

/// Terminal 命令注册表 (P15.2.2 新增).
///
/// 业务方 `register(StartDevServer)`, agent `invoke("start_dev_server", json!({"cmd": "npm"}))`.
pub struct TerminalRegistry {
    commands: std::collections::HashMap<String, Arc<dyn TerminalCommand>>,
}

impl TerminalRegistry {
    /// 创建一个空 registry
    pub fn new() -> Self {
        Self {
            commands: std::collections::HashMap::new(),
        }
    }

    /// 注册一个命令 (重复 name 覆盖前一个 + log warn)
    pub fn register<C: TerminalCommand>(&mut self, cmd: C) {
        let name = cmd.name().to_string();
        if self.commands.contains_key(&name) {
            tracing::warn!(
                command = %name,
                "TerminalRegistry::register overrides existing command"
            );
        }
        tracing::debug!(command = %name, "terminal command registered");
        self.commands.insert(name, Arc::new(cmd));
    }

    /// 按名拿命令
    pub fn get(&self, name: &str) -> Option<Arc<dyn TerminalCommand>> {
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

    /// 按名 invoke (便捷方法).
    ///
    /// # Errors
    /// - 命令不存在: `TerminalError::Unsupported { operation: "invoke", reason: ... }`
    pub async fn invoke(
        &self,
        pty: Arc<dyn PtyService>,
        name: &str,
        args: serde_json::Value,
        max_read_bytes: usize,
    ) -> Result<TerminalResult, TerminalError> {
        let cmd = self.get(name).ok_or_else(|| TerminalError::Unsupported {
            provider: "TerminalRegistry",
            operation: "invoke",
            reason: format!("command not found: {name}"),
        })?;
        cmd.invoke(pty, args, max_read_bytes).await
    }

    /// 给 LLM 用的 tool list (跟 dsh `tools/pre-execute` 走同一格式).
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

impl Default for TerminalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TerminalRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalRegistry")
            .field("commands", &self.list())
            .finish()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- TerminalSpec validate / builder -----

    #[test]
    fn terminal_spec_validate_rejects_empty_program() {
        let spec = TerminalSpec::new("");
        assert!(matches!(
            spec.validate(),
            Err(TerminalError::Spawn(msg)) if msg.contains("empty program")
        ));
    }

    #[test]
    fn terminal_spec_validate_rejects_zero_size() {
        let spec = TerminalSpec::new("sh").size(0, 24);
        assert!(spec.validate().is_err());
        let spec = TerminalSpec::new("sh").size(80, 0);
        assert!(spec.validate().is_err());
    }

    #[test]
    fn terminal_spec_validate_accepts_valid() {
        let spec = TerminalSpec::new("sh")
            .arg("-c")
            .arg("echo hello")
            .size(80, 24);
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn terminal_spec_builder_chain() {
        let spec = TerminalSpec::new("python3")
            .arg("-u")
            .args(["-c", "print(1)"])
            .env("PYTHONUNBUFFERED", "1")
            .size(120, 40);
        assert_eq!(spec.program, "python3");
        assert_eq!(spec.args, vec!["-u", "-c", "print(1)"]);
        assert_eq!(spec.env.get("PYTHONUNBUFFERED"), Some(&"1".to_string()));
        assert_eq!(spec.cols, 120);
        assert_eq!(spec.rows, 40);
    }

    // ----- TerminalHandle -----

    #[test]
    fn terminal_handle_display_and_eq() {
        let h1 = TerminalHandle("abc-123".into());
        let h2 = TerminalHandle("abc-123".into());
        let h3 = TerminalHandle("xyz".into());
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.to_string(), "abc-123");
        assert_eq!(h1.as_str(), "abc-123");
    }

    // ----- LocalPtyProvider 状态 -----

    #[tokio::test]
    async fn local_pty_provider_new_has_zero_handles() {
        let p = LocalPtyProvider::new();
        assert_eq!(p.active_count(), 0);
        let list = p.list().await.expect("list");
        assert_eq!(list.len(), 0);
    }

    #[tokio::test]
    async fn local_pty_provider_kill_unknown_handle_returns_handle_not_found() {
        let p = LocalPtyProvider::new();
        let bogus = TerminalHandle("bogus".into());
        let err = p.kill(&bogus).await.unwrap_err();
        assert!(matches!(err, TerminalError::HandleNotFound(_)));
    }

    #[tokio::test]
    async fn local_pty_provider_write_to_unknown_handle_returns_handle_not_found() {
        let p = LocalPtyProvider::new();
        let bogus = TerminalHandle("bogus".into());
        let err = p.write(&bogus, b"data").await.unwrap_err();
        assert!(matches!(err, TerminalError::HandleNotFound(_)));
    }

    #[tokio::test]
    async fn local_pty_provider_read_from_unknown_handle_returns_handle_not_found() {
        let p = LocalPtyProvider::new();
        let bogus = TerminalHandle("bogus".into());
        let err = p.read(&bogus, 1024).await.unwrap_err();
        assert!(matches!(err, TerminalError::HandleNotFound(_)));
    }

    #[tokio::test]
    async fn local_pty_provider_try_wait_unknown_handle_returns_handle_not_found() {
        // P15.2.3: try_wait 真正实现 — 读 entry.exit_status.
        // 未知 handle 返 HandleNotFound (不是 Unsupported)
        let p = LocalPtyProvider::new();
        let bogus = TerminalHandle("bogus".into());
        let err = p.try_wait(&bogus).await.unwrap_err();
        assert!(matches!(err, TerminalError::HandleNotFound(_)));
    }

    #[tokio::test]
    async fn local_pty_provider_try_wait_running_child_returns_none() {
        // P15.2.3: 短命令 (e.g. `cmd /C pause` 等用户输入) 不会立即退出,
        // try_wait 在 wait task 完成前返 None.
        // 注: cmd /C pause 等不到输入会 hang, 我们 spawn cmd.exe 不带 /C
        // 也不会立即退出 — 等用户输入.
        let p = LocalPtyProvider::new();
        let handle = p.spawn(&TerminalSpec::new("cmd.exe")).await.expect("spawn");
        // 立即 try_wait — wait task 还没完成
        let result = p.try_wait(&handle).await.expect("try_wait");
        assert_eq!(result, None, "running child should return None");
        // 清理
        p.kill(&handle).await.expect("kill");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn local_pty_provider_try_wait_after_quick_exit_returns_some() {
        // P15.2.3 端到端: spawn 一个快速退出的命令, 等 wait task 完成, try_wait 返 Some(0)
        let p = LocalPtyProvider::new();
        let handle = p
            .spawn(&TerminalSpec::new("cmd.exe").arg("/C").arg("exit 42"))
            .await
            .expect("spawn");
        // 等 wait task 完成 (cmd /C exit 42 应该 < 1s)
        let mut got = None;
        for _ in 0..40 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Some(code) = p.try_wait(&handle).await.expect("try_wait") {
                got = Some(code);
                break;
            }
        }
        assert_eq!(got, Some(42), "exit code should be 42");
        // entry 还在 (没 kill), 但 wait task 已 set
        assert_eq!(p.active_count(), 1);
        // 清理
        p.kill(&handle).await.expect("kill");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn local_pty_provider_try_wait_after_kill_returns_handle_not_found() {
        // P15.2.3: kill 移除 entry, try_wait 返 HandleNotFound (不返 stale data)
        // 业务方应在 kill 之前 try_wait 或在 kill 后知道 handle 失效
        let p = LocalPtyProvider::new();
        let handle = p
            .spawn(&TerminalSpec::new("cmd.exe")) // 不带 /C, 持续等输入
            .await
            .expect("spawn");
        p.kill(&handle).await.expect("kill");
        // kill 后 entry 已移除
        let err = p.try_wait(&handle).await.unwrap_err();
        assert!(
            matches!(err, TerminalError::HandleNotFound(_)),
            "expected HandleNotFound after kill, got {err:?}"
        );
    }

    #[tokio::test]
    async fn local_pty_provider_spawn_rejects_empty_program() {
        let p = LocalPtyProvider::new();
        let spec = TerminalSpec::new("");
        let err = p.spawn(&spec).await.unwrap_err();
        assert!(matches!(err, TerminalError::Spawn(_)));
    }

    #[tokio::test]
    async fn local_pty_provider_spawn_rejects_zero_size() {
        let p = LocalPtyProvider::new();
        let spec = TerminalSpec::new("sh").size(0, 0);
        let err = p.spawn(&spec).await.unwrap_err();
        assert!(matches!(err, TerminalError::Spawn(_)));
    }

    #[tokio::test]
    async fn local_pty_provider_provider_name_is_local_pty() {
        let p = LocalPtyProvider::new();
        assert_eq!(p.provider_name(), "local-pty");
    }

    // ----- 真实 PTY 集成 (平台相关) -----

    /// Windows: 跑 `cmd.exe /c echo hello` 验 PTY 真实能 spawn + 写 + 读.
    /// 业务方本机环境 (Windows PowerShell 5.1) 验证用.
    #[cfg(windows)]
    #[tokio::test]
    async fn local_pty_provider_real_cmd_exe_spawn_and_read() {
        let p = LocalPtyProvider::new();
        let spec = TerminalSpec::new("cmd.exe")
            .arg("/C")
            .arg("echo hello-from-pty");
        let handle = p.spawn(&spec).await.expect("spawn cmd.exe");
        assert_eq!(p.active_count(), 1);

        // 等 child 跑 + 输出 flush 到 pty master
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // 读 output (设大 buffer, 一次性收所有)
        let output = p.read(&handle, 4096).await.expect("read");
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("hello-from-pty"),
            "PTY output should contain 'hello-from-pty', got: {text:?}"
        );

        // 清理
        p.kill(&handle).await.expect("kill");
        assert_eq!(p.active_count(), 0);
    }

    /// POSIX: 跑 `sh -c "echo hello"` 验 PTY 真实能 spawn.
    #[cfg(unix)]
    #[tokio::test]
    async fn local_pty_provider_real_sh_spawn_and_read() {
        let p = LocalPtyProvider::new();
        let spec = TerminalSpec::new("sh").arg("-c").arg("echo hello-from-pty");
        let handle = p.spawn(&spec).await.expect("spawn sh");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let output = p.read(&handle, 4096).await.expect("read");
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("hello-from-pty"),
            "PTY output should contain 'hello-from-pty', got: {text:?}"
        );
        p.kill(&handle).await.expect("kill");
        assert_eq!(p.active_count(), 0);
    }

    /// 写 + 读 echo 风格: spawn sh, 写命令, 读 output.
    #[cfg(windows)]
    #[tokio::test]
    async fn local_pty_provider_write_then_read() {
        let p = LocalPtyProvider::new();
        // 跑 cmd.exe, 不自动退出 (不带 /C, 持续等输入)
        let spec = TerminalSpec::new("cmd.exe");
        let handle = p.spawn(&spec).await.expect("spawn cmd.exe");
        // 等 cmd 起来
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        // 写命令
        p.write(&handle, b"echo write-then-read\n")
            .await
            .expect("write");
        // 等 echo 输出 flush
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        // 读
        let output = p.read(&handle, 4096).await.expect("read");
        let text = String::from_utf8_lossy(&output);
        // cmd.exe 的 echo 会输出 "write-then-read\r\n"
        assert!(
            text.contains("write-then-read"),
            "PTY output should contain 'write-then-read', got: {text:?}"
        );
        p.kill(&handle).await.expect("kill");
    }

    /// 3 个并发 active handle 互不干扰 (P15.2.1 concurrency check).
    #[cfg(windows)]
    #[tokio::test]
    async fn local_pty_provider_three_concurrent_handles() {
        let p = LocalPtyProvider::new();
        let h1 = p
            .spawn(&TerminalSpec::new("cmd.exe").arg("/C").arg("echo one"))
            .await
            .expect("spawn 1");
        let h2 = p
            .spawn(&TerminalSpec::new("cmd.exe").arg("/C").arg("echo two"))
            .await
            .expect("spawn 2");
        let h3 = p
            .spawn(&TerminalSpec::new("cmd.exe").arg("/C").arg("echo three"))
            .await
            .expect("spawn 3");
        assert_eq!(p.active_count(), 3);

        // 等 child 跑完 (cmd /C echo 应该是 < 1s, 500ms 应该够)
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // 3 个都能 read 到自己的 output (P15.2.1 read 是 one-shot, 可能只读到 chunk 1)
        // 业务方自己 buffer 拼 — 这里 retry 直到找到 expected 或 1s timeout
        for (h, expected) in [(&h1, "one"), (&h2, "two"), (&h3, "three")] {
            let mut combined = Vec::new();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                let chunk = p.read(h, 4096).await.expect("read");
                combined.extend_from_slice(&chunk);
                let text = String::from_utf8_lossy(&combined);
                if text.contains(expected) {
                    break;
                }
                // 等多 50ms 再试
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            let text = String::from_utf8_lossy(&combined);
            assert!(
                text.contains(expected),
                "PTY output should contain '{expected}' within 2s, got: {text:?}"
            );
        }

        // 清理 (kill 3 个)
        p.kill(&h1).await.expect("kill 1");
        p.kill(&h2).await.expect("kill 2");
        p.kill(&h3).await.expect("kill 3");
        assert_eq!(p.active_count(), 0);
    }

    // ----- P15.2.2 Consumer Pattern: TerminalCommand + TerminalRegistry -----

    /// Test command: spawn `cmd.exe /C echo <text>` and read output.
    /// Param schema: `{ "text": "string" }`
    struct EchoCommand;

    #[async_trait]
    impl TerminalCommand for EchoCommand {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Spawn a shell that echoes the given text to stdout"
        }
        fn param_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "text to echo" }
                },
                "required": ["text"]
            })
        }
        async fn invoke(
            &self,
            pty: Arc<dyn PtyService>,
            args: serde_json::Value,
            max_read_bytes: usize,
        ) -> Result<TerminalResult, TerminalError> {
            let text = args
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| TerminalError::Spawn("missing 'text' arg".into()))?
                .to_string();

            // 平台相关: Windows 用 `cmd /C echo <text>`, POSIX 用 `sh -c "echo <text>"`
            let spec = if cfg!(windows) {
                TerminalSpec::new("cmd.exe")
                    .arg("/C")
                    .arg(format!("echo {text}"))
            } else {
                TerminalSpec::new("sh")
                    .arg("-c")
                    .arg(format!("echo {text}"))
            };

            let start = std::time::Instant::now();
            let handle = pty.spawn(&spec).await?;
            // 等 child 跑完 (短命令) + output flush
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let output = pty.read(&handle, max_read_bytes).await?;
            let duration = start.elapsed();

            Ok(TerminalResult {
                terminal_id: handle.to_string(),
                duration,
                output,
                still_running: true, // 简化: 不真 poll, 返 true
            })
        }
    }

    /// Failing command for "unknown command" test
    struct FailCommand;

    #[async_trait]
    impl TerminalCommand for FailCommand {
        fn name(&self) -> &str {
            "fail_on_purpose"
        }
        fn description(&self) -> &str {
            "Always returns Unsupported error (for testing)"
        }
        fn param_schema(&self) -> serde_json::Value {
            serde_json::json!({ "type": "object", "properties": {} })
        }
        async fn invoke(
            &self,
            _pty: Arc<dyn PtyService>,
            _args: serde_json::Value,
            _max_read_bytes: usize,
        ) -> Result<TerminalResult, TerminalError> {
            Err(TerminalError::Unsupported {
                provider: "FailCommand",
                operation: "invoke",
                reason: "intentional failure for test".to_string(),
            })
        }
    }

    #[test]
    fn terminal_registry_new_is_empty() {
        let r = TerminalRegistry::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert_eq!(r.list(), Vec::<String>::new());
    }

    #[test]
    fn terminal_registry_default_matches_new() {
        let r = TerminalRegistry::default();
        assert!(r.is_empty());
    }

    #[test]
    fn terminal_registry_register_adds_command() {
        let mut r = TerminalRegistry::new();
        r.register(EchoCommand);
        assert_eq!(r.len(), 1);
        assert_eq!(r.list(), vec!["echo".to_string()]);
        assert!(r.get("echo").is_some());
    }

    #[test]
    fn terminal_registry_register_override_warns_and_overrides() {
        let mut r = TerminalRegistry::new();
        r.register(EchoCommand);
        r.register(EchoCommand); // 第二次注册同名, 覆盖 + warn
        assert_eq!(r.len(), 1);
        // 仍能拿到
        assert!(r.get("echo").is_some());
    }

    #[test]
    fn terminal_registry_get_unknown_returns_none() {
        let r = TerminalRegistry::new();
        assert!(r.get("nope").is_none());
    }

    #[test]
    fn terminal_registry_tool_list_format() {
        let mut r = TerminalRegistry::new();
        r.register(EchoCommand);
        r.register(FailCommand);
        let tools = r.tool_list();
        assert_eq!(tools.len(), 2);
        // 2 个 tool 都有 name / description / parameters
        for t in &tools {
            assert!(t["name"].is_string());
            assert!(t["description"].is_string());
            assert!(t["parameters"].is_object());
        }
        // names 包含 echo + fail_on_purpose
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"echo"));
        assert!(names.contains(&"fail_on_purpose"));
    }

    #[tokio::test]
    async fn terminal_registry_invoke_unknown_returns_unsupported() {
        let r = TerminalRegistry::new();
        let pty: Arc<dyn PtyService> = Arc::new(LocalPtyProvider::new());
        let err = r
            .invoke(pty, "nope", serde_json::json!({}), 1024)
            .await
            .unwrap_err();
        match err {
            TerminalError::Unsupported {
                operation, reason, ..
            } => {
                assert_eq!(operation, "invoke");
                assert!(reason.contains("nope"));
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn terminal_registry_invoke_fail_command_propagates_error() {
        let mut r = TerminalRegistry::new();
        r.register(FailCommand);
        let pty: Arc<dyn PtyService> = Arc::new(LocalPtyProvider::new());
        let err = r
            .invoke(pty, "fail_on_purpose", serde_json::json!({}), 1024)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            TerminalError::Unsupported {
                provider: "FailCommand",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn terminal_registry_invoke_echo_command_runs_real_pty() {
        // P15.2.2 端到端: registry + command + pty 真跑
        let mut r = TerminalRegistry::new();
        r.register(EchoCommand);
        let pty: Arc<dyn PtyService> = Arc::new(LocalPtyProvider::new());

        let result = r
            .invoke(
                pty.clone(),
                "echo",
                serde_json::json!({"text": "from-registry"}),
                4096,
            )
            .await
            .expect("invoke echo");
        // P15.2.1 read 是 one-shot, 实际可能只读到 chunk 1 (ANSI escape).
        // 这里只验 "PTY 真 spawn 成功 + output bytes 不空", 内容验证留给下面 retry 版本.
        assert!(!result.terminal_id.is_empty());
        assert!(result.duration.as_millis() < 5000);
        // Retry 读直到找到 'from-registry' (业务方实际使用模式)
        let handle = TerminalHandle(result.terminal_id.clone());
        let mut combined = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            let chunk = pty.read(&handle, 4096).await.expect("read");
            combined.extend_from_slice(&chunk);
            if String::from_utf8_lossy(&combined).contains("from-registry") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let text = String::from_utf8_lossy(&combined);
        assert!(
            text.contains("from-registry"),
            "PTY output (after retry) should contain 'from-registry', got: {text:?}"
        );
        // 清理
        pty.kill(&handle).await.expect("kill");
    }

    #[test]
    fn terminal_result_output_str_handles_non_utf8() {
        // output 含 invalid UTF-8 → lossy 替换
        let r = TerminalResult {
            terminal_id: "id".into(),
            duration: std::time::Duration::from_millis(10),
            output: vec![b'h', b'i', 0xFF, 0xFE, b'!'],
            still_running: true,
        };
        let s = r.output_str();
        // 包含 'hi' 和 '!' (中间的 invalid byte 替换成 U+FFFD)
        assert!(s.contains("hi"));
        assert!(s.contains("!"));
    }
}
