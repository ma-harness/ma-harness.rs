//! EventLog — append-only SessionEvent 日志 (rusqlite 实现)
//!
//! Week 1 Day 7 实现. 设计见 `docs/ma-harness-arch-map.md` §4.
//!
//! # 关键不变量
//!
//! **"model-visible means logged"**: 任何 model context 里能看到的字符串, 都必须能
//! 在 SessionEvent 日志里查到对应事件. 落库失败 → panic (append-only 不能丢).
//!
//! # SQLite Schema
//!
//! ```sql
//! CREATE TABLE events (
//!     seq INTEGER PRIMARY KEY AUTOINCREMENT,
//!     id TEXT NOT NULL UNIQUE,
//!     session_id TEXT NOT NULL,
//!     event_type INTEGER NOT NULL,
//!     ts TEXT NOT NULL,                -- RFC3339 UTC
//!     severity INTEGER NOT NULL,
//!     run_id TEXT,
//!     plugin_name TEXT,
//!     payload_json TEXT,
//!     error_message TEXT,
//!     model_visible INTEGER NOT NULL   -- 0/1
//! );
//! CREATE INDEX idx_events_session ON events(session_id, seq);
//! CREATE INDEX idx_events_visible ON events(session_id, model_visible) WHERE model_visible = 1;
//! ```
//!
//! # 线程安全
//!
//! `EventLog` 内部用 `parking_lot::Mutex<Connection>`, 单写多读.
//! Phase 2 加 `r2d2` 连接池提升并发读.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{params, Connection};

use crate::event::{EventType, SessionEvent, Severity};

/// EventLog 主结构
#[derive(Clone)]
pub struct EventLog {
    inner: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for EventLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventLog").finish_non_exhaustive()
    }
}

/// EventQuery — 过滤参数
#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    /// 必填
    pub session_id: String,
    /// 包含起始 seq (None = 0)
    pub seq_from: Option<i64>,
    /// 包含结束 seq (None = MAX)
    pub seq_to: Option<i64>,
    /// 过滤 event_type 列表 (None = 全部)
    pub event_types: Option<Vec<EventType>>,
    /// 只取 model-visible 事件
    pub model_visible_only: bool,
    /// 最多多少条
    pub limit: Option<u32>,
    /// 跳过多少条
    pub offset: Option<u32>,
}

/// EventPage — 分页结果
#[derive(Debug, Clone)]
pub struct EventPage {
    /// 事件列表 (按 seq 升序)
    pub events: Vec<StoredEvent>,
    /// 范围内总事件数
    pub total_in_range: u64,
    /// 是否还有更多 (offset + len < total)
    pub has_more: bool,
}

/// StoredEvent — 日志里的事件 (含 seq)
#[derive(Debug, Clone)]
pub struct StoredEvent {
    /// 自增 seq
    pub seq: i64,
    /// 事件本身
    pub event: SessionEvent,
}

