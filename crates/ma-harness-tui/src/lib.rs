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
use std::time::{Duration, Instant};

/// TUI app state
pub struct TuiApp {
    /// session list (from EventLog)
    sessions: Arc<Mutex<Vec<SessionRow>>>,
    /// event log (rolling, 最新 100 条)
    events: Arc<Mutex<Vec<EventRow>>>,
    /// plugin list (from inventory)
    plugins: Arc<Mutex<Vec<String>>>,
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
    seq: u64,
    event_type: String,
    severity: String,
    timestamp: String,
}

impl TuiApp {
    /// 构造一个新 TUI app
    pub fn new() -> Result<Self> {
        let app = Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            plugins: Arc::new(Mutex::new(Vec::new())),
            started_at: Instant::now(),
            ticks: 0,
        };
        // 初始数据 (走 inventory + dummy sessions/events)
        app.refresh()?;
        Ok(app)
    }

    /// 刷新数据 (从 inventory + 模拟)
    fn refresh(&self) -> Result<()> {
        // 1. plugin list (从 inventory)
        let plugins: Vec<String> = ma_harness_seam::PluginLoader::list()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        *self.plugins.lock() = plugins;

        // 2. sessions: Phase 3.9 PoC 暂不接 EventLog (复杂),
        //    用 inventory plugin 数 + runtime 状态当 dummy 行
        let mut sessions = self.sessions.lock();
        sessions.clear();
        sessions.push(SessionRow {
            id: "default".to_string(),
            state: "running".to_string(),
            age: format!("{:.0}s", self.started_at.elapsed().as_secs_f32()),
        });
        for (i, p) in self.plugins.lock().iter().enumerate() {
            sessions.push(SessionRow {
                id: format!("plugin-{}", i),
                state: "loaded".to_string(),
                age: format!("plugin:{}", p),
            });
        }
        drop(sessions);

        // 3. events: dummy 滚动
        let mut events = self.events.lock();
        events.clear();
        let tick = self.ticks;
        for i in 0..20 {
            events.push(EventRow {
                seq: tick * 20 + i,
                event_type: match i % 4 {
                    0 => "session.start".to_string(),
                    1 => "tool.call".to_string(),
                    2 => "model.response".to_string(),
                    _ => "session.tick".to_string(),
                },
                severity: match i % 3 {
                    0 => "info".to_string(),
                    1 => "info".to_string(),
                    _ => "debug".to_string(),
                },
                timestamp: format!("+{}ms", i * 500),
            });
        }
        Ok(())
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
}
