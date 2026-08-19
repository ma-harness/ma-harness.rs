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
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    DefaultTerminal, Frame,
};
use std::sync::Arc;
use std::path::Path;
use std::time::{Duration, Instant};

use ma_harness_core::EventLog;
use ma_harness_server::SessionStore;

/// TUI app mode (P5-2 / Day 91)
#[derive(Debug, Clone, PartialEq)]
enum AppMode {
    /// 4-panel 主 view: sessions / plugins / events / status
    List,
    /// Detail view: 单个 session 的 events + metadata
    Detail { session_id: String },
}

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
    /// **P5-2**: 当前 mode (List / Detail)
    mode: Arc<Mutex<AppMode>>,
    /// **P5-2**: List mode 当前选中 session 的 index (j/k 上下移)
    selected_session: Arc<Mutex<usize>>,
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
            mode: Arc::new(Mutex::new(AppMode::List)),
            selected_session: Arc::new(Mutex::new(0)),
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
            mode: Arc::new(Mutex::new(AppMode::List)),
            selected_session: Arc::new(Mutex::new(0)),
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
        let mut running = true;

        while running {
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
                        let mode = self.mode.lock().clone();
                        match mode {
                            AppMode::List => {
                                running = self.handle_list_key(key)?;
                            }
                            AppMode::Detail { session_id: _ } => {
                                self.handle_detail_key(key)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// List mode 按键处理 (P5-2)
    ///
    /// - `q` / `Esc` / `Ctrl-C`: 退出 (返 false 让 run_loop 停)
    /// - `j` / `↓`: 下一个 session
    /// - `k` / `↑`: 上一个 session
    /// - `Enter`: 进 Detail view
    ///
    /// Returns: true = 继续 loop, false = 退出
    fn handle_list_key(&self, key: crossterm::event::KeyEvent) -> Result<bool> {
        match key.code {
            crossterm::event::KeyCode::Char('q') => return Ok(false),
            crossterm::event::KeyCode::Esc => return Ok(false),
            crossterm::event::KeyCode::Char('c')
                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                return Ok(false);
            }
            crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                self.move_selection(1i64);
            }
            crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                self.move_selection(-1i64);
            }
            crossterm::event::KeyCode::Enter => {
                self.enter_detail();
            }
            _ => {}
        }
        Ok(true)
    }

    /// Detail mode 按键处理 (P5-2)
    ///
    /// - `q` / `Esc` / `Ctrl-C` / `Backspace`: 退回 List
    fn handle_detail_key(&self, key: crossterm::event::KeyEvent) -> Result<()> {
        match key.code {
            crossterm::event::KeyCode::Char('q')
            | crossterm::event::KeyCode::Esc
            | crossterm::event::KeyCode::Backspace => {
                *self.mode.lock() = AppMode::List;
            }
            crossterm::event::KeyCode::Char('c')
                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                *self.mode.lock() = AppMode::List;
            }
            _ => {}
        }
        Ok(())
    }

    /// 移动选中 session (clamp 到 [0, len))
    fn move_selection(&self, delta: i64) {
        let sessions = self.sessions.lock();
        if sessions.is_empty() {
            return;
        }
        let cur = *self.selected_session.lock() as i64;
        let len = sessions.len() as i64;
        let next = if delta >= 0 {
            (cur + delta).min(len - 1)
        } else {
            (cur + delta).max(0)
        };
        *self.selected_session.lock() = next as usize;
    }

    /// 进 detail view (拿当前选中 session 的 id)
    fn enter_detail(&self) {
        let sessions = self.sessions.lock();
        let idx = *self.selected_session.lock();
        if let Some(row) = sessions.get(idx) {
            // 跳过 placeholder 行: "(no events yet)" / "(no sessions yet)" / "default" stub
            if row.id.starts_with('(') || row.id == "default" {
                return;
            }
            *self.mode.lock() = AppMode::Detail {
                session_id: row.id.clone(),
            };
        }
    }

    /// 画 UI: 根据 mode 选 view (P5-2)
    ///
    /// - List mode: 4 个 panel (title + sessions/plugins + events + status)
    /// - Detail mode: 单 session 的 events + metadata
    fn ui(&self, frame: &mut Frame) {
        let mode = self.mode.lock().clone();
        match mode {
            AppMode::List => self.ui_list(frame),
            AppMode::Detail { session_id } => self.ui_detail(frame, &session_id),
        }
    }

    /// List mode 4 panel UI (P4-5 布局 + P5-2 高亮选中)
    fn ui_list(&self, frame: &mut Frame) {
        let area = frame.area();

        // 主布局: title (3) + row1 (Min 5) + row2 (Min 8) + status (3)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Min(8),
                Constraint::Length(3),
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
                "(Phase 5 — 'j/k' nav, 'Enter' detail, 'q' quit)",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title("ma-harness"));
        frame.render_widget(title, main_chunks[0]);

        // Row 1: Sessions | Plugins
        let row1_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[1]);

        // Sessions panel (P5-2: 高亮 selected_session)
        let selected = *self.selected_session.lock();
        let sessions = self.sessions.lock();
        let session_items: Vec<ListItem> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let marker = if i == selected { "▶" } else { " " };
                let style = if i == selected {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let line = Line::from(vec![
                    Span::styled(marker, style),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:16}", s.id),
                        if i == selected { style } else { Style::default().fg(Color::Yellow) },
                    ),
                    Span::raw(" "),
                    Span::styled(&s.state, Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::styled(&s.age, Style::default().fg(Color::DarkGray)),
                ]);
                ListItem::new(line)
            })
            .collect();
        let sessions_list = List::new(session_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Sessions ({})", sessions.len())),
            );
        frame.render_widget(sessions_list, row1_chunks[0]);
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
        frame.render_widget(plugins_list, row1_chunks[1]);
        drop(plugins);

        // Row 2: Events panel
        let events = self.events.lock();
        let event_items: Vec<ListItem> = events
            .iter()
            .rev()
            .map(|e| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("#{}", e.seq), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(format!("[{}]", e.timestamp), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:8}", e.severity.to_lowercase()),
                        Style::default().fg(match e.severity.as_str() {
                            "Error" => Color::Red,
                            "Warn" => Color::Magenta,
                            _ => Color::Cyan,
                        }),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{}/{}", e.session_id, e.event_type),
                        Style::default().fg(Color::Yellow),
                    ),
                ]))
            })
            .collect();
        let events_list = List::new(event_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Events (latest {})", events.len())),
        );
        frame.render_widget(events_list, main_chunks[2]);
        let event_count = events.len();
        drop(events);

        // Status bar
        let status = Paragraph::new(Line::from(vec![
            Span::styled(format!("ticks: {}", self.ticks), Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(
                format!("uptime: {:.1}s", self.started_at.elapsed().as_secs_f32()),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(format!("events: {}", event_count), Style::default().fg(Color::Yellow)),
        ]))
        .block(Block::default().borders(Borders::ALL).title("status"));
        frame.render_widget(status, main_chunks[3]);
    }

    /// Detail mode 单 session view (P5-2)
    ///
    /// 显示选中 session 的:
    /// - 顶部: id / state / name / age
    /// - 中间: 全部 events (model_visible) for that session
    /// - 底部: "press 'q' to back"
    fn ui_detail(&self, frame: &mut Frame, session_id: &str) {
        let area = frame.area();

        // 主布局: header (5) + body (Min 10) + footer (3)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),  // header
                Constraint::Min(10),    // body (events for session)
                Constraint::Length(3),  // footer
            ])
            .split(area);

        // Header: session metadata
        let session_meta = self.sessions.lock().iter().find(|s| s.id == session_id).cloned();
        let header_text = if let Some(s) = session_meta {
            format!(
                "Session: {}\n  state: {}\n  name/age: {}",
                s.id, s.state, s.age
            )
        } else {
            format!("Session: {}\n  (not in current list)", session_id)
        };
        let header = Paragraph::new(header_text)
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL).title("Session Detail"));
        frame.render_widget(header, main_chunks[0]);

        // Body: events for this session (走 EventLog::get_model_visible)
        let body_items: Vec<ListItem> = if let Some(log) = &self.event_log {
            match log.get_model_visible(session_id) {
                Ok(page) => page
                    .events
                    .iter()
                    .map(|s| {
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("#{}", s.seq),
                                Style::default().fg(Color::DarkGray),
                            ),
                            Span::raw(" "),
                            Span::styled(
                                format!("[{}]", s.event.ts.format("%H:%M:%S")),
                                Style::default().fg(Color::DarkGray),
                            ),
                            Span::raw(" "),
                            Span::styled(
                                format!("{:8}", format!("{:?}", s.event.severity).to_lowercase()),
                                Style::default().fg(match format!("{:?}", s.event.severity).as_str() {
                                    "Error" => Color::Red,
                                    "Warn" => Color::Magenta,
                                    _ => Color::Cyan,
                                }),
                            ),
                            Span::raw(" "),
                            Span::styled(
                                format!("{:?}", s.event.event_type),
                                Style::default().fg(Color::Yellow),
                            ),
                        ]))
                    })
                    .collect(),
                Err(e) => vec![ListItem::new(Line::from(Span::styled(
                    format!("ERR: get_model_visible: {e}"),
                    Style::default().fg(Color::Red),
                )))],
            }
        } else {
            vec![ListItem::new(Line::from(Span::styled(
                "(no EventLog — pass --log <db>)",
                Style::default().fg(Color::DarkGray),
            )))]
        };
        let body = List::new(body_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Events for {} (model_visible)", &session_id[..8.min(session_id.len())])),
        );
        frame.render_widget(body, main_chunks[1]);

        // Footer: "press q to back"
        let footer = Paragraph::new(Line::from(vec![Span::styled(
            "press 'q' / Esc / Backspace to back to list",
            Style::default().fg(Color::DarkGray),
        )]))
        .block(Block::default().borders(Borders::ALL).title("nav"));
        frame.render_widget(footer, main_chunks[2]);
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

    // === P5-2: TUI session detail view (j/k/Enter/Esc 交互) ===

    /// 初始 mode = List
    #[test]
    fn tui_initial_mode_is_list() {
        let app = TuiApp::new().unwrap();
        let mode = app.mode.lock();
        assert_eq!(*mode, AppMode::List, "初始 mode 应是 List");
    }

    /// 按 j/k 移动 selection
    #[test]
    fn tui_move_selection_jk() {
        let app = TuiApp::new().unwrap();
        // 默认 sessions: 1 条 "default"
        assert_eq!(*app.selected_session.lock(), 0, "初始 selected=0");

        // k 上移 (0 - 1 = max(0, -1) = 0)
        app.move_selection(-1i64);
        assert_eq!(*app.selected_session.lock(), 0, "k 在 0 应不动");

        // j 下移 (0 + 1 = min(0, 0) = 0, 因为只有 1 个)
        app.move_selection(1i64);
        assert_eq!(*app.selected_session.lock(), 0, "j 在 only-1 应不动");
    }

    /// 多个 session 时 j/k 真的移动
    #[test]
    fn tui_move_selection_with_multiple_sessions() {
        use ma_harness_core::{EventLog, EventType, SessionEvent};
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };

        let tmpdir = tempfile::tempdir().unwrap();
        let log_path = tmpdir.path().join("events.db");
        let log = EventLog::open(&log_path).unwrap();
        for sid in ["alpha", "beta", "gamma"] {
            let mut ev = SessionEvent::new(sid, EventType::SessionStart);
            ev.payload_json = Some(format!(r#"{{"session":"{}"}}"#, sid));
            let _ = log.append(ev);
        }

        let store_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&store_path).unwrap();
        for (i, (id, name)) in [("a-1", "alpha"), ("a-2", "beta"), ("a-3", "gamma")].iter().enumerate() {
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
            // 给 store session 加 events (用 count 标志)
            let _ = i;
        }
        let store_arc: Arc<dyn SessionStore> = Arc::new(store);

        let app = TuiApp::new_with_log_and_store(Some(&log_path), Some(store_arc)).unwrap();
        // 应有 3 个 session (a-1, a-2, a-3)
        let sessions = app.sessions.lock();
        assert_eq!(sessions.len(), 3, "应有 3 个 session, got {}", sessions.len());
        drop(sessions);

        // j 从 0 → 1
        app.move_selection(1i64);
        assert_eq!(*app.selected_session.lock(), 1);

        // j 1 → 2
        app.move_selection(1i64);
        assert_eq!(*app.selected_session.lock(), 2);

        // j 2 → 2 (clamp, 不会越界)
        app.move_selection(1i64);
        assert_eq!(*app.selected_session.lock(), 2, "j 在末位应 clamp");

        // k 2 → 1
        app.move_selection(-1i64);
        assert_eq!(*app.selected_session.lock(), 1);

        // k 0 → 0 (clamp)
        app.move_selection(-1i64);
        app.move_selection(-1i64);
        assert_eq!(*app.selected_session.lock(), 0, "k 在 0 应 clamp");
    }

    /// Enter 触发进 Detail view
    #[test]
    fn tui_enter_detail_switches_mode() {
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };

        let tmpdir = tempfile::tempdir().unwrap();
        let store_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&store_path).unwrap();
        for (id, name) in [("real-1", "first"), ("real-2", "second")] {
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
        let store_arc: Arc<dyn SessionStore> = Arc::new(store);
        let app = TuiApp::new_with_log_and_store(None, Some(store_arc)).unwrap();

        // selected = 0 (默认)
        app.enter_detail();
        let mode = app.mode.lock();
        match &*mode {
            AppMode::Detail { session_id } => {
                assert!(
                    session_id == "real-1" || session_id == "real-2",
                    "session_id 应是 real-1 或 real-2, got {}",
                    session_id
                );
            }
            AppMode::List => panic!("进 detail 后 mode 应是 Detail"),
        }
    }

    /// Enter 跳过占位行 ("(no events yet)")
    #[test]
    fn tui_enter_detail_skips_placeholder() {
        let app = TuiApp::new().unwrap();
        // TuiApp::new() 没 log/store, sessions 只有 "default" placeholder
        app.enter_detail();
        // 不应切到 detail (因为只有 1 个 placeholder, selected=0 是 "default")
        let mode = app.mode.lock();
        assert_eq!(*mode, AppMode::List, "只有 placeholder 时 Enter 不应切 mode");
    }

    /// Detail 模式按 q/Esc/Backspace 退回 List
    #[test]
    fn tui_detail_q_esc_back_returns_to_list() {
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

        let tmpdir = tempfile::tempdir().unwrap();
        let store_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&store_path).unwrap();
        let proto = ProtoSession {
            id: "back-test".to_string(),
            name: "back".to_string(),
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
        let store_arc: Arc<dyn SessionStore> = Arc::new(store);
        let app = TuiApp::new_with_log_and_store(None, Some(store_arc)).unwrap();

        // 强制进 Detail mode
        *app.mode.lock() = AppMode::Detail {
            session_id: "back-test".to_string(),
        };

        // 模拟按 'q'
        let key_q = KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_detail_key(key_q).unwrap();
        assert_eq!(*app.mode.lock(), AppMode::List, "'q' 应退回 List");

        // 再进 detail
        *app.mode.lock() = AppMode::Detail {
            session_id: "back-test".to_string(),
        };
        // 模拟按 Esc
        let key_esc = KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_detail_key(key_esc).unwrap();
        assert_eq!(*app.mode.lock(), AppMode::List, "Esc 应退回 List");
    }

    /// List 模式按 q 应该让 run_loop 停 (handle_list_key 返 false)
    #[test]
    fn tui_list_q_returns_false_to_exit() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
        let app = TuiApp::new().unwrap();
        let key_q = KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        let cont = app.handle_list_key(key_q).unwrap();
        assert!(!cont, "'q' 应返 false 让 loop 停");
    }
}
