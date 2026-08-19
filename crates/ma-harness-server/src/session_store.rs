//! SessionStore trait + 2 个实现 (Phase 2.6 持久化)
//!
//! **目标**: SessionServiceImpl 跟存储解耦, 默认 `InMemoryStore`, 业务方要持久化
//! 用 `SqliteStore` (rusqlite). 同 trait 两套都能用.
//!
//! **设计**:
//! - `SessionStore` trait: get / put / delete / list
//! - `InMemoryStore` (Phase 1 默认, 跟现有 DashMap 行为一致)
//! - `SqliteStore` (Phase 2.6, rusqlite, schema 跟 proto Session 对齐)
//! - SessionServiceImpl 接 `Arc<dyn SessionStore>`, 默认 InMemory
//!
//! **限制 (Phase 2.6 PoC)**:
//! - SqliteStore 只持久化 session metadata (name/state/mode/timestamps/metadata JSON),
//!   events 仍走 ma_harness_core::EventLog (已经 rusqlite, Phase 1 实现)
//! - 不支持 migration (drop table 重 create)
//! - 不支持 session stats (run_count/event_count) 持久化 (Phase 2.7)

use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

use ma_harness_proto::ma_harness::v1::{Session as ProtoSession, SessionMetadata};

#[derive(Debug, Error)]
pub enum SessionStoreError {
    #[error("session not found: {0}")]
    NotFound(String),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// SessionStore trait — 抽象 session CRUD
pub trait SessionStore: Send + Sync + 'static {
    /// 创建 session
    fn create(&self, session: &ProtoSession) -> Result<(), SessionStoreError>;

    /// 拿 session
    fn get(&self, id: &str) -> Result<Option<ProtoSession>, SessionStoreError>;

    /// 列所有 session (Phase 2.6: 不分页, 后续加分页参数)
    fn list(&self) -> Result<Vec<ProtoSession>, SessionStoreError>;

    /// 更新 (close 时改 state/closed_at)
    fn update(&self, session: &ProtoSession) -> Result<(), SessionStoreError>;

    /// 删除 (Phase 2.6 暂不暴露, 留接口)
    fn delete(&self, id: &str) -> Result<(), SessionStoreError>;
}

// ============================================================================
// InMemoryStore — Phase 1 行为, 默认
// ============================================================================

/// SessionMetadata (prost::Message) → JSON string.
/// 手写 (因 prost Message 不 impl serde), 序列化已知 string 字段.
fn metadata_to_json(m: &SessionMetadata) -> Result<String, serde_json::Error> {
    let v = serde_json::json!({
        "agents_md_path": m.agents_md_path,
        "working_directory": m.working_directory,
        "profile": m.profile,
    });
    serde_json::to_string(&v)
}

/// JSON string → SessionMetadata.
/// 解析失败时返 None (宽容; 老数据/手动编辑可能缺字段).
fn metadata_from_json(s: &str) -> Result<Option<SessionMetadata>, serde_json::Error> {
    let v: serde_json::Value = serde_json::from_str(s)?;
    let m = SessionMetadata {
        agents_md_path: v
            .get("agents_md_path")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        working_directory: v
            .get("working_directory")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        profile: v
            .get("profile")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        custom: std::collections::HashMap::new(),
    };
    Ok(Some(m))
}

/// 内存版 store (DashMap 包装, 跟 Phase 1 ServerBuilder 默认行为一致)
pub struct InMemoryStore {
    inner: dashmap::DashMap<String, ProtoSession>,
}

impl InMemoryStore {
    /// 构造空 store
    pub fn new() -> Self {
        Self {
            inner: dashmap::DashMap::new(),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore for InMemoryStore {
    fn create(&self, session: &ProtoSession) -> Result<(), SessionStoreError> {
        self.inner.insert(session.id.clone(), session.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<ProtoSession>, SessionStoreError> {
        Ok(self.inner.get(id).map(|e| e.value().clone()))
    }

    fn list(&self) -> Result<Vec<ProtoSession>, SessionStoreError> {
        Ok(self.inner.iter().map(|e| e.value().clone()).collect())
    }

    fn update(&self, session: &ProtoSession) -> Result<(), SessionStoreError> {
        self.inner.insert(session.id.clone(), session.clone());
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), SessionStoreError> {
        self.inner.remove(id);
        Ok(())
    }
}

// ============================================================================
// SqliteStore — Phase 2.6 持久化
// ============================================================================

/// SQLite 版 store (单 Connection, Arc<Mutex> 包裹, 线程安全)
pub struct SqliteStore {
    conn: Arc<std::sync::Mutex<Connection>>,
}

impl SqliteStore {
    /// 打开 / 创建 sqlite db + 跑 schema migration
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self, SessionStoreError> {
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(std::sync::Mutex::new(conn)),
        })
    }

    /// 内存版 sqlite (跟 rusqlite::Connection::open_in_memory 一致, 测试用)
    pub fn open_in_memory() -> Result<Self, SessionStoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(std::sync::Mutex::new(conn)),
        })
    }

    fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                state INTEGER NOT NULL,
                mode INTEGER NOT NULL,
                created_at_secs INTEGER,
                created_at_nanos INTEGER,
                updated_at_secs INTEGER,
                updated_at_nanos INTEGER,
                closed_at_secs INTEGER,
                closed_at_nanos INTEGER,
                metadata_json TEXT,
                enabled_plugins_json TEXT,
                user_id TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);
            "#,
        )
    }

    /// 提取 Timestamp (prost_types::Timestamp) 转 (i64 secs, i32 nanos) for sqlite
    fn ts_to_parts(
        ts: &Option<prost_types::Timestamp>,
    ) -> (Option<i64>, Option<i32>) {
        match ts {
            Some(t) => (Some(t.seconds), Some(t.nanos)),
            None => (None, None),
        }
    }

    /// 写 row
    fn write_session(
        &self,
        session: &ProtoSession,
    ) -> Result<(), SessionStoreError> {
        let (cs, cn) = Self::ts_to_parts(&session.created_at);
        let (us, un) = Self::ts_to_parts(&session.updated_at);
        let (xs, xn) = Self::ts_to_parts(&session.closed_at);
        // SessionMetadata 是 prost::Message, 不 impl serde, 手写 JSON 转换
        let metadata_json = session
            .metadata
            .as_ref()
            .map(metadata_to_json)
            .transpose()?;
        let plugins_json = serde_json::to_string(&session.enabled_plugins)?;

        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        conn.execute(
            r#"INSERT OR REPLACE INTO sessions (
                id, name, state, mode,
                created_at_secs, created_at_nanos,
                updated_at_secs, updated_at_nanos,
                closed_at_secs, closed_at_nanos,
                metadata_json, enabled_plugins_json, user_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
            params![
                session.id,
                session.name,
                session.state,
                session.mode,
                cs,
                cn,
                us,
                un,
                xs,
                xn,
                metadata_json,
                plugins_json,
                session.user_id,
            ],
        )?;
        Ok(())
    }

    /// 读 row → ProtoSession
    fn read_session(row: &rusqlite::Row) -> rusqlite::Result<ProtoSession> {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let state: i32 = row.get(2)?;
        let mode: i32 = row.get(3)?;
        let cs: Option<i64> = row.get(4)?;
        let cn: Option<i32> = row.get(5)?;
        let us: Option<i64> = row.get(6)?;
        let un: Option<i32> = row.get(7)?;
        let xs: Option<i64> = row.get(8)?;
        let xn: Option<i32> = row.get(9)?;
        let metadata_json: Option<String> = row.get(10)?;
        let plugins_json: Option<String> = row.get(11)?;
        let user_id: String = row.get(12)?;

        fn ts(secs: Option<i64>, nanos: Option<i32>) -> Option<prost_types::Timestamp> {
            match (secs, nanos) {
                (Some(s), Some(n)) => Some(prost_types::Timestamp { seconds: s, nanos: n }),
                _ => None,
            }
        }
        let created_at = ts(cs, cn);
        let updated_at = ts(us, un);
        let closed_at = ts(xs, xn);
        // SessionMetadata 是 prost::Message, 从 JSON 解析回来.
        // metadata_json 是 Option<String>, metadata_from_json 返 Result<Option<SessionMetadata>, _>
        // -> 想要 Option<SessionMetadata>: 用 .and_then 串接.
        let metadata = metadata_json
            .and_then(|s| metadata_from_json(&s).ok().flatten());
        let enabled_plugins = plugins_json
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e)))?
            .unwrap_or_default();

        Ok(ProtoSession {
            id,
            name,
            state,
            mode,
            created_at,
            updated_at,
            closed_at,
            metadata,
            stats: None,
            enabled_plugins,
            user_id,
        })
    }
}

impl SessionStore for SqliteStore {
    fn create(&self, session: &ProtoSession) -> Result<(), SessionStoreError> {
        self.write_session(session)
    }

    fn get(&self, id: &str) -> Result<Option<ProtoSession>, SessionStoreError> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        let session = conn
            .query_row(
                "SELECT id, name, state, mode, created_at_secs, created_at_nanos, updated_at_secs, updated_at_nanos, closed_at_secs, closed_at_nanos, metadata_json, enabled_plugins_json, user_id FROM sessions WHERE id = ?1",
                params![id],
                Self::read_session,
            )
            .optional()?;
        Ok(session)
    }

    fn list(&self) -> Result<Vec<ProtoSession>, SessionStoreError> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, name, state, mode, created_at_secs, created_at_nanos, updated_at_secs, updated_at_nanos, closed_at_secs, closed_at_nanos, metadata_json, enabled_plugins_json, user_id FROM sessions ORDER BY created_at_secs DESC",
        )?;
        let sessions = stmt
            .query_map([], Self::read_session)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(sessions)
    }

    fn update(&self, session: &ProtoSession) -> Result<(), SessionStoreError> {
        self.write_session(session)
    }

    fn delete(&self, id: &str) -> Result<(), SessionStoreError> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        let affected = conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        if affected == 0 {
            return Err(SessionStoreError::NotFound(id.to_string()));
        }
        Ok(())
    }
}

