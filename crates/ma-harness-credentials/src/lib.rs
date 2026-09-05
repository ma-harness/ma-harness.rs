//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-credentials`
//! **Crate ident** (`use` 路径): `ma_harness_credentials`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident,
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-credentials = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_credentials::{DotenvCredentialsStore, EnvCredentialsStore, LayeredCredentialsStore, CredentialsStore};
//!
//! // 业务方: env 优先, 退回 dotenv
//! let layered = LayeredCredentialsStore::new(vec![
//!     std::sync::Arc::new(EnvCredentialsStore::ma_harness()),
//!     std::sync::Arc::new(DotenvCredentialsStore::at_default()?),
//! ]);
//! let cred = layered.get("openai_key").await?;
//! println!("got credential: {}", cred.name);
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-credentials
//!
//! # 设计 (Design) — P15.5.6
//!
//! **目标**: 抽象 `ctx.credentials` 能力缝 (跟 dsh `~/.dsh/.env` 1:1 对等).
//! 业务方
//! - 存 secret (api_key, db password, oauth token) 跟 settings 分离
//! - env 优先 + .env file fallback (跟 12-factor app 一致)
//! - 未来加 OS keyring (macOS Keychain / Windows Credential Vault / Linux Secret Service)
//!
//! **背景**: 见 [dsh-feature-parity-table §9] (settings-management + secret-management).
//! 之前 P15.5.1 settings crate 装普通 config (model pref / paths),
//! P15.5.6 credentials 专装 secret. 业务方不应该把 API key 放 `settings.yaml`
//! (可见 / 易泄漏), 应该用 env 或 OS keyring.
//!
//! **核心抽象**:
//! - [`Credential`] struct (name + value)
//! - [`CredentialsStore`] trait (`get` / `list` / `provider_name`)
//! - [`EnvCredentialsStore`] (P15.5.6 主交付): 读 `MA_HARNESS_SECRET_<NAME>` env vars
//! - [`DotenvCredentialsStore`] (P15.5.6): 读 `~/.ma-harness/.env` (key=value 文件)
//! - [`LayeredCredentialsStore`]: 多层组合, last = highest priority
//! - [`default_credentials_path()`][]: `~/.ma-harness/.env` (env override supported)
//! - [`CREDENTIALS_STORE`] typed key (跟 SETTINGS_STORE 平行)
//!
//! **跟 Settings 的差异**:
//! - Settings: 公开 config, YAML file 是 OK 的 (gitignored 就行)
//! - Credentials: secret 优先 env / OS keyring, .env file 是 fallback
//! - 未来 P17 加 `KeyringCredentialsStore` (OS keyring, 强加密)
//!
//! **6 质量属性** (业务方 2026-09-04 约定):
//! - 可复用: trait 抽象, future `KeyringCredentialsStore` 可插
//! - 可维护: 模块化分块, error / credential / store 集中 lib.rs
//! - 鲁棒: missing file 返 NotFound (不 panic), bad .env line skip (继续 parse)
//! - 安全: 不 log secret value, Debug impl 只显示 name 跟 provider, 0-length secret warn
//! - 可测: 单元测试用 tempfile / env var mock, 不依赖真 keyring
//! - 可扩展: trait 抽象, future keyring / vault provider 可直接接
//!
//! # 限制 (Limitations) — P15.5.6
//!
//! - **没**OS keyring 支持 (P17+)
//! - **没**encrypted file backend (P15.5.6.1+)
//! - **没**`mah credentials set` CLI (P15.5.6.2+)
//! - **没**secret rotation / TTL (P15.5.6.3+)

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

// ============================================================================
// CredentialsError
// ============================================================================

/// Credentials capability error.
#[derive(Debug, Error)]
pub enum CredentialsError {
    /// IO 错误 (read / write / parse .env file)
    #[error("credentials I/O error at {path:?}: {source}")]
    Io {
        /// Path (debug)
        path: PathBuf,
        /// 底层 IO 错误
        #[source]
        source: std::io::Error,
    },

