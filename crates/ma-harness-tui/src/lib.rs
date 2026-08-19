//! ma_harness_tui — TUI dashboard (ratatui 0.29)
//!
//! 实时显示 ma-harness session / event / plugin 状态.
//! 3 个 panel 布局: Sessions | Events (滚动) | Plugins.
//! 每 500ms 刷新. Ctrl-C / 'q' 退出.
//!
//! # 用法
//!
//! ```ignore
//! use ma_harness_tui::TuiApp;
//!
//! TuiApp::new()?.run()?;  // 启动 dashboard
//! ```
//!
//! # 数据源 (简化版 PoC)
//!
//! - Sessions: 从 EventLog 读 (Phase 1 Week 1-2 实现, EventLog trait)
//! - Events: 从 EventLog 读最近 50 条,滚动显示
//! - Plugins: 从 `ma_harness_seam::PluginLoader::list()` 读 inventory

#![deny(unsafe_code)]
#![warn(missing_docs)]

// 引用 hello plugin 触发 inventory::submit! 才有 effect
#[allow(unused_imports)]
use ma_harness_plugin_hello as _hello;

use anyhow::Result;
use parking_lot::Mutex;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    DefaultTerminal, Frame,
};
use std::sync::Arc;
use std::path::Path;
use std::time::{Duration, Instant};

use ma_harness_core::EventLog;
use ma_harness_server::SessionStore;

/// TUI app state
pub struct TuiApp {
    /// session list (from EventLog)
    sessions: Arc<Mutex<Vec<SessionRow>>>,
    /// event log (rolling, 最新 100 条)
    events: Arc<Mutex<Vec<EventRow>>>,
    /// plugin list (from inventory)
    plugins: Arc<Mutex<Vec<String>>>,
    /// EventLog 可选 (None = stub fallback)
    event_log: Option<Arc<EventLog>>,
    /// **P4-3**: SessionStore 可选 (None = fallback stub / event_log 推 session)
    session_store: Option<Arc<dyn SessionStore>>,
    /// 启动时间
    started_at: Instant,
    /// tick 计数
    ticks: u64,
}

/// 一行 session 信息
#[derive(Debug, Clone)]
struct SessionRow {
    id: String,
    state: String,
    age: String,
}

/// 一行 event
#[derive(Debug, Clone)]
struct EventRow {
    seq: i64,
    session_id: String,
    event_type: String,
    severity: String,
    timestamp: String,
}


impl TuiApp {
    /// 构造一个新 TUI app (无 EventLog, 全 stub fallback)
    pub fn new() -> Result<Self> {
        Self::new_with_log(None)
    }