// ============================================================================
// Type alias — 默认 store
// ============================================================================

/// 默认 store (跟 ServerBuilder 默认行为一致, 内存)
pub type DefaultSessionStore = InMemoryStore;

#[doc(hidden)]
pub fn default_store() -> Arc<dyn SessionStore> {
    Arc::new(InMemoryStore::new())
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ma_harness_proto::ma_harness::v1::{OperatingMode, SessionState};

    fn sample_session(id: &str, name: &str) -> ProtoSession {
        ProtoSession {
            id: id.to_string(),
            name: name.to_string(),
            state: SessionState::Created as i32,
            mode: OperatingMode::Default as i32,
            created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            closed_at: None,
            metadata: None,
            stats: None,
            enabled_plugins: vec!["hello".to_string(), "fs".to_string()],
            user_id: "user-1".to_string(),
        }
    }

    // ---- InMemoryStore ----

    #[test]
    fn in_memory_create_and_get() {
        let store = InMemoryStore::new();
        store.create(&sample_session("s1", "first")).unwrap();
        let got = store.get("s1").unwrap().unwrap();
        assert_eq!(got.id, "s1");
        assert_eq!(got.name, "first");
        assert_eq!(got.user_id, "user-1");
        assert_eq!(got.enabled_plugins, vec!["hello".to_string(), "fs".to_string()]);
    }

    #[test]
    fn in_memory_get_missing_returns_none() {
        let store = InMemoryStore::new();
        assert!(store.get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn in_memory_list_empty() {
        let store = InMemoryStore::new();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn in_memory_list_multiple() {
        let store = InMemoryStore::new();
        for i in 0..3 {
            store
                .create(&sample_session(&format!("s{i}"), &format!("name{i}")))
                .unwrap();
        }
        let list = store.list().unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn in_memory_update_overwrites() {
        let store = InMemoryStore::new();
        let mut s = sample_session("s1", "first");
        store.create(&s).unwrap();
        s.name = "renamed".to_string();
        s.state = SessionState::Active as i32;
        store.update(&s).unwrap();
        let got = store.get("s1").unwrap().unwrap();
        assert_eq!(got.name, "renamed");
        assert_eq!(got.state, SessionState::Active as i32);
    }

    #[test]
    fn in_memory_delete() {
        let store = InMemoryStore::new();
        store.create(&sample_session("s1", "first")).unwrap();
        store.delete("s1").unwrap();
        assert!(store.get("s1").unwrap().is_none());
    }

    // ---- SqliteStore ----

    #[test]
    fn sqlite_create_and_get() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.create(&sample_session("s1", "first")).unwrap();
        let got = store.get("s1").unwrap().unwrap();
        assert_eq!(got.id, "s1");
        assert_eq!(got.name, "first");
        assert_eq!(got.user_id, "user-1");
    }

    #[test]
    fn sqlite_persistence_across_instances() {
        // 模拟 restart: 1 个 SqliteStore 写, drop, 重新 open 读
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.db");

        // 1. 写
        let s1 = SqliteStore::open(&path).unwrap();
        s1.create(&sample_session("s1", "first")).unwrap();
        s1.create(&sample_session("s2", "second")).unwrap();
        drop(s1);

        // 2. 重启 + 读
        let s2 = SqliteStore::open(&path).unwrap();
        let list = s2.list().unwrap();
        assert_eq!(list.len(), 2, "2 个 session 应该从磁盘恢复");
        let got1 = s2.get("s1").unwrap().unwrap();
        assert_eq!(got1.name, "first");
    }

    #[test]
    fn sqlite_update_and_list() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut s = sample_session("s1", "first");
        store.create(&s).unwrap();
        s.closed_at = Some(prost_types::Timestamp::from(std::time::SystemTime::now()));
        s.state = SessionState::Closed as i32;
        store.update(&s).unwrap();

        let got = store.get("s1").unwrap().unwrap();
        assert_eq!(got.state, SessionState::Closed as i32);
        assert!(got.closed_at.is_some());

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn sqlite_delete_not_found_errors() {
        let store = SqliteStore::open_in_memory().unwrap();
        let result = store.delete("nonexistent");
        assert!(matches!(result, Err(SessionStoreError::NotFound(_))));
    }

    #[test]
    fn sqlite_metadata_roundtrip() {
        let metadata = SessionMetadata {
            agents_md_path: "/path/to/AGENTS.md".to_string(),
            working_directory: "/work/dir".to_string(),
            profile: "default".to_string(),
            custom: std::collections::HashMap::new(),
        };
        let mut s = sample_session("s1", "first");
        s.metadata = Some(metadata);
        let store = SqliteStore::open_in_memory().unwrap();
        store.create(&s).unwrap();
        let got = store.get("s1").unwrap().unwrap();
        // metadata_from_json 返 Option<Option<SessionMetadata>>, inner None = 解析失败
        let got_meta = got.metadata.unwrap();
        assert_eq!(got_meta.agents_md_path, "/path/to/AGENTS.md");
        assert_eq!(got_meta.working_directory, "/work/dir");
        assert_eq!(got_meta.profile, "default");
    }
}