    /// .env 解析错误
    #[error("credentials parse error: {0}")]
    Parse(String),

    /// Config 错误 (e.g. no HOME / USERPROFILE env)
    #[error("credentials config error: {0}")]
    Config(String),

    /// Secret 不存在 (key not in any layer)
    #[error("credential not found: {0}")]
    NotFound(String),
}

// ============================================================================
// Credential
// ============================================================================

/// A single credential (P15.5.6).
///
/// **业务方**: 拿到一个 credential 后, value 应该尽快消费 (e.g. 配 LLM client),
/// 不应该长期存到内存或文件 (安全风险).
#[derive(Debug, Clone)]
pub struct Credential {
    /// Credential name (e.g. "openai_key")
    pub name: String,
    /// Credential value (the secret itself)
    pub value: String,
}

impl std::fmt::Display for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't print value (security: secret leak risk)
        write!(
            f,
            "Credential({}: <redacted, {} bytes>)",
            self.name,
            self.value.len()
        )
    }
}

// ============================================================================
// CredentialsStore trait
// ============================================================================

/// Credentials store abstraction (P15.5.6).
///
/// **生命周期**: `get()` 拿一个 credential 快照, 业务方 mutate 后调
/// 任何使用 (e.g. inject 到 LLM client). credentials 不是 hot-reload 类型
/// (跟 settings 不同, secret 变了需要重启 agent loop).
#[async_trait]
pub trait CredentialsStore: Send + Sync + 'static {
    /// 拿一个 credential by name.
    ///
    /// **Errors**:
    /// - `CredentialsError::NotFound` — key 不在 store 里
    /// - `CredentialsError::Io` — 底层 IO 失败 (.env file 不可读等)
    async fn get(&self, name: &str) -> Result<Credential, CredentialsError>;

    /// 列出所有 known credential names (without values, 安全).
    ///
    /// **用途**: 业务方 introspection / admin CLI (`mah credentials list`).
    async fn list(&self) -> Result<Vec<String>, CredentialsError>;

    /// Provider 标识 (`"env"` / `"dotenv"` / `"keyring"` / `"layered"`).
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// EnvCredentialsStore (P15.5.6 主交付)
// ============================================================================

/// Env-var based credentials store (P15.5.6).
///
/// **行为**: 读 `MA_HARNESS_SECRET_<NAME>` env vars. 业务方设
/// `MA_HARNESS_SECRET_OPENAI_KEY=sk-...`, store 自动 get("openai_key") 拿到.
///
/// **No write**: env 是只读 at runtime. 业务方用 .env file 或 OS keyring 持久化.
///
/// **Example**:
/// ```bash
/// export MA_HARNESS_SECRET_OPENAI_KEY=sk-prod-key
/// mah run "task"
/// ```
pub struct EnvCredentialsStore {
    /// Env var prefix (e.g. `"MA_HARNESS_SECRET_"`)
    prefix: String,
}

impl std::fmt::Debug for EnvCredentialsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvCredentialsStore")
            .field("prefix", &self.prefix)
            .finish()
    }
}

impl EnvCredentialsStore {
    /// 创建一个 EnvCredentialsStore with custom prefix.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// Default prefix `MA_HARNESS_SECRET_` (跟其它 settings 路径风格一致).
    pub fn ma_harness() -> Self {
        Self::new("MA_HARNESS_SECRET_")
    }
}

#[async_trait]
impl CredentialsStore for EnvCredentialsStore {
    async fn get(&self, name: &str) -> Result<Credential, CredentialsError> {
        let env_name = format!("{}{}", self.prefix, name.to_uppercase());
        match std::env::var(&env_name) {
            Ok(value) => {
                if value.is_empty() {
                    tracing::warn!(
                        name = %name,
                        "env credential is empty (likely misconfiguration)"
                    );
                }
                Ok(Credential {
                    name: name.to_string(),
                    value,
                })
            }
            Err(_) => Err(CredentialsError::NotFound(name.to_string())),
        }
    }