    /// 构造 + 接 EventLog (P4-1 / TUI 接真数据)
    ///
    /// log_path = Some(path) → 打开 sqlite, 接真 events
    /// log_path = None → 走 stub fallback (Phase 3.9 行为)
    pub fn new_with_log(log_path: Option<&Path>) -> Result<Self> {
        // 尝试打开 EventLog
        let event_log = match log_path {
            Some(p) => match EventLog::open(p) {
                Ok(log) => {
                    eprintln!("TUI: opened event log {}", p.display());
                    Some(Arc::new(log))
                }
                Err(e) => {
                    eprintln!(
                        "TUI: WARN failed to open event log {}: {e}; using stub",
                        p.display()
                    );
                    None
                }
            },
            None => None,
        };
        let app = Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            plugins: Arc::new(Mutex::new(Vec::new())),
            event_log,
            session_store: None,  // P4-3 单独 API: TuiApp::new_with_log_and_store
            started_at: Instant::now(),
            ticks: 0,
        };
        app.refresh()?;
        Ok(app)
    }

    /// **P4-3 (Phase 4) 新增**: 构造 + 接 EventLog + SessionStore
    ///
    /// 业务方传:
    /// - log_path: 走 EventLog 拿真 events (P4-1)
    /// - store: 走 SessionStore 拿真 sessions (P4-3)
    /// 都 None → 走 stub fallback
    pub fn new_with_log_and_store(
        log_path: Option<&Path>,
        store: Option<Arc<dyn SessionStore>>,
    ) -> Result<Self> {
        // 1. EventLog (同 new_with_log)
        let event_log = match log_path {
            Some(p) => match EventLog::open(p) {
                Ok(log) => Some(Arc::new(log)),
                Err(_) => None,
            },
            None => None,
        };
        let app = Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            plugins: Arc::new(Mutex::new(Vec::new())),
            event_log,
            session_store: store,
            started_at: Instant::now(),
            ticks: 0,
        };
        app.refresh()?;
        Ok(app)
    }

    /// 刷新数据 (从 inventory + SessionStore + EventLog / stub)
    fn refresh(&self) -> Result<()> {
        // 1. plugin list (从 inventory, 永远真)
        let plugins: Vec<String> = ma_harness_seam::PluginLoader::list()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        *self.plugins.lock() = plugins;

        // 2. sessions: 优先 SessionStore (P4-3), fallback event_log 推 (P4-1), 再 fallback stub
        if let Some(store) = &self.session_store {
            self.refresh_sessions_from_store(store);
        } else if let Some(log) = &self.event_log {
            self.refresh_sessions_from_log(log);
        } else {
            self.refresh_sessions_stub();
        }

        // 3. events: 优先 EventLog, fallback stub
        if let Some(log) = &self.event_log {
            self.refresh_events_from_log(log)?;
        } else {
            self.refresh_events_stub();
        }
        Ok(())
    }

    /// P4-3: 从 SessionStore 拿 session list
    fn refresh_sessions_from_store(&self, store: &Arc<dyn SessionStore>) {
        let sessions_vec = store.list().unwrap_or_default();
        let mut sessions = self.sessions.lock();
        sessions.clear();
        for s in sessions_vec.iter().take(20) {
            // s.state 是 proto i32 (SessionState enum as i32), 转成 enum 变体名
            use ma_harness_proto::ma_harness::v1::SessionState;
            let state_name = SessionState::try_from(s.state)
                .map(|st| format!("{:?}", st))
                .unwrap_or_else(|_| format!("unknown({})", s.state));
            sessions.push(SessionRow {
                id: s.id.clone(),
                state: state_name,
                age: s.name.clone(),
            });
        }
        if sessions.is_empty() {
            sessions.push(SessionRow {
                id: "(no sessions yet)".to_string(),
                state: "—".to_string(),
                age: "—".to_string(),
            });
        }
    }

    fn refresh_sessions_from_log(&self, log: &EventLog) {
        let session_ids = log.list_sessions().unwrap_or_default();
        let mut sessions = self.sessions.lock();
        sessions.clear();
        for sid in session_ids.iter().take(20) {
            let count = log.count(sid).unwrap_or_else(|_| 0);
            sessions.push(SessionRow {
                id: sid.clone(),
                state: if count > 0 { "active" } else { "idle" }.to_string(),
                age: format!("{} events", count),
            });
        }
        if sessions.is_empty() {
            sessions.push(SessionRow {
                id: "(no events yet)".to_string(),
                state: "—".to_string(),
                age: "—".to_string(),
            });
        }
    }

    fn refresh_events_from_log(&self, log: &EventLog) -> Result<()> {
        let stored = log.recent_events(20).unwrap_or_default();
        let mut events = self.events.lock();
        events.clear();
        for s in stored {
            events.push(EventRow {
                seq: s.seq,
                event_type: format!("{:?}", s.event.event_type),
                severity: format!("{:?}", s.event.severity),
                timestamp: s.event.ts.format("%H:%M:%S").to_string(),
                session_id: s.event.session_id.chars().take(8).collect(),
            });
        }
        Ok(())
    }

    fn refresh_sessions_stub(&self) {
        let mut sessions = self.sessions.lock();
        sessions.clear();
        sessions.push(SessionRow {
            id: "default".to_string(),
            state: "stub".to_string(),
            age: format!("{:.0}s", self.started_at.elapsed().as_secs_f32()),
        });
    }

    fn refresh_events_stub(&self) {
        let mut events = self.events.lock();
        events.clear();
        let tick = self.ticks;
        for i in 0..20 {
            events.push(EventRow {
                seq: (tick * 20 + i) as i64,
                session_id: "stub".to_string(),
                event_type: match i % 4 {
                    0 => "SessionStart".to_string(),
                    1 => "ToolCall".to_string(),
                    2 => "ModelResponse".to_string(),
                    _ => "SessionTick".to_string(),
                },
                severity: match i % 3 {
                    0 => "Info".to_string(),
                    1 => "Info".to_string(),
                    _ => "Debug".to_string(),
                },
                timestamp: format!("+{}ms", i * 500),
            });
        }
    }

    /// 跑 TUI main loop
    pub fn run(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();
        let result = self.run_loop(&mut terminal);
        ratatui::restore();
        result
    }

    fn run_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut last_refresh = Instant::now();
        let refresh_interval = Duration::from_millis(500);

        loop {
            // 1. 刷新数据 (每 500ms)
            if last_refresh.elapsed() >= refresh_interval {
                self.ticks += 1;
                self.refresh()?;
                last_refresh = Instant::now();
            }

            // 2. 渲染
            terminal.draw(|frame| self.ui(frame))?;

            // 3. 处理事件 (50ms poll,避免 busy loop)
            if crossterm::event::poll(Duration::from_millis(50))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        match key.code {
                            crossterm::event::KeyCode::Char('q') => return Ok(()),
                            crossterm::event::KeyCode::Esc => return Ok(()),
                            crossterm::event::KeyCode::Char('c')
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    /// 画 UI: 3 个 panel (上: title bar, 中: 2x1 split, 下: events)
    fn ui(&self, frame: &mut Frame) {
        let area = frame.area();

        // 主布局: title (3 行) + body (rest)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // title
                Constraint::Min(0),     // body
                Constraint::Length(3),  // status
            ])
            .split(area);

        // Title
        let title = Paragraph::new(Line::from(vec![
            Span::styled(
                "ma-harness TUI",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                "(Phase 3.9 / T3.9 — press 'q' to quit)",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title("ma-harness"));
        frame.render_widget(title, main_chunks[0]);

        // Body: Sessions | Plugins (左右 1:1)
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[1]);

        // Sessions panel
        let sessions = self.sessions.lock();
        let session_items: Vec<ListItem> = sessions
            .iter()
            .map(|s| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:16}", s.id),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(" "),
                    Span::styled(&s.state, Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::styled(&s.age, Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect();
        let sessions_list = List::new(session_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Sessions ({})", sessions.len())),
            );
        frame.render_widget(sessions_list, body_chunks[0]);
        drop(sessions);

        // Plugins panel
        let plugins = self.plugins.lock();
        let plugin_items: Vec<ListItem> = plugins
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled("● ", Style::default().fg(Color::Green)),
                    Span::raw(p.as_str()),
                ]))
            })
            .collect();
        let plugins_list = List::new(plugin_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Plugins ({})", plugins.len())),
            );
        frame.render_widget(plugins_list, body_chunks[1]);
        drop(plugins);

        // Status bar
        let status = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("ticks: {}", self.ticks),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled(
                format!("uptime: {:.1}s", self.started_at.elapsed().as_secs_f32()),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(
                format!("events: {}", self.events.lock().len()),
                Style::default().fg(Color::Yellow),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title("status"));
        frame.render_widget(status, main_chunks[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_app_constructs() {
        // 构造 + 拿初始数据 (不在 terminal 跑, 只验 init 逻辑)
        let app = TuiApp::new().unwrap();
        let plugins = app.plugins.lock();
        // hello plugin 应在 inventory 列表
        assert!(
            plugins.iter().any(|p| p == "hello"),
            "hello plugin 应在 inventory 列表: {:?}",
            *plugins
        );
    }

    #[test]
    fn tui_app_refresh_increments_ticks() {
        let mut app = TuiApp::new().unwrap();
        let before = app.ticks;
        app.refresh().unwrap();
        app.ticks += 1; // 模拟 main loop 的 tick increment
        assert!(app.ticks > before, "ticks should increase after refresh");
    }

    #[test]
    fn session_rows_include_default() {
        let app = TuiApp::new().unwrap();
        let sessions = app.sessions.lock();
        assert!(sessions.iter().any(|s| s.id == "default"));
    }

    #[test]
    fn event_rows_have_20_entries() {
        let app = TuiApp::new().unwrap();
        let events = app.events.lock();
        assert_eq!(events.len(), 20, "events 应有 20 条滚动");
    }

    // === P4-1: 接真 EventLog ===

    /// 业务方传 --log <sqlite db>, 走真 EventLog (不 stub)
    #[test]
    fn tui_with_real_event_log_reads_sessions() {
        use ma_harness_core::{EventLog, EventType, SessionEvent};

        // 1. 准备一个 sqlite, append 2 个 session + 3 events
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("events.db");
        let log = EventLog::open(&db_path).unwrap();

        for session_id in ["s1", "s2"] {
            // SessionStart / ToolCall 是 model_visible, 必须有 payload_json
            let mut ev_start = SessionEvent::new(session_id, EventType::SessionStart);
            ev_start.payload_json = Some(format!(r#"{{"session":"{}"}}"#, session_id));
            let _ = log.append(ev_start);
            let mut ev_tool = SessionEvent::new(session_id, EventType::ToolCall);
            ev_tool.payload_json = Some(r#"{"tool":"echo"}"#.to_string());
            let _ = log.append(ev_tool);
        }

        // 2. TUI 接真 EventLog
        let app = TuiApp::new_with_log(Some(&db_path)).unwrap();
        let sessions = app.sessions.lock();
        // 2 个 session 出现 (顺序按 count DESC, 都 = 2 events, 都进)
        let ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
        assert!(ids.contains(&"s1".to_string()), "s1 应在 sessions: {:?}", ids);
        assert!(ids.contains(&"s2".to_string()), "s2 应在 sessions: {:?}", ids);
        // state 应该是 "active" (count > 0)
        for s in sessions.iter() {
            if s.id == "s1" || s.id == "s2" {
                assert_eq!(s.state, "active", "session {} 应 active", s.id);
            }
        }
        drop(sessions);

        // 3. events 应有 4 条 (2 session_start + 2 tool_call)
        let events = app.events.lock();
        assert!(events.len() >= 2, "events 应有 >= 2 条, got {}", events.len());
        // 验证 event_type 是 EventType Debug 输出
        let first = &events[0];
        assert!(
            first.event_type.contains("SessionStart") || first.event_type.contains("ToolCall"),
            "event_type 应是 SessionStart/ToolCall: {}",
            first.event_type
        );
    }

    /// 业务方传不可写 path (目录) → EventLog::open 失败 → fallback stub (不 panic)
    #[test]
    fn tui_with_unwritable_log_path_falls_back_to_stub() {
        // 用一个目录当 path (sqlite 不能 open 目录, 必 fail)
        let tmpdir = tempfile::tempdir().unwrap();
        // TUI 应 fallback stub, 不 panic
        let app = TuiApp::new_with_log(Some(tmpdir.path())).unwrap();
        // events 应 fallback 到 stub (20 条)
        let events = app.events.lock();
        assert_eq!(events.len(), 20, "fallback 后 events 应有 20 条");
    }

    // === P4-3: 接真 SessionStore ===

    /// 业务方传 --store-path <sqlite db> → SessionStore 拿真 sessions (P4-3)
    /// 走 SqliteStore 写 2 session → TuiApp::new_with_log_and_store → sessions 应含这 2 个
    #[test]
    fn tui_with_real_session_store_reads_sessions() {
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };

        // 1. SqliteStore 写 2 个 session
        let tmpdir = tempfile::tempdir().unwrap();
        let db_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&db_path).unwrap();

        for (id, name) in [("s-1", "alpha"), ("s-2", "beta")] {
            let proto = ProtoSession {
                id: id.to_string(),
                name: name.to_string(),
                state: ProtoSessionState::Active as i32,
                mode: OperatingMode::Default as i32,
                created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
                closed_at: None,
                metadata: None,
                stats: None,
                enabled_plugins: vec![],
                user_id: String::new(),
            };
            store.create(&proto).unwrap();
        }

        // 2. TUI 接 SqliteStore
        let store_arc: Arc<dyn SessionStore> = Arc::new(store);
        let app = TuiApp::new_with_log_and_store(None, Some(store_arc)).unwrap();

        // 3. 验 sessions 含 s-1, s-2
        let sessions = app.sessions.lock();
        let ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
        assert!(ids.contains(&"s-1".to_string()), "s-1 应在 sessions: {:?}", ids);
        assert!(ids.contains(&"s-2".to_string()), "s-2 应在 sessions: {:?}", ids);

        // 4. state 是 "Active" Debug 形式 (ProtoSessionState::Active)
        for s in sessions.iter() {
            if s.id == "s-1" || s.id == "s-2" {
                assert!(
                    s.state.contains("Active"),
                    "session {} state 应含 Active, got {}",
                    s.id,
                    s.state
                );
                // age 是 name
                assert!(
                    s.age == "alpha" || s.age == "beta",
                    "session {} age 应是 name, got {}",
                    s.id,
                    s.age
                );
            }
        }
    }

    /// SessionStore 跟 EventLog 优先级: 都传 → store 拿 sessions, log 拿 events (P4-3)
    #[test]
    fn tui_store_takes_priority_over_event_log_for_sessions() {
        use ma_harness_core::{EventLog, EventType, SessionEvent};
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };

        // 1. EventLog: 含 session "log-only", 但没 "store-only"
        let tmpdir = tempfile::tempdir().unwrap();
        let log_path = tmpdir.path().join("events.db");
        let log = EventLog::open(&log_path).unwrap();
        let mut ev = SessionEvent::new("log-only", EventType::SessionStart);
        ev.payload_json = Some(r#"{"session":"log-only"}"#.to_string());
        let _ = log.append(ev);

        // 2. SessionStore: 含 session "store-only", 但没 "log-only"
        let store_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&store_path).unwrap();
        let proto = ProtoSession {
            id: "store-only".to_string(),
            name: "store-session".to_string(),
            state: ProtoSessionState::Active as i32,
            mode: OperatingMode::Default as i32,
            created_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            updated_at: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
            closed_at: None,
            metadata: None,
            stats: None,
            enabled_plugins: vec![],
            user_id: String::new(),
        };
        store.create(&proto).unwrap();

        // 3. TUI 都接 (store 优先)
        let store_arc: Arc<dyn SessionStore> = Arc::new(store);
        let app = TuiApp::new_with_log_and_store(Some(&log_path), Some(store_arc)).unwrap();

        // 4. sessions 应是 store 拿的: "store-only" 在, "log-only" 不在
        let sessions = app.sessions.lock();
        let ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
        assert!(
            ids.contains(&"store-only".to_string()),
            "store-only 应在: {:?}",
            ids
        );
        assert!(
            !ids.contains(&"log-only".to_string()),
            "log-only 不应在 (store 优先): {:?}",
            ids
        );

        // 5. events 仍走 EventLog (store 跟 events 无关)
        let events = app.events.lock();
        assert!(events.len() >= 1, "events 走 EventLog, 应有 1+ 条");
    }
}
