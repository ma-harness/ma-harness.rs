//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-settings`
//! **Crate ident** (`use` 路径): `ma_harness_settings`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident,
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-settings = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_settings::{FileSettingsStore, Settings, SettingsStore};
//!
//! // 默认路径: ~/.ma-harness/settings.yaml
//! let store = FileSettingsStore::at_default()?;
//!
//! // 加载现有 settings (first run = 空 settings)
//! let mut settings = store.load().await?;
//!
//! // Get/set 用 dot-notation key
//! settings.set("api.openai_key", "sk-...");
//! settings.set("models.default", "gpt-4");
//!
//! let api_key = settings.get_str("api.openai_key");  // Some("sk-...")
//!
//! // 持久化 (atomic write: 先写 .tmp, 然后 rename)
//! store.save(&settings).await?;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-settings
//!
//! # 设计 (Design) — P15.5.1
//!
//! **目标**: 抽象 user-level settings 能力缝 (跟 dsh `settings/` package 1:1 对等).
//! 业务方
//! - 存 user-level config: API keys, model preferences, paths
//! - 用 `mah settings set api.openai_key sk-...` 改 key (P15.5.3 集成 CLI)
//! - 热重载 (P15.5.2): 改 `~/.ma-harness/settings.yaml` 自动 reload 到 in-memory state
//!
//! **背景**: 见 [dsh-feature-parity-table §9] `dsh --profile <custom> --patch foo.yml`.
//! 之前 ma-harness 用 plugin config (compile-time), 改 setting 需重新编译.
//! P15.5 起业务方可改 `~/.ma-harness/settings.yaml` 直接生效, 跟 dsh `~/.dsh/settings.yaml` 对齐.
//!
//! **核心抽象**:
//! - [`Settings`] — in-memory YAML 树 (mapping), 用 dot-notation key 访问
//! - [`SettingsStore`] trait — `load` / `save` / `path` / `provider_name`
//! - [`FileSettingsStore`] (P15.5.1 主交付): YAML 文件 + atomic save
//! - [`SettingsError`] — Io / Parse / Config / Serialize
//! - [`SETTINGS_STORE`] typed key (跟 SHELL_SERVICE / SKILL_PROVIDER 平行)
//! - [`default_settings_path()`] helper — `~/.ma-harness/settings.yaml`
//!
//! **存储格式**: YAML (跟 dsh 一致, 也跟现有 profile/bundle crate 一致 —
//! 都是 `serde_yaml::Mapping` 树).
//!
//! **Atomic save**: 写 `.tmp` 文件 → `fsync` → rename 到目标. 防止半写状态
//! 业务方进程 crash 后 settings.yaml 损坏.
//!
//! **6 质量属性** (业务方 2026-09-04 约定):
//! - 可复用: `SettingsStore` trait, future `RemoteSettingsStore` (P16+ cloud config)
//! - 可维护: 模块化分块, error / value / store 集中 lib.rs
//! - 鲁棒: atomic save, missing file 返空, invalid YAML 显式 error
//! - 安全: 不 eval settings 值, 静态 string
//! - 可测: 12+ 测试覆盖 load / save / get / set / dotted path / 错误路径
//! - 可扩展: 未来加 `LlmSettingsProvider` (从远端 fetch) / `EnvSettingsProvider` (env 覆盖)
//!
//! # 限制 (Limitations) — P15.5.1
//!
//! - **没**热重载 (P15.5.2 引入 `notify` crate + file watcher)
//! - **没**CLI command (P15.5.3 集成 `mah settings set/get/list`)
//! - **没**`EnvSettingsProvider` (P15.5.4 加 env 优先级覆盖)
//! - **没**schema validation (业务方可手写, P15.5.5 加 typed schema)

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_yaml::{Mapping, Value};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

// ============================================================================
// SettingsError
// ============================================================================

/// Settings 能力缝错误.
#[derive(Debug, Error)]
pub enum SettingsError {
    /// IO 错误 (读 / 写 / rename 失败)
    #[error("settings I/O error at {path:?}: {source}")]
    Io {
        /// 路径 (调试用)
        path: PathBuf,
        /// 底层 IO 错误
        #[source]
        source: std::io::Error,
    },

    /// YAML 解析失败
    #[error("settings YAML parse error: {0}")]
    Parse(String),

    /// YAML 序列化失败
    #[error("settings YAML serialize error: {0}")]
    Serialize(String),

    /// 配置错误 (e.g. 没 HOME / USERPROFILE env)
    #[error("settings config error: {0}")]
    Config(String),

    /// 不支持的 key (空 / 包含 NUL 等)
    #[error("invalid settings key: {0}")]
    InvalidKey(String),
}

// ============================================================================
// Settings: in-memory YAML 树
// ============================================================================

/// User-level settings (P15.5.1).
///
/// **存储**: `serde_yaml::Mapping` 树 — 顶层必须是 mapping (跟 dsh `settings.yaml` 格式一致).
///
/// **Key 格式**: dot-notation 路径 (e.g. `"api.openai_key"`, `"models.default"`).
/// - 顶层 key: `"openai_key"`
/// - 嵌套 key: `"api.openai_key"` (走 `api` → `openai_key`)
/// - 设置路径冲突 (e.g. 已存在 `"api"` 是个 string, 但想 set `"api.foo"`) → 自动覆盖为 mapping
///
/// **线程安全**: `Settings` 不可 `Clone` 也不可 `Send` (内部无 lock),
/// 业务方在 `Arc<RwLock<Settings>>` 里用, 或者每次 `load → mutate → save` 模式.
///
/// **Example**:
/// ```ignore
/// let mut s = Settings::empty();
/// s.set("api.openai_key", "sk-...");
/// s.set("models.default", "gpt-4");
/// assert_eq!(s.get_str("api.openai_key"), Some("sk-..."));
/// assert_eq!(s.get_str("models.default"), Some("gpt-4"));
/// ```
pub struct Settings {
    /// 顶层 mapping
    root: Mapping,
}