    async fn list(&self) -> Result<Vec<String>, CredentialsError> {
        let mut names = Vec::new();
        for (key, _) in std::env::vars() {
            if let Some(stripped) = key.strip_prefix(&self.prefix) {
                names.push(stripped.to_lowercase());
            }
        }
        names.sort();
        Ok(names)
    }

    fn provider_name(&self) -> &'static str {
        "env"
    }
}

// ============================================================================
// DotenvCredentialsStore (P15.5.6 主交付)
// ============================================================================

/// .env file based credentials store (P15.5.6).
///
/// **格式**: 每行 `KEY=value`, 支持:
/// - `#` 开头是注释 (skip)
/// - 空行 (skip)
/// - `KEY="value with spaces"` 双引号包 value (strip outer quotes)
/// - `KEY=value` 无引号 (trim whitespace)
///
/// **Example .env**:
/// ```text
/// # ~/.ma-harness/.env
/// OPENAI_KEY=sk-prod-123
/// GITHUB_TOKEN="ghp_abc def"
/// ```
pub struct DotenvCredentialsStore {
    /// .env file path
    path: PathBuf,
}

impl std::fmt::Debug for DotenvCredentialsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DotenvCredentialsStore")
            .field("path", &self.path)
            .finish()
    }
}

impl DotenvCredentialsStore {
    /// 创建一个 DotenvCredentialsStore bound to a specific path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Default path: `~/.ma-harness/.env` (跟其它 settings 路径风格一致).
    pub fn at_default() -> Result<Self, CredentialsError> {
        let path = default_credentials_path()?;
        Ok(Self::new(path))
    }
}

/// Parse a single line of a .env file. Returns (key, value) or None if line is
/// empty / comment / malformed.
///
/// **Public for testability** — business logic isn't on the trait, but the
/// parser is pure and testable.
pub fn parse_dotenv_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (k, v) = trimmed.split_once('=')?;
    let key = k.trim().to_string();
    if key.is_empty() {
        return None;
    }
    let value = v.trim();
    // Strip outer quotes (single or double)
    let value = if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
        || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    };
    Some((key, value))
}

#[async_trait]
impl CredentialsStore for DotenvCredentialsStore {
    async fn get(&self, name: &str) -> Result<Credential, CredentialsError> {
        let content = tokio::fs::read_to_string(&self.path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CredentialsError::NotFound(name.to_string())
            } else {
                CredentialsError::Io {
                    path: self.path.clone(),
                    source: e,
                }
            }
        })?;
        // 业务方传 "openai_key" 跟 .env 的 "OPENAI_KEY" 都 OK (uppercase canonical)
        let name_upper = name.to_uppercase();
        for line in content.lines() {
            if let Some((k, v)) = parse_dotenv_line(line) {
                if k == name_upper {
                    return Ok(Credential {
                        name: name.to_string(),
                        value: v,
                    });
                }
            }
        }
        Err(CredentialsError::NotFound(name.to_string()))
    }

    async fn list(&self) -> Result<Vec<String>, CredentialsError> {
        let content = match tokio::fs::read_to_string(&self.path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(CredentialsError::Io {
                    path: self.path.clone(),
                    source: e,
                });
            }
        };
        let mut names = Vec::new();
        for line in content.lines() {
            if let Some((k, _)) = parse_dotenv_line(line) {
                names.push(k);
            }
        }
        names.sort();
        Ok(names)
    }

    fn provider_name(&self) -> &'static str {
        "dotenv"
    }
}

// ============================================================================
// LayeredCredentialsStore (P15.5.6 组合层)
// ============================================================================