impl EventLog {
    /// 打开 (或创建) 一个事件日志, 落 path
    pub fn open<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// 打开内存日志 (测试用)
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                session_id TEXT NOT NULL,
                event_type INTEGER NOT NULL,
                ts TEXT NOT NULL,
                severity INTEGER NOT NULL,
                run_id TEXT,
                plugin_name TEXT,
                payload_json TEXT,
                error_message TEXT,
                model_visible INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id, seq);
            CREATE INDEX IF NOT EXISTS idx_events_visible
                ON events(session_id, model_visible) WHERE model_visible = 1;
            "#,
        )
    }

    /// 追加一个事件
    ///
    /// **不变量校验**: 先 `event.validate()`, 失败 → panic.
    /// **同步落库**: 写失败 → panic. append-only 模式不允许静默丢.
    ///
    /// 返回分配的 `seq` (1-based, session 内单调递增).
    #[track_caller]
    pub fn append(&self, event: SessionEvent) -> i64 {
        // 1. 不变量校验
        if let Err(msg) = event.validate() {
            panic!("EventLog.append 不变量违反: {}", msg);
        }

        // 2. 同步落库
        let conn = self.inner.lock();
        let ts_str = event.ts.to_rfc3339();
        let result = conn.execute(
            r#"
            INSERT INTO events
                (id, session_id, event_type, ts, severity, run_id, plugin_name, payload_json, error_message, model_visible)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                event.id,
                event.session_id,
                event.event_type as i32,
                ts_str,
                event.severity as i32,
                event.run_id,
                event.plugin_name,
                event.payload_json,
                event.error_message,
                event.model_visible as i32,
            ],
        );

        match result {
            Ok(_) => {
                // 3. 拿回分配的 seq
                let seq: i64 = conn
                    .query_row(
                        "SELECT seq FROM events WHERE id = ?",
                        params![event.id],
                        |row| row.get(0),
                    )
                    .expect("刚 insert 的 row 拿不到 seq, 这是 bug");
                tracing::trace!(seq, event_type = %event.event_type, "event appended");
                seq
            }
            Err(e) => panic!(
                "EventLog.append 落库失败 (id={}, session={}, type={}): {}",
                event.id, event.session_id, event.event_type, e
            ),
        }
    }

    /// 查询事件
    pub fn query(&self, q: &EventQuery) -> rusqlite::Result<EventPage> {
        let conn = self.inner.lock();

        // build WHERE
        let mut sql = String::from("SELECT seq, id, session_id, event_type, ts, severity, run_id, plugin_name, payload_json, error_message, model_visible FROM events WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        sql.push_str(" AND session_id = ?");
        args.push(Box::new(q.session_id.clone()));

        if let Some(from) = q.seq_from {
            sql.push_str(" AND seq >= ?");
            args.push(Box::new(from));
        }
        if let Some(to) = q.seq_to {
            sql.push_str(" AND seq <= ?");
            args.push(Box::new(to));
        }
        if q.model_visible_only {
            sql.push_str(" AND model_visible = 1");
        }
        if let Some(types) = &q.event_types {
            if !types.is_empty() {
                let placeholders: Vec<&str> = types.iter().map(|_| "?").collect();
                sql.push_str(&format!(" AND event_type IN ({})", placeholders.join(",")));
                for t in types {
                    args.push(Box::new(*t as i32));
                }
            }
        }

        // 先查 total
        let count_sql = format!("SELECT COUNT(*) FROM ({})", sql);
        let total: i64 = conn.query_row(
            &count_sql,
            rusqlite::params_from_iter(args.iter().map(|b| b.as_ref())),
            |row| row.get(0),
        )?;

        // 排序 + 分页
        sql.push_str(" ORDER BY seq ASC");
        if let Some(limit) = q.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = q.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let mut stmt = conn.prepare(&sql)?;
        let events: rusqlite::Result<Vec<StoredEvent>> = stmt
            .query_map(rusqlite::params_from_iter(args.iter().map(|b| b.as_ref())), |row| {
                let seq: i64 = row.get(0)?;
                let id: String = row.get(1)?;
                let session_id: String = row.get(2)?;
                let event_type_int: i32 = row.get(3)?;
                let ts_str: String = row.get(4)?;
                let severity_int: i32 = row.get(5)?;
                let run_id: Option<String> = row.get(6)?;
                let plugin_name: Option<String> = row.get(7)?;
                let payload_json: Option<String> = row.get(8)?;
                let error_message: Option<String> = row.get(9)?;
                let model_visible_int: i32 = row.get(10)?;

                let event = SessionEvent {
                    id,
                    session_id,
                    event_type: EventType::from_i32(event_type_int),
                    ts: parse_rfc3339(&ts_str)?,
                    severity: severity_from_int(severity_int),
                    run_id,
                    plugin_name,
                    payload_json,
                    error_message,
                    model_visible: model_visible_int != 0,
                };
                Ok(StoredEvent { seq, event })
            })?
            .collect();

        let events = events?;
        let total_u = total as u64;
        let has_more = (q.offset.unwrap_or(0) as u64) + (events.len() as u64) < total_u;

        Ok(EventPage {
            events,
            total_in_range: total_u,
            has_more,
        })
    }

    /// 取 session 所有 model-visible 事件 (用于 replay 到 model context)
    pub fn get_model_visible(&self, session_id: &str) -> rusqlite::Result<EventPage> {
        self.query(&EventQuery {
            session_id: session_id.to_string(),
            model_visible_only: true,
            ..Default::default()
        })
    }

    /// 统计 session 事件总数
    pub fn count(&self, session_id: &str) -> rusqlite::Result<i64> {
        let conn = self.inner.lock();
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE session_id = ?",
            params![session_id],
            |row| row.get(0),
        )
    }
}