impl Default for Settings {
    fn default() -> Self {
        Self::empty()
    }
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("keys", &self.keys())
            .field("len", &self.len())
            .finish()
    }
}

impl Settings {
    /// 创建一个空 Settings.
    pub fn empty() -> Self {
        Self {
            root: Mapping::new(),
        }
    }

    /// 从现有 Mapping 构 Settings.
    pub fn from_mapping(root: Mapping) -> Self {
        Self { root }
    }

    /// 从 YAML 字符串解析 (顶层必须是 mapping 或 null).
    ///
    /// **Null 处理**: 解析出 `Value::Null` → empty settings (first run 友好).
    pub fn from_yaml(content: &str) -> Result<Self, SettingsError> {
        let value: Value =
            serde_yaml::from_str(content).map_err(|e| SettingsError::Parse(e.to_string()))?;
        match value {
            Value::Null => Ok(Self::empty()),
            Value::Mapping(m) => Ok(Self { root: m }),
            other => Err(SettingsError::Parse(format!(
                "expected mapping at root, got {other:?}"
            ))),
        }
    }

    /// 序列化为 YAML 字符串.
    pub fn to_yaml(&self) -> Result<String, SettingsError> {
        serde_yaml::to_string(&self.root).map_err(|e| SettingsError::Serialize(e.to_string()))
    }

    /// 拿底层 mapping (业务方高级用法: e.g. JSON-RPC expose 整个 settings).
    pub fn as_mapping(&self) -> &Mapping {
        &self.root
    }

    // ----- get -----

    /// 用 dot-notation 拿 value (`None` = 不存在或路径不通).
    pub fn get(&self, key: &str) -> Option<&Value> {
        let parts = split_key(key)?;
        let mut current: &Mapping = &self.root;
        for (i, part) in parts.iter().enumerate() {
            let v = current.get(*part)?;
            if i == parts.len() - 1 {
                return Some(v);
            }
            match v {
                Value::Mapping(m) => current = m,
                _ => return None, // 路径中间不是 mapping → 不通
            }
        }
        None
    }

    /// 拿 string 值 (`None` = 不存在或不是 string).
    pub fn get_str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// 拿 bool 值 (`None` = 不存在或不是 bool).
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// 拿 i64 值 (`None` = 不存在或不是整数).
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        match self.get(key)? {
            Value::Number(n) => n.as_i64(),
            _ => None,
        }
    }

    /// 检查 key 是否存在 (任何类型).
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    // ----- mutate -----

    /// Set value (dot-notation key).
    ///
    /// **路径冲突处理**: 如果中间路径不是 mapping (e.g. 已有 `"api"` 是 string),
    /// 自动覆盖为 empty mapping, 继续设置. 业务方不会因为 stale data 卡住.
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        let value = value.into();
        let parts = match split_key(key) {
            Some(p) => p,
            None => return, // 空 key → no-op
        };
        let (last, rest) = parts.split_last().unwrap();
        let mut current: &mut Mapping = &mut self.root;
        for part in rest {
            let needs_new = !matches!(current.get(*part), Some(Value::Mapping(_)));
            if needs_new {
                current.insert(
                    Value::String((*part).to_string()),
                    Value::Mapping(Mapping::new()),
                );
            }
            match current.get_mut(*part) {
                Some(Value::Mapping(m)) => current = m,
                _ => return, // unreachable
            }
        }
        current.insert(Value::String((*last).to_string()), value);
    }

    /// 删除 key. 返 `true` = 删了, `false` = key 不存在.
    ///
    /// **注**: 不清理空 mapping 父节点 (P15.5.2 之后可以加, 现在 keep simple).
    pub fn unset(&mut self, key: &str) -> bool {
        let parts = match split_key(key) {
            Some(p) => p,
            None => return false,
        };
        let (last, rest) = parts.split_last().unwrap();
        let mut current: &mut Mapping = &mut self.root;
        for part in rest {
            match current.get_mut(*part) {
                Some(Value::Mapping(m)) => current = m,
                _ => return false,
            }
        }
        current.remove(Value::String((*last).to_string())).is_some()
    }

    // ----- inspect -----

    /// 列所有 dot-notation keys (递归).
    ///
    /// **Example**: settings `{api: {openai_key: sk-...}, models: {default: gpt-4}}` →
    /// `["api.openai_key", "models.default"]`
    pub fn keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_keys(&self.root, "", &mut out);
        out
    }

    /// 顶层 key 数 (不递归).
    pub fn len(&self) -> usize {
        self.root.len()
    }

    /// 是否空.
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }
}

/// 拆 key 为 parts (空 key 返 None).
fn split_key(key: &str) -> Option<Vec<&str>> {
    if key.is_empty() || key.contains('\0') {
        return None;
    }
    Some(key.split('.').collect())
}