/// 多层 CredentialsStore 组合 (P15.5.6).
///
/// **行为**: `get(name)` 遍历 layers, 第一个 found 的 layer 胜出.
/// 业务方组合: `[EnvCredentialsStore, DotenvCredentialsStore]` 让 env 优先,
/// .env file 退到 fallback.
///
/// **Save / set**: 不支持 (P15.5.6 minimal). 业务方要存 secret, 用
/// .env file (write out-of-band) 或 OS keyring (P17+).
pub struct LayeredCredentialsStore {
    /// Layer 列表: 索引 0 = 最低优先级, 最后一个 = 最高优先级.
    layers: Vec<Arc<dyn CredentialsStore>>,
}

impl std::fmt::Debug for LayeredCredentialsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.layers.iter().map(|l| l.provider_name()).collect();
        f.debug_struct("LayeredCredentialsStore")
            .field("layers", &names)
            .field("count", &self.layers.len())
            .finish()
    }
}

impl LayeredCredentialsStore {
    /// 创建一个 LayeredCredentialsStore, 绑 layer 列表.
    pub fn new(layers: Vec<Arc<dyn CredentialsStore>>) -> Self {
        Self { layers }
    }
}

#[async_trait]
impl CredentialsStore for LayeredCredentialsStore {
    async fn get(&self, name: &str) -> Result<Credential, CredentialsError> {
        // 顺序: 0 = 最低, last = 最高
        // first-found 胜出 (因为 earlier layers 是 fallback, 后面是 override)
        // Wait: 跟 settings 相反 — credentials 是 first-found (env 优先 → 找 env 找到就停)
        // vs settings 是 last-wins (env 覆盖 file → 找 file, 找 env override)
        // 原因: secret 不需要 merge, 单一来源
        let mut last_err = CredentialsError::NotFound(name.to_string());
        for layer in &self.layers {
            match layer.get(name).await {
                Ok(cred) => return Ok(cred),
                Err(CredentialsError::NotFound(_)) => continue,
                Err(e) => {
                    tracing::warn!(
                        layer = layer.provider_name(),
                        error = %e,
                        "LayeredCredentialsStore: layer get failed, continuing"
                    );
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    async fn list(&self) -> Result<Vec<String>, CredentialsError> {
        // 合并所有 layer 的 names (去重)
        let mut all = std::collections::BTreeSet::new();
        for layer in &self.layers {
            for name in layer.list().await? {
                all.insert(name);
            }
        }
        Ok(all.into_iter().collect())
    }

    fn provider_name(&self) -> &'static str {
        "layered"
    }
}

// ============================================================================
// Default path helper
// ============================================================================

/// Default credentials path: `~/.ma-harness/.env`.
///
/// **优先级**: `MA_HARNESS_CREDENTIALS` env override → `~/.ma-harness/.env`.
///
/// **Errors**:
/// - `CredentialsError::Config` — HOME / USERPROFILE / MA_HARNESS_CREDENTIALS 都没设
pub fn default_credentials_path() -> Result<PathBuf, CredentialsError> {
    if let Some(p) = std::env::var_os("MA_HARNESS_CREDENTIALS") {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            CredentialsError::Config(
                "no HOME / USERPROFILE / MA_HARNESS_CREDENTIALS env set".to_string(),
            )
        })?;
    Ok(PathBuf::from(home).join(".ma-harness").join(".env"))
}

// ============================================================================
// Typed key + type alias
// ============================================================================

/// Typed key: `ctx.credentials` injected CredentialsStore (P15.5.6 业务方注入).
pub static CREDENTIALS_STORE: ma_harness_cordis::CtxKey<Arc<dyn CredentialsStore>> =
    ma_harness_seam::ctx_key!("credentials_store");

/// 平台默认 credentials provider (P15.5.6: LayeredCredentialsStore with
/// env + dotenv layers; 业务方可以 wrap 自己喜欢的组合).
pub type DefaultCredentialsStore = LayeredCredentialsStore;

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ----- Credential -----

    #[test]
    fn credential_display_redacts_value() {
        let cred = Credential {
            name: "openai_key".to_string(),
            value: "sk-very-secret".to_string(),
        };
        let s = format!("{cred}");
        assert!(s.contains("openai_key"));
        assert!(!s.contains("sk-very-secret"), "Display leaked value: {s}");
        assert!(s.contains("redacted"));
        assert!(s.contains("14 bytes"), "should report length");
    }

    #[test]
    fn credential_clone_preserves_value() {
        let c1 = Credential {
            name: "x".to_string(),
            value: "y".to_string(),
        };
        let c2 = c1.clone();
        assert_eq!(c1.name, c2.name);
        assert_eq!(c1.value, c2.value);
    }

    // ----- parse_dotenv_line -----

    #[test]
    fn parse_dotenv_line_handles_basic_key_equals_value() {
        assert_eq!(
            parse_dotenv_line("FOO=bar"),
            Some(("FOO".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn parse_dotenv_line_handles_whitespace() {
        assert_eq!(
            parse_dotenv_line("  FOO  =  bar  "),
            Some(("FOO".to_string(), "bar".to_string()))
        );
    }

    #[test]
    fn parse_dotenv_line_handles_double_quotes() {
        assert_eq!(
            parse_dotenv_line("FOO=\"bar baz\""),
            Some(("FOO".to_string(), "bar baz".to_string()))
        );
    }

    #[test]
    fn parse_dotenv_line_handles_single_quotes() {
        assert_eq!(
            parse_dotenv_line("FOO='bar baz'"),
            Some(("FOO".to_string(), "bar baz".to_string()))
        );
    }

    #[test]
    fn parse_dotenv_line_skips_comments_and_blanks() {
        assert_eq!(parse_dotenv_line("# this is a comment"), None);
        assert_eq!(parse_dotenv_line(""), None);
        assert_eq!(parse_dotenv_line("   "), None);
    }

    #[test]
    fn parse_dotenv_line_rejects_malformed() {
        assert_eq!(parse_dotenv_line("NO_EQUALS_SIGN"), None);
        assert_eq!(parse_dotenv_line("=value_only"), None); // empty key
    }

    // ----- EnvCredentialsStore -----

    #[test]
    fn env_credentials_store_get_reads_uppercase_env() {
        // 用 unique prefix 避免跟其它 env test race
        with_env(
            "MA_HARNESS_TEST_CRED_GET_OPENAI_KEY",
            Some("sk-env-test"),
            || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let store = EnvCredentialsStore::new("MA_HARNESS_TEST_CRED_GET_");
                let cred = rt.block_on(store.get("openai_key")).expect("get");
                assert_eq!(cred.name, "openai_key");
                assert_eq!(cred.value, "sk-env-test");
            },
        );
    }

    #[test]
    fn env_credentials_store_get_not_found() {
        with_env("MA_HARNESS_TEST_CRED_NOTFOUND_KEY", None, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let store = EnvCredentialsStore::new("MA_HARNESS_TEST_CRED_NOTFOUND_");
            let err = rt.block_on(store.get("missing_key")).unwrap_err();
            assert!(matches!(err, CredentialsError::NotFound(_)));
        });
    }

    #[test]
    fn env_credentials_store_get_empty_warns_but_returns() {
        with_env("MA_HARNESS_TEST_CRED_EMPTY_KEY", Some(""), || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let store = EnvCredentialsStore::new("MA_HARNESS_TEST_CRED_EMPTY_");
            let cred = rt.block_on(store.get("key")).expect("get");
            assert_eq!(cred.value, "");
        });
    }

    #[test]
    fn env_credentials_store_list_returns_lowercase_names() {
        with_env("MA_HARNESS_TEST_CRED_LIST_ALPHA", Some("x"), || {
            with_env("MA_HARNESS_TEST_CRED_LIST_BETA", Some("y"), || {
                // 第三个 var 用 NON-MATCHING prefix, 不应被 list 捕获
                with_env("OTHER_PREFIX_IGNORE_ME", Some("z"), || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    let store = EnvCredentialsStore::new("MA_HARNESS_TEST_CRED_LIST_");
                    let mut names = rt.block_on(store.list()).expect("list");
                    names.sort();
                    assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
                });
            });
        });
    }

    #[test]
    fn env_credentials_store_provider_name() {
        assert_eq!(EnvCredentialsStore::ma_harness().provider_name(), "env");
        assert_eq!(
            EnvCredentialsStore::new("MY_SECRET_").provider_name(),
            "env"
        );
    }

    // ----- DotenvCredentialsStore -----

    #[tokio::test]
    async fn dotenv_credentials_store_get_reads_value() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(
            &path,
            "# comment\nOPENAI_KEY=sk-test\nGITHUB_TOKEN=\"ghp abc\"\n",
        )
        .expect("write");
        let store = DotenvCredentialsStore::new(&path);
        let cred = store.get("OPENAI_KEY").await.expect("get");
        assert_eq!(cred.name, "OPENAI_KEY");
        assert_eq!(cred.value, "sk-test");
    }