fn parse_rfc3339(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

fn severity_from_int(v: i32) -> Severity {
    match v {
        0 => Severity::Debug,
        1 => Severity::Info,
        2 => Severity::Warn,
        3 => Severity::Error,
        4 => Severity::Fatal,
        _ => Severity::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log() -> EventLog {
        EventLog::open_in_memory().unwrap()
    }

    fn sample_event(session: &str, t: EventType) -> SessionEvent {
        SessionEvent::new(session, t)
            .with_payload(&serde_json::json!({"k": "v"}))
            .unwrap()
    }

    #[test]
    fn open_creates_schema() {
        let _ = log();
    }

    #[test]
    fn append_and_query_round_trip() {
        let l = log();
        let seq1 = l.append(sample_event("s1", EventType::SessionStart));
        let seq2 = l.append(sample_event("s1", EventType::RunStart));
        assert!(seq2 > seq1);

        let page = l
            .query(&EventQuery {
                session_id: "s1".to_string(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.events.len(), 2);
        assert_eq!(page.total_in_range, 2);
    }

    #[test]
    fn query_filters_by_event_type() {
        let l = log();
        l.append(sample_event("s1", EventType::SessionStart));
        l.append(sample_event("s1", EventType::RunStart));
        l.append(sample_event("s1", EventType::ToolCall));

        let page = l
            .query(&EventQuery {
                session_id: "s1".to_string(),
                event_types: Some(vec![EventType::RunStart]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].event.event_type, EventType::RunStart);
    }

    #[test]
    fn query_model_visible_only() {
        let l = log();
        l.append(sample_event("s1", EventType::SessionStart));
        l.append(sample_event("s1", EventType::ModelError)); // 不是 model_visible
        l.append(sample_event("s1", EventType::RunStart));

        let page = l
            .query(&EventQuery {
                session_id: "s1".to_string(),
                model_visible_only: true,
                ..Default::default()
            })
            .unwrap();
        // SessionStart + RunStart = 2
        assert_eq!(page.events.len(), 2);
        assert!(page.events.iter().all(|e| e.event.model_visible));
    }

    #[test]
    fn get_model_visible_returns_only_visible() {
        let l = log();
        l.append(sample_event("s1", EventType::SessionStart));
        l.append(sample_event("s1", EventType::ModelError));

        let page = l.get_model_visible("s1").unwrap();
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].event.event_type, EventType::SessionStart);
    }

    #[test]
    fn query_limit_and_offset() {
        let l = log();
        for _ in 0..5 {
            l.append(sample_event("s1", EventType::RunStart));
        }

        let page = l
            .query(&EventQuery {
                session_id: "s1".to_string(),
                limit: Some(2),
                offset: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.events.len(), 2);
        assert_eq!(page.total_in_range, 5);
        assert!(page.has_more);
    }

    #[test]
    fn count_returns_event_total() {
        let l = log();
        assert_eq!(l.count("s1").unwrap(), 0);
        l.append(sample_event("s1", EventType::SessionStart));
        l.append(sample_event("s1", EventType::RunStart));
        l.append(sample_event("s2", EventType::SessionStart));
        assert_eq!(l.count("s1").unwrap(), 2);
        assert_eq!(l.count("s2").unwrap(), 1);
    }

    #[test]
    #[should_panic(expected = "不变量违反")]
    fn append_panics_on_invalid_event() {
        let l = log();
        // 没有 payload 但 model_visible=true → 不变量违反
        let bad = SessionEvent::new("s1", EventType::SessionStart);
        // bad.payload_json = None (默认), bad.model_visible = true (auto)
        l.append(bad);
    }

    #[test]
    #[should_panic(expected = "落库失败")]
    fn append_panics_on_db_failure() {
        // 用一个只读连接模拟落库失败
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("readonly.db");
        // 先创建表, 再 chmod 只读
        {
            let _ = EventLog::open(&path).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o444); // read-only
            std::fs::set_permissions(&path, perms).unwrap();
        }
        // 尝试开 + append
        let l = EventLog::open(&path).unwrap();
        l.append(sample_event("s1", EventType::SessionStart));
    }

    #[test]
    fn events_are_append_only_no_update_path() {
        // 验证 EventLog 没有提供 update / delete API (编译期保证)
        // 这里只验 schema 没有 UPDATE / DELETE 权限被定义
        // (实际保证: 这个文件里不提供 update / delete fn, 类型签名也没有)
        let l = log();
        l.append(sample_event("s1", EventType::SessionStart));
        // 没有任何 API 可以改已经落库的事件
    }
}