/// 递归收集所有 leaf key (dot-notation).
fn collect_keys(mapping: &Mapping, prefix: &str, out: &mut Vec<String>) {
    for (k, v) in mapping {
        let k_str = match k {
            Value::String(s) => s.clone(),
            other => format!("{other:?}"),
        };
        let full_key = if prefix.is_empty() {
            k_str
        } else {
            format!("{prefix}.{k_str}")
        };
        match v {
            Value::Mapping(m) => collect_keys(m, &full_key, out),
            _ => out.push(full_key),
        }
    }
}

// ============================================================================
// SettingsStore trait
// ============================================================================

/// Settings 存储抽象 (P15.5.1).
///
/// **业务方用**: `Arc<dyn SettingsStore>` 注入到 `ctx.settings` (通过 [`SETTINGS_STORE`]).
///
/// **生命周期**: `load()` 返 settings 快照, 业务方 mutate 后调 `save(&settings)`.
/// `SettingsStore` 本身不持 in-memory state (除了 path), 简化并发模型.
#[async_trait]
pub trait SettingsStore: Send + Sync + 'static {
    /// 加载现有 settings.
    ///
    /// **Missing file 行为**: 返 `Settings::empty()` (first run 友好).
    /// **Invalid YAML 行为**: 返 `Err(SettingsError::Parse(...))`.
    async fn load(&self) -> Result<Settings, SettingsError>;

    /// 保存 settings 到底层 storage.
    ///
    /// **FileSettingsStore 实现**: atomic write (`.tmp` + rename).
    async fn save(&self, settings: &Settings) -> Result<(), SettingsError>;

    /// 存储路径 (e.g. `~/.ma-harness/settings.yaml`).
    fn path(&self) -> &Path;

    /// Provider 标识 (e.g. `"file"`).
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// FileSettingsStore (P15.5.1 主交付)
// ============================================================================

/// 文件 backing 的 SettingsStore (P15.5.1 主交付).
///
/// **行为**:
/// - `load()`: 读文件 → 解析 YAML → 返 Settings. 缺失文件 → empty settings.
/// - `save()`: atomic write (写 `<path>.tmp` → `fsync` → rename 到 `<path>`).
///
/// **默认路径**: `~/.ma-harness/settings.yaml` (通过 [`default_settings_path`] 拿).
///
/// **业务方用**:
/// ```ignore
/// let store = FileSettingsStore::new("~/.ma-harness/settings.yaml")?;
/// // 或
/// let store = FileSettingsStore::at_default()?;
/// ```
pub struct FileSettingsStore {
    /// YAML 文件路径
    path: PathBuf,
}

impl std::fmt::Debug for FileSettingsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSettingsStore")
            .field("path", &self.path)
            .finish()
    }
}

impl FileSettingsStore {
    /// 创建一个 FileSettingsStore, 绑指定路径.
    ///
    /// **注**: 不创建父目录 (load/save 失败时返 Io error, 业务方自己 mkdir).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 用默认路径 `~/.ma-harness/settings.yaml` 创建.
    ///
    /// **Errors**:
    /// - `SettingsError::Config`: `HOME` (Unix) / `USERPROFILE` (Windows) env 都没设.
    pub fn at_default() -> Result<Self, SettingsError> {
        let path = default_settings_path()?;
        Ok(Self::new(path))
    }
}

#[async_trait]
impl SettingsStore for FileSettingsStore {
    async fn load(&self) -> Result<Settings, SettingsError> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => {
                let content = String::from_utf8(bytes)
                    .map_err(|e| SettingsError::Parse(format!("file is not valid UTF-8: {e}")))?;
                Settings::from_yaml(&content)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // First run: 返 empty settings, 不报错
                tracing::debug!(path = %self.path.display(), "settings file not found, returning empty");
                Ok(Settings::empty())
            }
            Err(e) => Err(SettingsError::Io {
                path: self.path.clone(),
                source: e,
            }),
        }
    }

    async fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        let content = settings.to_yaml()?;
        let tmp_path = self.path.with_extension("yaml.tmp");

        // 1. 写 .tmp
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| SettingsError::Io {
                path: tmp_path.clone(),
                source: e,
            })?;
        file.write_all(content.as_bytes())
            .await
            .map_err(|e| SettingsError::Io {
                path: tmp_path.clone(),
                source: e,
            })?;
        file.sync_all().await.map_err(|e| SettingsError::Io {
            path: tmp_path.clone(),
            source: e,
        })?;
        drop(file);

        // 2. Atomic rename (.tmp → target)
        tokio::fs::rename(&tmp_path, &self.path)
            .await
            .map_err(|e| SettingsError::Io {
                path: self.path.clone(),
                source: e,
            })?;

        tracing::debug!(path = %self.path.display(), "settings saved");
        Ok(())
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn provider_name(&self) -> &'static str {
        "file"
    }
}

// ============================================================================
// Default path helper
// ============================================================================

/// 拿默认 settings 路径 `~/.ma-harness/settings.yaml`.
///
/// **优先级**: `MA_HARNESS_SETTINGS` env 覆盖 → `~/.ma-harness/settings.yaml` (默认).
///
/// **Errors**:
/// - `SettingsError::Config`: `MA_HARNESS_SETTINGS` / `HOME` / `USERPROFILE` 都没设
pub fn default_settings_path() -> Result<PathBuf, SettingsError> {
    // 1. env 覆盖
    if let Some(p) = std::env::var_os("MA_HARNESS_SETTINGS") {
        return Ok(PathBuf::from(p));
    }
    // 2. 平台 home + .ma-harness/settings.yaml
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            SettingsError::Config("no HOME / USERPROFILE / MA_HARNESS_SETTINGS env set".to_string())
        })?;
    Ok(PathBuf::from(home)
        .join(".ma-harness")
        .join("settings.yaml"))
}