    #[tokio::test]
    async fn dotenv_credentials_store_get_strips_quotes() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(&path, "TOKEN=\"with spaces\"\n").expect("write");
        let store = DotenvCredentialsStore::new(&path);
        let cred = store.get("TOKEN").await.expect("get");
        assert_eq!(cred.value, "with spaces");
    }

    #[tokio::test]
    async fn dotenv_credentials_store_get_missing_file_returns_not_found() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.env");
        let store = DotenvCredentialsStore::new(&path);
        let err = store.get("OPENAI_KEY").await.unwrap_err();
        assert!(matches!(err, CredentialsError::NotFound(_)));
    }

    #[tokio::test]
    async fn dotenv_credentials_store_get_key_not_in_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(&path, "OTHER_KEY=value\n").expect("write");
        let store = DotenvCredentialsStore::new(&path);
        let err = store.get("MISSING_KEY").await.unwrap_err();
        assert!(matches!(err, CredentialsError::NotFound(_)));
    }

    #[tokio::test]
    async fn dotenv_credentials_store_skips_comments_and_blanks() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(
            &path,
            "\n# leading comment\n\nKEY1=v1\n\n# mid comment\nKEY2=v2\n",
        )
        .expect("write");
        let store = DotenvCredentialsStore::new(&path);
        assert_eq!(store.get("KEY1").await.unwrap().value, "v1");
        assert_eq!(store.get("KEY2").await.unwrap().value, "v2");
    }

    #[tokio::test]
    async fn dotenv_credentials_store_list_returns_all_keys() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(&path, "ALPHA=1\nBETA=2\nGAMMA=3\n# skip\nDELTA=4\n").expect("write");
        let store = DotenvCredentialsStore::new(&path);
        let names = store.list().await.expect("list");
        assert_eq!(
            names,
            vec![
                "ALPHA".to_string(),
                "BETA".to_string(),
                "DELTA".to_string(),
                "GAMMA".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn dotenv_credentials_store_list_missing_file_returns_empty() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.env");
        let store = DotenvCredentialsStore::new(&path);
        let names = store.list().await.expect("list");
        assert!(names.is_empty());
    }

    // ----- LayeredCredentialsStore -----

    #[test]
    fn layered_credentials_store_first_layer_wins() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(&path, "OPENAI_KEY=from-dotenv\n").expect("write");

        with_env(
            "MA_HARNESS_TEST_LAYERED_ENV_OPENAI_KEY",
            Some("from-env"),
            || {
                let env_store: Arc<dyn CredentialsStore> =
                    Arc::new(EnvCredentialsStore::new("MA_HARNESS_TEST_LAYERED_ENV_"));
                let dotenv_store: Arc<dyn CredentialsStore> =
                    Arc::new(DotenvCredentialsStore::new(&path));
                // env 在 list 前面 (高优先级)
                let layered = LayeredCredentialsStore::new(vec![env_store, dotenv_store]);
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let cred = rt.block_on(layered.get("openai_key")).expect("get");
                assert_eq!(cred.value, "from-env");
            },
        );
    }

    #[test]
    fn layered_credentials_store_falls_back_to_next_layer() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".env");
        std::fs::write(&path, "OPENAI_KEY=from-dotenv\n").expect("write");

        with_env("MA_HARNESS_TEST_LAYERED_FALLBACK_OPENAI_KEY", None, || {
            let env_store: Arc<dyn CredentialsStore> = Arc::new(EnvCredentialsStore::new(
                "MA_HARNESS_TEST_LAYERED_FALLBACK_",
            ));
            let dotenv_store: Arc<dyn CredentialsStore> =
                Arc::new(DotenvCredentialsStore::new(&path));
            let layered = LayeredCredentialsStore::new(vec![env_store, dotenv_store]);
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let cred = rt.block_on(layered.get("openai_key")).expect("get");
            assert_eq!(cred.value, "from-dotenv");
        });
    }

    #[test]
    fn layered_credentials_store_no_layer_has_key() {
        let env_store: Arc<dyn CredentialsStore> =
            Arc::new(EnvCredentialsStore::new("MA_HARNESS_TEST_LAYERED_NONE_"));
        let layered = LayeredCredentialsStore::new(vec![env_store]);
        with_env("MA_HARNESS_TEST_LAYERED_NONE_MISSING", None, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let err = rt.block_on(layered.get("missing")).unwrap_err();
            assert!(matches!(err, CredentialsError::NotFound(_)));
        });
    }

    #[test]
    fn layered_credentials_store_list_merges_layers() {
        with_env("MA_HARNESS_TEST_LAYERED_MERGE_FROM_ENV", Some("x"), || {
            let dir = tempdir().expect("tempdir");
            let path = dir.path().join(".env");
            std::fs::write(&path, "FROM_DOTENV=y\n").expect("write");
            let env_store: Arc<dyn CredentialsStore> =
                Arc::new(EnvCredentialsStore::new("MA_HARNESS_TEST_LAYERED_MERGE_"));
            let dotenv_store: Arc<dyn CredentialsStore> =
                Arc::new(DotenvCredentialsStore::new(&path));
            let layered = LayeredCredentialsStore::new(vec![env_store, dotenv_store]);
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let names = rt.block_on(layered.list()).expect("list");
            // 合并 + 去重 + 排序
            assert!(names.contains(&"from_env".to_string()));
            assert!(names.contains(&"FROM_DOTENV".to_string()));
        });
    }

    // ----- default_credentials_path -----

    #[test]
    fn default_credentials_path_respects_env_override() {
        with_env("MA_HARNESS_CREDENTIALS", Some("/tmp/custom.env"), || {
            let result = default_credentials_path().expect("path");
            assert_eq!(result, PathBuf::from("/tmp/custom.env"));
        });
    }

    #[test]
    fn default_credentials_path_falls_back_to_home() {
        with_env("MA_HARNESS_CREDENTIALS", None, || {
            let result = default_credentials_path().expect("path");
            assert!(result.ends_with(".ma-harness/.env"));
        });
    }

    // ----- Debug impl redaction -----

    #[test]
    fn env_credentials_store_debug_shows_prefix_not_value() {
        let store = EnvCredentialsStore::new("MA_HARNESS_TEST_DEBUG_PREFIX_");
        let debug = format!("{store:?}");
        assert!(debug.contains("MA_HARNESS_TEST_DEBUG_PREFIX_"));
        // 不需要真的设 env var, debug 不显示 value
    }

    #[test]
    fn dotenv_credentials_store_debug_shows_path() {
        let dir = tempdir().expect("tempdir");
        let store = DotenvCredentialsStore::new(dir.path().join("secrets.env"));
        let debug = format!("{store:?}");
        assert!(debug.contains("DotenvCredentialsStore"));
        assert!(debug.contains("secrets.env"));
    }

    // ----- helpers -----

    /// Set env var, run closure, restore original
    fn with_env<F: FnOnce()>(name: &str, value: Option<&str>, f: F) {
        let original = std::env::var_os(name);
        unsafe {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
        f();
        unsafe {
            match original {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }
}