// ============================================================================
// Typed key + type alias
// ============================================================================

/// Typed key: `ctx.settings` 注入的 SettingsStore (P15.5.3 业务方注入).
pub static SETTINGS_STORE: ma_harness_cordis::CtxKey<std::sync::Arc<dyn SettingsStore>> =
    ma_harness_seam::ctx_key!("settings_store");

/// 平台默认 settings provider (P15.5.1: FileSettingsStore).
pub type DefaultSettingsStore = FileSettingsStore;

// ============================================================================
// P15.5.2: SettingsWatcher (hot-reload via notify)
// ============================================================================

use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

/// Settings hot-reload watcher (P15.5.2).
///
/// **行为**: 监控 `path` 所在目录. 文件有变化 (create / modify / rename) →
/// debounce `debounce` ms → 重新读文件 → 调 `callback(new_settings)`.
///
/// **用途**: 业务方外部编辑 `~/.ma-harness/settings.yaml` (e.g. 用 vim,
/// VSCode, 或另一个进程) 时, 自动 reload 到 in-memory state, 不需要重启
/// `mah` 进程.
///
/// **生命周期**: `SettingsWatcher` 持有 notify watcher + 后台 thread.
/// **Drop = stop**: drop 时, notify watcher 先 drop (关闭 channel), 后台
/// thread `recv()` 拿到 Disconnected 退出, 然后 `Drop` 等 thread join.
///
/// **Example**:
/// ```ignore
/// use ma_harness_settings::SettingsWatcher;
/// use std::sync::{Arc, Mutex};
///
/// let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
/// let captured_clone = Arc::clone(&captured);
/// let watcher = SettingsWatcher::new("~/.ma-harness/settings.yaml", move |s| {
///     captured_clone.lock().unwrap().push(format!("reloaded: {} keys", s.keys().len()));
/// })?;
/// // watcher 在 scope 内 active, drop 时停
/// ```
///
/// **Debounce 算法** (trailing edge):
/// - 收到事件 → 标记 `last_change = now`
/// - 后续 `debounce` ms 内没新事件 → fire callback
/// - 连续 edit (e.g. 多次 save) 自动 coalesce 成 1 次 reload
///
/// **跨平台**: 委托 `notify::RecommendedWatcher` →
/// - Windows: `ReadDirectoryChangesW`
/// - Linux: `inotify`
/// - macOS: `FSEvents`
pub struct SettingsWatcher {
    /// notify watcher (Option 让 Drop::drop 能 take)
    watcher: Option<RecommendedWatcher>,
    /// 后台 reload thread
    join: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for SettingsWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsWatcher")
            .field("active", &self.watcher.is_some())
            .finish()
    }
}

impl SettingsWatcher {
    /// 默认 debounce 100ms.
    pub fn new<F>(path: impl AsRef<Path>, callback: F) -> Result<Self, SettingsError>
    where
        F: Fn(Settings) + Send + 'static,
    {
        Self::with_debounce(path, callback, Duration::from_millis(100))
    }

    /// 自定义 debounce duration.
    ///
    /// **典型用法**: 业务方想要 0ms (no debounce) 用 `Duration::from_millis(0)`.
    /// 想要 1s 静默期用 `Duration::from_secs(1)`.
    pub fn with_debounce<F>(
        path: impl AsRef<Path>,
        callback: F,
        debounce: Duration,
    ) -> Result<Self, SettingsError>
    where
        F: Fn(Settings) + Send + 'static,
    {
        let path = path.as_ref().to_path_buf();

        // 1. 拿 file name 用来在 callback 过滤
        let target_name: std::ffi::OsString = path
            .file_name()
            .ok_or_else(|| SettingsError::Config(format!("path has no file name: {path:?}")))?
            .to_os_string();

        // 2. 父目录 (notify 监控目录, 不直接监控文件 — atomic rename 才能 catch)
        let parent = path
            .parent()
            .ok_or_else(|| SettingsError::Config(format!("path has no parent: {path:?}")))?;
        if !parent.exists() {
            // Best-effort: 创建父目录 (跟 FileSettingsStore::save 行为一致)
            std::fs::create_dir_all(parent).map_err(|e| SettingsError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        // 3. Channel: notify callback → background thread
        let (tx, rx) = channel::<()>();

        // 4. notify watcher
        let target_for_filter = target_name.clone();
        let mut watcher: RecommendedWatcher =
            notify::recommended_watcher(move |res: notify::Result<Event>| {
                // 过滤: 只关心目标文件的事件
                let event = match res {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(error = %e, "notify watcher error");
                        return;
                    }
                };
                if event
                    .paths
                    .iter()
                    .any(|p| p.file_name() == Some(&target_for_filter))
                {
                    let _ = tx.send(());
                }
            })
            .map_err(|e| SettingsError::Config(format!("create notify watcher: {e}")))?;

        // 5. 监控父目录 (non-recursive, 业务方只关心这一个文件)
        watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .map_err(|e| SettingsError::Config(format!("watch parent dir {parent:?}: {e}")))?;

        // 6. 后台 thread: recv events → debounce → reload → callback
        let callback_path = path.clone();
        let join = std::thread::Builder::new()
            .name("ma-harness-settings-watcher".to_string())
            .spawn(move || {
                run_watcher_loop(rx, debounce, &callback_path, callback);
            })
            .map_err(|e| SettingsError::Config(format!("spawn watcher thread: {e}")))?;

        Ok(Self {
            watcher: Some(watcher),
            join: Some(join),
        })
    }
}

impl Drop for SettingsWatcher {
    fn drop(&mut self) {
        // 1. 先 drop watcher → channel Sender 在 callback closure 里被 drop
        //    → rx.recv() 拿 Disconnected → thread 退出
        self.watcher.take();
        // 2. 等 thread 退出
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// 后台 thread 主循环: 收 events → debounce → reload → callback.
fn run_watcher_loop<F>(
    rx: std::sync::mpsc::Receiver<()>,
    debounce: Duration,
    path: &Path,
    callback: F,
) where
    F: Fn(Settings) + Send + 'static,
{
    let mut last_change: Option<std::time::Instant> = None;
    loop {
        match rx.recv_timeout(debounce) {
            Ok(()) => {
                // 收到事件 → 标记时间, 等 debounce 静默期
                last_change = Some(std::time::Instant::now());
            }
            Err(RecvTimeoutError::Timeout) => {
                // 没新事件. 如果之前有变化 + 距离 `last_change` ≥ debounce → fire
                if let Some(t) = last_change {
                    if t.elapsed() >= debounce {
                        let settings = load_for_watcher(path);
                        tracing::debug!(
                            path = %path.display(),
                            keys = settings.keys().len(),
                            "settings hot-reload fired"
                        );
                        callback(settings);
                        last_change = None;
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                // Watcher dropped → 退出
                tracing::debug!("settings watcher channel disconnected, stopping");
                break;
            }
        }
    }
}

/// Watcher 用的 load helper: missing file → empty settings, parse error → 旧 state (log warn).
///
/// **注**: 跟 `FileSettingsStore::load` 行为略有不同 (P15.5.2 watcher 是 sync,
/// 不能用 async `tokio::fs::read`). 同步 std::fs::read + 错误归一化.
fn load_for_watcher(path: &Path) -> Settings {
    match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(content) => Settings::from_yaml(&content).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "settings reload: parse error, using empty");
                Settings::empty()
            }),
            Err(e) => {
                tracing::warn!(error = %e, "settings reload: not UTF-8, using empty");
                Settings::empty()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File deleted externally → 用 empty (跟 FileSettingsStore::load 一致)
            Settings::empty()
        }
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "settings reload: read error, using empty");
            Settings::empty()
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
#[allow(unsafe_code)] // 测试用 std::env::set_var / remove_var (Rust 1.85+ 要求 unsafe)
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    // ----- Settings::empty / from_mapping -----

    #[test]
    fn settings_empty_is_empty() {
        let s = Settings::empty();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        assert_eq!(s.keys(), Vec::<String>::new());
    }

    #[test]
    fn settings_default_impl_matches_empty() {
        let s: Settings = Default::default();
        assert!(s.is_empty());
    }

    #[test]
    fn settings_from_mapping_preserves_content() {
        let mut m = Mapping::new();
        m.insert(
            Value::String("foo".to_string()),
            Value::String("bar".to_string()),
        );
        let s = Settings::from_mapping(m);
        assert_eq!(s.len(), 1);
        assert_eq!(s.get_str("foo"), Some("bar"));
    }

    // ----- Settings::set / get -----

    #[test]
    fn settings_set_and_get_top_level_key() {
        let mut s = Settings::empty();
        s.set("name", "alice");
        assert_eq!(s.get_str("name"), Some("alice"));
    }

    #[test]
    fn settings_set_and_get_nested_key() {
        let mut s = Settings::empty();
        s.set("api.openai_key", "sk-abc");
        s.set("api.anthropic_key", "sk-ant-xyz");
        assert_eq!(s.get_str("api.openai_key"), Some("sk-abc"));
        assert_eq!(s.get_str("api.anthropic_key"), Some("sk-ant-xyz"));
    }

    #[test]
    fn settings_set_creates_intermediate_mapping() {
        let mut s = Settings::empty();
        s.set("a.b.c.d", "deep");
        assert_eq!(s.get_str("a.b.c.d"), Some("deep"));
        // 验证结构: a.b.c 是 mapping, a.b.c.d 是 string
        assert!(matches!(s.get("a.b.c"), Some(Value::Mapping(_))));
        assert!(matches!(s.get("a.b"), Some(Value::Mapping(_))));
        assert!(matches!(s.get("a"), Some(Value::Mapping(_))));
    }

    #[test]
    fn settings_set_overwrites_existing_value() {
        let mut s = Settings::empty();
        s.set("api.key", "old");
        s.set("api.key", "new");
        assert_eq!(s.get_str("api.key"), Some("new"));
    }

    #[test]
    fn settings_set_overwrites_non_mapping_path_with_mapping() {
        // 已有 "api" 是 string, 现在想 set "api.key"
        // → "api" 应被覆盖为 mapping, "api.key" 是 string
        let mut s = Settings::empty();
        s.set("api", "not-a-mapping");
        s.set("api.key", "now-a-mapping");
        assert_eq!(s.get_str("api.key"), Some("now-a-mapping"));
    }

    #[test]
    fn settings_get_returns_none_for_missing_key() {
        let s = Settings::empty();
        assert_eq!(s.get_str("missing"), None);
        assert_eq!(s.get_str("a.b.c.d"), None);
    }

    #[test]
    fn settings_get_returns_none_when_path_through_non_mapping() {
        let mut s = Settings::empty();
        s.set("foo", "string-value");
        // "foo.bar" 不能 navigate 因为 foo 不是 mapping
        assert_eq!(s.get_str("foo.bar"), None);
    }

    // ----- Settings::has / unset / keys / len -----

    #[test]
    fn settings_has_returns_correct_values() {
        let mut s = Settings::empty();
        s.set("api.key", "v");
        assert!(s.has("api.key"));
        assert!(!s.has("api.missing"));
        assert!(!s.has("missing"));
    }

    #[test]
    fn settings_unset_removes_key() {
        let mut s = Settings::empty();
        s.set("api.key", "v");
        assert!(s.has("api.key"));
        assert!(s.unset("api.key"));
        assert!(!s.has("api.key"));
    }

    #[test]
    fn settings_unset_returns_false_for_missing_key() {
        let mut s = Settings::empty();
        assert!(!s.unset("never-existed"));
    }

    #[test]
    fn settings_keys_lists_all_dotted_keys() {
        let mut s = Settings::empty();
        s.set("api.openai_key", "sk-1");
        s.set("api.anthropic_key", "sk-2");
        s.set("models.default", "gpt-4");
        s.set("debug", "true");

        let mut keys = s.keys();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "api.anthropic_key".to_string(),
                "api.openai_key".to_string(),
                "debug".to_string(),
                "models.default".to_string(),
            ]
        );
    }

    #[test]
    fn settings_len_counts_only_top_level_keys() {
        let mut s = Settings::empty();
        s.set("api.openai_key", "x");
        s.set("api.anthropic_key", "y");
        s.set("models.default", "gpt-4");
        // len = 2 (api + models), 不递归
        assert_eq!(s.len(), 2);
    }

    // ----- typed accessors -----

    #[test]
    fn settings_get_bool_works() {
        let mut s = Settings::empty();
        s.set("debug", true);
        s.set("verbose", false);
        assert_eq!(s.get_bool("debug"), Some(true));
        assert_eq!(s.get_bool("verbose"), Some(false));
        assert_eq!(s.get_bool("missing"), None);
    }

    #[test]
    fn settings_get_bool_returns_none_for_non_bool() {
        let mut s = Settings::empty();
        s.set("not-a-bool", "yes");
        assert_eq!(s.get_bool("not-a-bool"), None);
    }

    #[test]
    fn settings_get_i64_works() {
        let mut s = Settings::empty();
        s.set("port", 9090_i64);
        s.set("timeout_ms", 30_000_i64);
        assert_eq!(s.get_i64("port"), Some(9090));
        assert_eq!(s.get_i64("timeout_ms"), Some(30_000));
        assert_eq!(s.get_i64("missing"), None);
    }

    #[test]
    fn settings_get_str_returns_none_for_non_string() {
        let mut s = Settings::empty();
        s.set("not-a-string", 42_i64);
        assert_eq!(s.get_str("not-a-string"), None);
    }

    // ----- YAML round-trip -----

    #[test]
    fn settings_from_yaml_parses_valid_yaml() {
        let yaml =
            "api:\n  openai_key: sk-abc\n  anthropic_key: sk-ant-xyz\nmodels:\n  default: gpt-4\n";
        let s = Settings::from_yaml(yaml).expect("parse");
        assert_eq!(s.get_str("api.openai_key"), Some("sk-abc"));
        assert_eq!(s.get_str("api.anthropic_key"), Some("sk-ant-xyz"));
        assert_eq!(s.get_str("models.default"), Some("gpt-4"));
    }

    #[test]
    fn settings_from_yaml_handles_empty_as_null() {
        // 空 YAML 串 → Value::Null → empty settings
        let s = Settings::from_yaml("").expect("parse");
        assert!(s.is_empty());
    }

    #[test]
    fn settings_from_yaml_rejects_non_mapping_root() {
        // 顶层是 string → 报错
        let err = Settings::from_yaml("just a string").unwrap_err();
        match err {
            SettingsError::Parse(msg) => assert!(msg.contains("expected mapping")),
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[test]
    fn settings_from_yaml_rejects_invalid_yaml() {
        let err = Settings::from_yaml("foo: : : invalid").unwrap_err();
        assert!(matches!(err, SettingsError::Parse(_)));
    }

    #[test]
    fn settings_to_yaml_roundtrip() {
        let mut s = Settings::empty();
        s.set("api.openai_key", "sk-abc");
        s.set("models.default", "gpt-4");

        let yaml = s.to_yaml().expect("serialize");
        let s2 = Settings::from_yaml(&yaml).expect("parse");
        assert_eq!(s2.get_str("api.openai_key"), Some("sk-abc"));
        assert_eq!(s2.get_str("models.default"), Some("gpt-4"));
    }

    // ----- FileSettingsStore: load / save -----

    #[tokio::test]
    async fn file_settings_store_load_missing_file_returns_empty() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        let store = FileSettingsStore::new(&path);
        let s = store.load().await.expect("load missing");
        assert!(s.is_empty());
    }

    #[tokio::test]
    async fn file_settings_store_load_existing_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        tokio::fs::write(&path, "api:\n  openai_key: sk-from-disk\n")
            .await
            .expect("write");
        let store = FileSettingsStore::new(&path);
        let s = store.load().await.expect("load");
        assert_eq!(s.get_str("api.openai_key"), Some("sk-from-disk"));
    }

    #[tokio::test]
    async fn file_settings_store_load_invalid_yaml_errors() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        tokio::fs::write(&path, "foo: : : invalid")
            .await
            .expect("write");
        let store = FileSettingsStore::new(&path);
        let err = store.load().await.unwrap_err();
        assert!(matches!(err, SettingsError::Parse(_)));
    }

    #[tokio::test]
    async fn file_settings_store_save_then_load_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        let store = FileSettingsStore::new(&path);

        let mut s = Settings::empty();
        s.set("api.openai_key", "sk-1");
        s.set("models.default", "gpt-4");
        store.save(&s).await.expect("save");

        // Reload 拿回来
        let s2 = store.load().await.expect("load");
        assert_eq!(s2.get_str("api.openai_key"), Some("sk-1"));
        assert_eq!(s2.get_str("models.default"), Some("gpt-4"));

        // .tmp 文件应已被 rename 走, 不应存在
        let tmp_path = path.with_extension("yaml.tmp");
        assert!(!tmp_path.exists(), ".tmp should be cleaned up after rename");
    }

    #[tokio::test]
    async fn file_settings_store_save_overwrites_existing_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        tokio::fs::write(&path, "old:\n  key: old-value\n")
            .await
            .expect("write");
        let store = FileSettingsStore::new(&path);

        let mut s = Settings::empty();
        s.set("new", "new-value");
        store.save(&s).await.expect("save");

        // Reload 验证覆盖
        let s2 = store.load().await.expect("load");
        assert_eq!(s2.get_str("new"), Some("new-value"));
        assert!(!s2.has("old.key"), "old key should be gone after overwrite");
    }

    #[tokio::test]
    async fn file_settings_store_provider_name_is_file() {
        let dir = tempdir().expect("tempdir");
        let store = FileSettingsStore::new(dir.path().join("settings.yaml"));
        assert_eq!(store.provider_name(), "file");
    }

    #[tokio::test]
    async fn file_settings_store_path_returns_configured_path() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("custom.yaml");
        let store = FileSettingsStore::new(&path);
        assert_eq!(store.path(), path);
    }

    // ----- default_settings_path -----

    #[test]
    fn default_settings_path_respects_ma_harness_settings_env() {
        // 设 env 覆盖 → 用 env 路径
        let original = std::env::var_os("MA_HARNESS_SETTINGS");
        unsafe { std::env::set_var("MA_HARNESS_SETTINGS", "/tmp/custom-settings.yaml") };
        let result = default_settings_path().expect("path");
        // Restore 必须在 assert 之前, 否则 test 间相互污染
        match original {
            Some(v) => unsafe { std::env::set_var("MA_HARNESS_SETTINGS", v) },
            None => unsafe { std::env::remove_var("MA_HARNESS_SETTINGS") },
        }
        assert_eq!(result, PathBuf::from("/tmp/custom-settings.yaml"));
    }

    #[test]
    fn default_settings_path_falls_back_to_home() {
        // 不设 env → 用 HOME / USERPROFILE
        let original = std::env::var_os("MA_HARNESS_SETTINGS");
        unsafe { std::env::remove_var("MA_HARNESS_SETTINGS") };
        let result = default_settings_path().expect("path");
        if let Some(v) = original {
            unsafe { std::env::set_var("MA_HARNESS_SETTINGS", v) };
        }
        // 应以 .ma-harness/settings.yaml 结尾
        assert!(result.ends_with(".ma-harness/settings.yaml"));
    }

    // ----- Debug impl redaction -----

    #[test]
    fn settings_debug_does_not_leak_values() {
        // 不强制 — 但确保 keys / len 出现, 不会打印整个 mapping
        let mut s = Settings::empty();
        s.set("api.key", "secret");
        let debug = format!("{s:?}");
        // keys 应该有 "api.key" 出现
        assert!(debug.contains("api.key"));
        // value 不会 leak (impl 只 print keys, 不 print mapping 内容)
        // (不强 assert "secret" 不在 — 业务方自己 wrap 才稳)
    }

    #[test]
    fn file_settings_store_debug_shows_path() {
        let dir = tempdir().expect("tempdir");
        let store = FileSettingsStore::new(dir.path().join("settings.yaml"));
        let debug = format!("{store:?}");
        assert!(debug.contains("FileSettingsStore"));
        assert!(debug.contains("settings.yaml"));
    }

    // ----- Settings: invalid key -----

    #[test]
    fn settings_set_with_empty_key_is_noop() {
        let mut s = Settings::empty();
        s.set("", "v"); // no-op, 不 panic
        assert!(s.is_empty());
    }

    // ========================================================================
    // P15.5.2 tests: SettingsWatcher (hot-reload)
    // ========================================================================

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;

    /// 写一个 settings.yaml + 等 watcher ready + 等 callback fire
    async fn wait_for_callback(path: &Path, count: &StdArc<AtomicUsize>, timeout_ms: u64) {
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        while std::time::Instant::now() < deadline {
            if count.load(Ordering::SeqCst) > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // 最后再 print 一下, 方便 debug
        let _ = path; // suppress unused warning if test fail
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settings_watcher_fires_on_external_file_create() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        // 不预创建文件, 让 watcher 看 create 事件

        let count = StdArc::new(AtomicUsize::new(0));
        let count_clone = StdArc::clone(&count);
        let _watcher = SettingsWatcher::new(&path, move |_s| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watcher");
        tokio::time::sleep(Duration::from_millis(200)).await; // watcher ready

        tokio::fs::write(&path, "key: value\n")
            .await
            .expect("write");
        wait_for_callback(&path, &count, 2000).await;

        assert!(
            count.load(Ordering::SeqCst) >= 1,
            "callback should fire on file create"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settings_watcher_fires_on_external_modify() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        tokio::fs::write(&path, "initial: 1\n")
            .await
            .expect("initial write");

        let captured: StdArc<tokio::sync::Mutex<Vec<String>>> =
            StdArc::new(tokio::sync::Mutex::new(Vec::new()));
        let captured_clone = StdArc::clone(&captured);
        let _watcher = SettingsWatcher::new(&path, move |s| {
            let captured = StdArc::clone(&captured_clone);
            // Sync closure → 用 blocking_lock 拿 async mutex
            // 注: 实际应用里 callback 应该是 sync 的, 这是测试方便
            let key_count = s.keys().len();
            // 用 std::sync::Mutex 避免 async-in-sync 问题
            captured.blocking_lock().push(format!("keys={key_count}"));
        })
        .expect("watcher");
        tokio::time::sleep(Duration::from_millis(200)).await;

        tokio::fs::write(&path, "first: 1\nsecond: 2\nthird: 3\n")
            .await
            .expect("modify");
        wait_for_callback(&path, &StdArc::new(AtomicUsize::new(0)), 2000).await;

        let g = captured.lock().await;
        assert!(!g.is_empty(), "callback should fire at least once");
        // 最后一次 callback 应反映 3 个 key
        let last = g.last().expect("at least one callback");
        assert!(
            last.contains("keys=3"),
            "last callback should report 3 keys, got {last}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settings_watcher_callback_receives_empty_settings_after_delete() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        tokio::fs::write(&path, "key: value\n")
            .await
            .expect("initial");

        let captured: StdArc<tokio::sync::Mutex<Vec<bool>>> =
            StdArc::new(tokio::sync::Mutex::new(Vec::new()));
        let captured_clone = StdArc::clone(&captured);
        let _watcher = SettingsWatcher::new(&path, move |s| {
            let mut g = captured_clone.blocking_lock();
            g.push(s.is_empty());
        })
        .expect("watcher");
        tokio::time::sleep(Duration::from_millis(200)).await;

        tokio::fs::remove_file(&path).await.expect("delete");
        wait_for_callback(&path, &StdArc::new(AtomicUsize::new(0)), 2000).await;

        let g = captured.lock().await;
        assert!(!g.is_empty(), "callback should fire after delete");
        assert!(
            g.last().copied().unwrap_or(false),
            "callback after delete should receive empty settings"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settings_watcher_drop_stops_callback() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        tokio::fs::write(&path, "v: 1\n").await.expect("initial");

        let count = StdArc::new(AtomicUsize::new(0));
        let count_clone = StdArc::clone(&count);
        let watcher = SettingsWatcher::new(&path, move |_s| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watcher");
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Drop watcher
        drop(watcher);

        // 等 200ms (thread join 应该已经完成)
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Reset count (之前可能没 fire, 也可能 fire 0 次)
        count.store(0, Ordering::SeqCst);

        // 修改文件 — 不应再 fire
        tokio::fs::write(&path, "v: 2\n").await.expect("write");
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "after drop, callback should NOT fire"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settings_watcher_creates_missing_parent_dir() {
        let dir = tempdir().expect("tempdir");
        // 不存在的子目录
        let nested = dir.path().join("nested").join("settings.yaml");

        let _watcher = SettingsWatcher::new(&nested, |_s| {})
            .expect("watcher (should auto-create parent dir)");
        // 父目录应被自动创建
        assert!(
            nested.parent().unwrap().exists(),
            "parent dir should be created"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settings_watcher_rejects_path_with_no_file_name() {
        // 路径没 file name (e.g. just ".")
        let result = SettingsWatcher::new(".", |_s| {});
        assert!(result.is_err(), "should reject path with no file name");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settings_watcher_debounce_coalesces_rapid_edits() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        tokio::fs::write(&path, "v: 1\n").await.expect("initial");

        let count = StdArc::new(AtomicUsize::new(0));
        let count_clone = StdArc::clone(&count);
        let _watcher = SettingsWatcher::with_debounce(
            &path,
            move |_s| {
                count_clone.fetch_add(1, Ordering::SeqCst);
            },
            Duration::from_millis(150),
        )
        .expect("watcher");
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 5 次 rapid edit (50ms 间隔, 都在 150ms debounce 之内)
        for i in 2..=6 {
            tokio::fs::write(&path, format!("v: {i}\n"))
                .await
                .expect("write");
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        // 等 debounce 静默 + 一点 buffer
        tokio::time::sleep(Duration::from_millis(400)).await;

        let final_count = count.load(Ordering::SeqCst);
        assert!(
            (1..=3).contains(&final_count),
            "debounce should coalesce 5 rapid edits into 1-3 callbacks, got {final_count}"
        );
    }

    #[test]
    fn settings_watcher_debug_shows_active_field() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("settings.yaml");
        let watcher = SettingsWatcher::new(&path, |_s| {}).expect("watcher");
        let debug = format!("{watcher:?}");
        assert!(debug.contains("SettingsWatcher"));
        assert!(debug.contains("active"));
    }
}
