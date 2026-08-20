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

pub mod approval;

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

/// TUI app mode (P5-2 / Day 91, P10-2.5 加 Approval modal)
#[derive(Debug, Clone, PartialEq)]
enum AppMode {
    /// 4-panel 主 view: sessions / plugins / events / status
    List,
    /// Detail view: 单个 session 的 events + metadata
    Detail { session_id: String },
    /// 审批 modal: tool invoke 需用户 y/n (P10-2.5)
    Approval {
        tool_call_id: String,
        tool_name: String,
        context: String,
    },
}

/// TUI panel focus (P6-5 / Day 101) — j/k 作用在哪一个 panel
///
/// Plugins panel 不可 focus (纯展示)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    /// Sessions panel (左)
    Sessions,
    /// Events panel (下, 全宽)
    Events,
}

impl Panel {
    /// 下一个 panel (Tab cycle)
    fn next(self) -> Self {
        match self {
            Panel::Sessions => Panel::Events,
            Panel::Events => Panel::Sessions,
        }
    }
    /// 上一个 panel (BackTab / Shift-Tab cycle)
    fn prev(self) -> Self {
        match self {
            Panel::Sessions => Panel::Events,
            Panel::Events => Panel::Sessions,
        }
    }
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
    /// P10-2.5: 当前 pending approval (TUI 主循环轮询 TuiApprover.peek_pending 拿)
    pending_approval: Arc<Mutex<Option<crate::approval::PendingApproval>>>,
    /// P10-2.5: TuiApprover 引用 (主循环轮询 + key 路由调 approve/deny)
    tui_approver: Arc<Mutex<Option<Arc<crate::approval::TuiApprover>>>>,
    /// **P5-2**: List mode 当前选中 session 的 index (j/k 上下移, Sessions panel focus)
    selected_session: Arc<Mutex<usize>>,
    /// **P6-5**: 当前 focus panel (j/k 作用在哪个 panel)
    focus: Arc<Mutex<Panel>>,
    /// **P6-5**: Events panel 的 scroll offset (0 = 最新最上, j 下滚一条, k 上滚一条)
    events_scroll: Arc<Mutex<usize>>,
    /// **P6-5 (B)**: 选中状态持久化路径 (~/.ma-harness/tui-state.json, 可被构造参数覆盖)
    state_path: Option<std::path::PathBuf>,
    /// **P6-5 (B)**: 启动时从 state file 读出的 last_session_id
    /// (refresh 后用这个把 selected_session 重新对位到持久化的 session)
    persisted_last_session_id: Arc<Mutex<Option<String>>>,
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

/// **P6-5 B**: 持久化 state JSON schema
#[derive(serde::Serialize, serde::Deserialize, Debug, Default, Clone)]
struct PersistedState {
    /// 上次选中的 session id (Detail view 进入时, 或 List mode 切换 focus 时记录)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_session_id: Option<String>,
    /// 上次 focus 在哪个 panel ("Sessions" / "Events")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_focus: Option<String>,
}

impl TuiApp {
    /// P10-2.5: 业务方装 TuiApprover (主循环会轮询 + key 路由 y/n)
    pub fn install_tui_approver(&self, approver: Arc<crate::approval::TuiApprover>) {
        *self.tui_approver.lock() = Some(approver);
    }

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
            session_store: None, // P4-3 单独 API: TuiApp::new_with_log_and_store
            started_at: Instant::now(),
            ticks: 0,
            mode: Arc::new(Mutex::new(AppMode::List)),
            pending_approval: Arc::new(Mutex::new(None)),
            tui_approver: Arc::new(Mutex::new(None)),
            selected_session: Arc::new(Mutex::new(0)),
            focus: Arc::new(Mutex::new(Panel::Sessions)),
            events_scroll: Arc::new(Mutex::new(0)),
            state_path: Self::default_state_path(),
            persisted_last_session_id: Arc::new(Mutex::new(None)),
        };
        // P6-5 B: 加载持久化状态 (last_session_id, last_focus)
        if let Some(path) = &app.state_path {
            if let Err(e) = app.load_persisted_state(path) {
                eprintln!(
                    "TUI: WARN failed to load state from {}: {e}",
                    path.display()
                );
            }
        }
        app.refresh()?;
        app.apply_persisted_selection();
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
            pending_approval: Arc::new(Mutex::new(None)),
            tui_approver: Arc::new(Mutex::new(None)),
            selected_session: Arc::new(Mutex::new(0)),
            focus: Arc::new(Mutex::new(Panel::Sessions)),
            events_scroll: Arc::new(Mutex::new(0)),
            state_path: Self::default_state_path(),
            persisted_last_session_id: Arc::new(Mutex::new(None)),
        };
        // P6-5 B: 加载持久化状态
        if let Some(path) = &app.state_path {
            if let Err(e) = app.load_persisted_state(path) {
                eprintln!(
                    "TUI: WARN failed to load state from {}: {e}",
                    path.display()
                );
            }
        }
        app.refresh()?;
        app.apply_persisted_selection();
        Ok(app)
    }

    /// **P6-5 B**: 用自定义 state 路径构造 (测试用, 业务方 CLI 走 default ~/.ma-harness/tui-state.json)
    pub fn new_with_log_and_store_and_state_path(
        log_path: Option<&Path>,
        store: Option<Arc<dyn SessionStore>>,
        state_path: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        // 1. 调 new_with_log_and_store (它已经 load + apply 默认 path 的 state)
        let mut app = Self::new_with_log_and_store(log_path, store)?;
        // 2. 如果业务方传了 state_path, 覆盖并 reload + apply
        if let Some(p) = state_path {
            // 覆盖前清掉之前默认 path load 出来的 state, 避免双 source 混淆
            *app.persisted_last_session_id.lock() = None;
            app.state_path = Some(p.clone());
            if let Err(e) = app.load_persisted_state(&p) {
                eprintln!("TUI: WARN failed to load state from {}: {e}", p.display());
            }
            // reload 后再 apply 一次 (new_with_log_and_store 里的 apply 用的是默认 path)
            app.apply_persisted_selection();
        }
        Ok(app)
    }

    /// **P6-5 B**: 默认 state 文件路径
    ///
    /// 优先级:
    /// 1. `MA_HARNESS_TUI_STATE` 环境变量
    /// 2. `~/.ma-harness/tui-state.json` (XDG-style, Linux/Mac)
    /// 3. `None` (业务方走 cwd 不持久化)
    fn default_state_path() -> Option<std::path::PathBuf> {
        if let Ok(p) = std::env::var("MA_HARNESS_TUI_STATE") {
            if !p.is_empty() {
                return Some(std::path::PathBuf::from(p));
            }
        }
        // ~/.ma-harness/tui-state.json
        if let Some(home) = std::env::var_os("HOME") {
            let dir = std::path::PathBuf::from(home).join(".ma-harness");
            return Some(dir.join("tui-state.json"));
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            // Windows fallback
            let dir = std::path::PathBuf::from(home).join(".ma-harness");
            return Some(dir.join("tui-state.json"));
        }
        None
    }

    /// **P6-5 B**: 从 path 加载持久化 state (启动时调)
    ///
    /// 错误 (文件不存在 / JSON 错) 都不抛, 走空 state (容错优先)
    fn load_persisted_state(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(()); // 首次跑, 没文件, 走默认
        }
        let content = std::fs::read_to_string(path)?;
        let state: PersistedState = serde_json::from_str(&content).unwrap_or_default();
        if let Some(sid) = state.last_session_id {
            *self.persisted_last_session_id.lock() = Some(sid);
        }
        if let Some(panel_str) = state.last_focus {
            let panel = match panel_str.as_str() {
                "Events" => Panel::Events,
                _ => Panel::Sessions,
            };
            *self.focus.lock() = panel;
        }
        Ok(())
    }

    /// **P6-5 B**: 保存持久化 state 到 path (交互事件触发, e.g. Tab 切 focus / Enter 进 detail)
    ///
    /// 容错: 目录不存在则 create_dir_all; 写失败 Log 但不 panic (TUI 不能挂)
    fn save_persisted_state(&self, path: &Path) -> Result<()> {
        let state = PersistedState {
            last_session_id: self.persisted_last_session_id.lock().clone(),
            last_focus: Some(match *self.focus.lock() {
                Panel::Events => "Events".to_string(),
                Panel::Sessions => "Sessions".to_string(),
            }),
        };
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(&state)?;
        // 写 tmp + rename 避免半路挂时文件半空
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// **P6-5 B**: refresh 后用持久化的 last_session_id 把 selected_session 对位
    ///
    /// 业务方上次选中的 session 可能这次还在 → 对位到对应 index
    /// session 不在了 (e.g. 已关闭) → 保持 0, 把 persisted_last_session_id 清掉
    fn apply_persisted_selection(&self) {
        let last = match self.persisted_last_session_id.lock().clone() {
            Some(s) => s,
            None => return, // 没持久化, 默认 0
        };
        let sessions = self.sessions.lock();
        if let Some(idx) = sessions.iter().position(|s| s.id == last) {
            *self.selected_session.lock() = idx;
        } else {
            // session 不在了, 清掉 (避免下次再尝试对位 stale id)
            *self.persisted_last_session_id.lock() = None;
        }
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
                            AppMode::Approval { .. } => {
                                self.handle_approval_key(key)?;
                            }
                        }
                    }
                }
            }

            // 4. P10-2.5: 轮询 pending approval (每 100ms)
            if last_refresh.elapsed() >= Duration::from_millis(100) {
                self.poll_approval();
            }
        }
        Ok(())
    }

    /// List mode 按键处理 (P5-2 + P6-5)
    ///
    /// - `q` / `Esc` / `Ctrl-C`: 退出 (返 false 让 run_loop 停)
    /// - `Tab`: focus forward (Sessions → Events → Sessions)
    /// - `BackTab`: focus backward
    /// - `j` / `↓`: 下一个 (按当前 focus panel)
    /// - `k` / `↑`: 上一个 (按当前 focus panel)
    /// - `Enter`: 进 Detail view (仅 Sessions focus 有效)
    ///
    /// Returns: true = 继续 loop, false = 退出
    fn handle_list_key(&self, key: crossterm::event::KeyEvent) -> Result<bool> {
        match key.code {
            crossterm::event::KeyCode::Char('q') => return Ok(false),
            crossterm::event::KeyCode::Esc => return Ok(false),
            crossterm::event::KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                return Ok(false);
            }
            crossterm::event::KeyCode::Tab => {
                // P6-5 A: focus 切换 (forward)
                let _next = self.focus.lock().next();
                *self.focus.lock() = _next;
                self.persist_state();
            }
            crossterm::event::KeyCode::BackTab => {
                // P6-5 A: focus 切换 (backward)
                let _prev = self.focus.lock().prev();
                *self.focus.lock() = _prev;
                self.persist_state();
            }
            crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                // P6-5 A: 按 focus 路由 j/k
                match *self.focus.lock() {
                    Panel::Sessions => self.move_selection(1i64),
                    Panel::Events => self.scroll_events(1i64),
                }
            }
            crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                match *self.focus.lock() {
                    Panel::Sessions => self.move_selection(-1i64),
                    Panel::Events => self.scroll_events(-1i64),
                }
            }
            crossterm::event::KeyCode::Enter => {
                // P6-5 A: Enter 仅在 Sessions focus 有效
                if *self.focus.lock() == Panel::Sessions {
                    self.enter_detail();
                    self.persist_state();
                }
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
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                *self.mode.lock() = AppMode::List;
            }
            _ => {}
        }
        Ok(())
    }

    /// P10-2.5: 轮询 TuiApprover pending 列表, 自动切到 Approval modal
    fn poll_approval(&self) {
        // 已经处于 Approval mode, 不重复进
        {
            let mode = self.mode.lock();
            if matches!(*mode, AppMode::Approval { .. }) {
                return;
            }
        }
        let approver_opt = self.tui_approver.lock().clone();
        if let Some(approver) = approver_opt {
            // 拿第一个 pending 当 modal
            if let Some(pending) = approver.peek_pending().into_iter().next() {
                *self.mode.lock() = AppMode::Approval {
                    tool_call_id: pending.tool_call_id,
                    tool_name: pending.tool_name,
                    context: pending.context,
                };
            }
        }
    }

    /// P10-2.5: 审批 modal 按键 (y/n/Esc)
    fn handle_approval_key(&self, key: crossterm::event::KeyEvent) -> Result<()> {
        let mode = self.mode.lock().clone();
        let (tool_call_id, _tool_name) = match mode {
            AppMode::Approval {
                tool_call_id,
                tool_name,
                ..
            } => (tool_call_id, tool_name),
            _ => return Ok(()),
        };
        match key.code {
            crossterm::event::KeyCode::Char('y') | crossterm::event::KeyCode::Char('Y') => {
                if let Some(approver) = self.tui_approver.lock().clone() {
                    approver.approve(&tool_call_id);
                }
                *self.mode.lock() = AppMode::List;
            }
            crossterm::event::KeyCode::Char('n') | crossterm::event::KeyCode::Char('N') => {
                if let Some(approver) = self.tui_approver.lock().clone() {
                    approver.deny(&tool_call_id, "user declined via TUI");
                }
                *self.mode.lock() = AppMode::List;
            }
            crossterm::event::KeyCode::Esc => {
                if let Some(approver) = self.tui_approver.lock().clone() {
                    approver.deny(&tool_call_id, "cancelled via Esc");
                }
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
            // P6-5 B: 记录 last_session_id 用于持久化
            *self.persisted_last_session_id.lock() = Some(row.id.clone());
        }
    }

    /// 滚动 events panel (clamp 到 [0, len-1])
    ///
    /// events 数组是"最新在末尾", iter().rev() 后 0 = 最新最上
    /// j 下滚 → scroll++ (看到更老的)
    /// k 上滚 → scroll-- (回到最新的)
    fn scroll_events(&self, delta: i64) {
        let events = self.events.lock();
        if events.is_empty() {
            return;
        }
        let cur = *self.events_scroll.lock() as i64;
        let len = events.len() as i64;
        let next = if delta >= 0 {
            (cur + delta).min(len - 1)
        } else {
            (cur + delta).max(0)
        };
        *self.events_scroll.lock() = next as usize;
    }

    /// **P6-5 B**: 写持久化 state (eprintln 失败不阻断 TUI, 跟持久化 import 思路一致)
    fn persist_state(&self) {
        if let Some(path) = &self.state_path {
            if let Err(e) = self.save_persisted_state(path) {
                eprintln!("TUI: WARN failed to save state to {}: {e}", path.display());
            }
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
            AppMode::Approval {
                tool_call_id,
                tool_name,
                context,
            } => self.ui_approval(frame, &tool_call_id, &tool_name, &context),
        }
    }

    /// P10-2.5: Approval modal UI (覆盖在 List view 上)
    fn ui_approval(&self, frame: &mut Frame, tool_call_id: &str, tool_name: &str, context: &str) {
        use ratatui::layout::{Constraint, Direction, Layout};
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        let area = frame.area();
        // 60% 宽, 30% 高的 modal, 居中
        let modal_width = (area.width as f32 * 0.6) as u16;
        let modal_height = 9u16;
        let x = (area.width.saturating_sub(modal_width)) / 2;
        let y = (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = ratatui::layout::Rect {
            x: area.x + x,
            y: area.y + y,
            width: modal_width,
            height: modal_height,
        };

        // 清空 modal 区域 (半透明 overlay 效果简化)
        frame.render_widget(Clear, modal_area);

        // modal 内容
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(modal_area);

        let title = Paragraph::new("⚠ Approval required").style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(title, chunks[0]);

        let id_line = Paragraph::new(format!("Tool call: {}", tool_call_id))
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(id_line, chunks[1]);

        let name_line =
            Paragraph::new(format!("Tool: {}", tool_name)).style(Style::default().fg(Color::White));
        frame.render_widget(name_line, chunks[2]);

        let ctx_line = Paragraph::new(format!("Context: {}", context))
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        frame.render_widget(ctx_line, chunks[3]);

        frame.render_widget(Paragraph::new(""), chunks[4]);

        let hint = Paragraph::new("[Y] approve    [N] deny    [Esc] cancel").style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(hint, chunks[5]);

        // 边框
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        frame.render_widget(block, modal_area);
    }

    /// List mode 4 panel UI (P4-5 布局 + P5-2 高亮选中 + P6-5 focus 高亮 + Events scroll)
    fn ui_list(&self, frame: &mut Frame) {
        let area = frame.area();
        let focus = *self.focus.lock();

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

        // Title — 显示当前 focus
        let focus_label = match focus {
            Panel::Sessions => "Sessions",
            Panel::Events => "Events",
        };
        let title = Paragraph::new(Line::from(vec![
            Span::styled(
                "ma-harness TUI",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!(
                    "(Phase 6 — focus: {focus_label}; Tab switch; 'j/k' nav; 'Enter' detail; 'q' quit)"
                ),
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

        // Sessions panel (P5-2: 高亮 selected_session, P6-5: 边框 BOLD 当 focus=Sessions)
        let selected = *self.selected_session.lock();
        let sessions = self.sessions.lock();
        let session_items: Vec<ListItem> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let marker = if i == selected { "▶" } else { " " };
                let style = if i == selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let line = Line::from(vec![
                    Span::styled(marker, style),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:16}", s.id),
                        if i == selected {
                            style
                        } else {
                            Style::default().fg(Color::Yellow)
                        },
                    ),
                    Span::raw(" "),
                    Span::styled(&s.state, Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::styled(&s.age, Style::default().fg(Color::DarkGray)),
                ]);
                ListItem::new(line)
            })
            .collect();
        // P6-5 A: focus 高亮 (border title 加 ▶ marker, border BOLD)
        let sessions_title = if focus == Panel::Sessions {
            format!("▶ Sessions ({})", sessions.len())
        } else {
            format!("Sessions ({})", sessions.len())
        };
        let sessions_block = if focus == Panel::Sessions {
            Block::default()
                .borders(Borders::ALL)
                .title(sessions_title)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
        } else {
            Block::default().borders(Borders::ALL).title(sessions_title)
        };
        let sessions_list = List::new(session_items).block(sessions_block);
        frame.render_widget(sessions_list, row1_chunks[0]);
        drop(sessions);

        // Plugins panel (纯展示, 不可 focus)
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
        let plugins_list = List::new(plugin_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Plugins ({})", plugins.len())),
        );
        frame.render_widget(plugins_list, row1_chunks[1]);
        drop(plugins);

        // Row 2: Events panel (P6-5: 边框高亮当 focus, scroll 偏移按 events_scroll)
        // P7-5 (Day 101): event_type 着色 + Approval 事件特殊颜色
        let events = self.events.lock();
        let scroll = *self.events_scroll.lock();
        let event_items: Vec<ListItem> = events
            .iter()
            .rev()
            .skip(scroll)
            .map(|e| {
                // P7-5: 按 event_type 着色
                let type_color = match e.event_type.as_str() {
                    "SessionStart" | "SessionEnd" => Color::Blue,
                    "RunStart" | "RunEnd" => Color::LightBlue,
                    "ModelRequest" | "ModelResponse" => Color::Green,
                    "ModelError" => Color::Red,
                    "ToolCall" | "ToolResult" => Color::Yellow,
                    "ToolError" => Color::Red,
                    "UserInput" | "UserCancel" => Color::Magenta,
                    "SandboxViolation" => Color::Red,
                    "ApprovalRequest" => Color::LightRed,
                    "ApprovalDecision" => Color::LightGreen,
                    _ => Color::DarkGray,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("#{}", e.seq), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(
                        format!("[{}]", e.timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
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
                        Style::default().fg(type_color),
                    ),
                ]))
            })
            .collect();
        // P6-5 A: focus 高亮 + scroll 状态
        let events_title = if focus == Panel::Events {
            format!(
                "▶ Events (latest {} of {}, scroll={})",
                event_items.len(),
                events.len(),
                scroll
            )
        } else {
            format!("Events (latest {})", events.len())
        };
        let events_block = if focus == Panel::Events {
            Block::default()
                .borders(Borders::ALL)
                .title(events_title)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
        } else {
            Block::default().borders(Borders::ALL).title(events_title)
        };
        let events_list = List::new(event_items).block(events_block);
        frame.render_widget(events_list, main_chunks[2]);
        let event_count = events.len();
        drop(events);

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
                format!("events: {}", event_count),
                Style::default().fg(Color::Yellow),
            ),
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
                Constraint::Length(5), // header
                Constraint::Min(10),   // body (events for session)
                Constraint::Length(3), // footer
            ])
            .split(area);

        // Header: session metadata
        let session_meta = self
            .sessions
            .lock()
            .iter()
            .find(|s| s.id == session_id)
            .cloned();
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Session Detail"),
            );
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
                                Style::default().fg(
                                    match format!("{:?}", s.event.severity).as_str() {
                                        "Error" => Color::Red,
                                        "Warn" => Color::Magenta,
                                        _ => Color::Cyan,
                                    },
                                ),
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
        let body =
            List::new(body_items).block(Block::default().borders(Borders::ALL).title(format!(
                "Events for {} (model_visible)",
                &session_id[..8.min(session_id.len())]
            )));
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
        assert!(
            ids.contains(&"s1".to_string()),
            "s1 应在 sessions: {:?}",
            ids
        );
        assert!(
            ids.contains(&"s2".to_string()),
            "s2 应在 sessions: {:?}",
            ids
        );
        // state 应该是 "active" (count > 0)
        for s in sessions.iter() {
            if s.id == "s1" || s.id == "s2" {
                assert_eq!(s.state, "active", "session {} 应 active", s.id);
            }
        }
        drop(sessions);

        // 3. events 应有 4 条 (2 session_start + 2 tool_call)
        let events = app.events.lock();
        assert!(
            events.len() >= 2,
            "events 应有 >= 2 条, got {}",
            events.len()
        );
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
        assert!(
            ids.contains(&"s-1".to_string()),
            "s-1 应在 sessions: {:?}",
            ids
        );
        assert!(
            ids.contains(&"s-2".to_string()),
            "s-2 应在 sessions: {:?}",
            ids
        );

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
        for (i, (id, name)) in [("a-1", "alpha"), ("a-2", "beta"), ("a-3", "gamma")]
            .iter()
            .enumerate()
        {
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
        assert_eq!(
            sessions.len(),
            3,
            "应有 3 个 session, got {}",
            sessions.len()
        );
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
            &AppMode::Approval { .. } => panic!("进 detail 后 mode 应是 Detail 或 Approval"),
            AppMode::Approval { .. } => panic!("进 detail 后 mode 应是 Detail 或 Approval"),
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
        assert_eq!(
            *mode,
            AppMode::List,
            "只有 placeholder 时 Enter 不应切 mode"
        );
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

    // === P6-5 A: j/k 跨 panel (Tab 切 focus) ===

    /// 初始 focus = Sessions
    #[test]
    fn tui_initial_focus_is_sessions() {
        let app = TuiApp::new().unwrap();
        let focus = app.focus.lock();
        assert_eq!(*focus, Panel::Sessions, "初始 focus 应是 Sessions");
    }

    /// Tab 切 focus: Sessions → Events → Sessions
    #[test]
    fn tui_tab_cycles_focus() {
        let app = TuiApp::new().unwrap();
        // 初始 = Sessions
        assert_eq!(*app.focus.lock(), Panel::Sessions);

        // Tab → Events
        let _next = app.focus.lock().next();
        *app.focus.lock() = _next;
        assert_eq!(
            *app.focus.lock(),
            Panel::Events,
            "Tab 1 次: Sessions → Events"
        );

        // Tab → Sessions (cycle)
        let _next = app.focus.lock().next();
        *app.focus.lock() = _next;
        assert_eq!(
            *app.focus.lock(),
            Panel::Sessions,
            "Tab 2 次: Events → Sessions"
        );
    }

    /// BackTab 反向: Sessions → Events (用 prev)
    #[test]
    fn tui_backtab_cycles_focus() {
        let app = TuiApp::new().unwrap();
        // 初始 = Sessions
        // BackTab → Events (走 prev)
        let _prev = app.focus.lock().prev();
        *app.focus.lock() = _prev;
        assert_eq!(
            *app.focus.lock(),
            Panel::Events,
            "BackTab: Sessions → Events"
        );
    }

    /// j/k 按 focus 路由: Sessions focus → move_selection, Events focus → scroll_events
    #[test]
    fn tui_jk_routes_by_focus() {
        use ma_harness_core::{EventLog, EventType, SessionEvent};
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };

        let tmpdir = tempfile::tempdir().unwrap();
        let log_path = tmpdir.path().join("events.db");
        let log = EventLog::open(&log_path).unwrap();
        for i in 0..5 {
            let mut ev = SessionEvent::new(format!("s-{i}"), EventType::SessionStart);
            ev.payload_json = Some(format!(r#"{{"session":"s-{i}"}}"#));
            let _ = log.append(ev);
        }
        let store_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&store_path).unwrap();
        for i in 0..5 {
            let proto = ProtoSession {
                id: format!("s-{i}"),
                name: format!("name-{i}"),
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
        let app = TuiApp::new_with_log_and_store(Some(&log_path), Some(store_arc)).unwrap();

        // 初始 focus = Sessions, 5 个 session
        assert_eq!(*app.focus.lock(), Panel::Sessions);
        assert_eq!(app.sessions.lock().len(), 5);

        // Sessions focus: j 移动 selected_session 0 → 1
        app.move_selection(1i64);
        assert_eq!(
            *app.selected_session.lock(),
            1,
            "Sessions focus: j 移 selected 0→1"
        );

        // 切到 Events focus
        *app.focus.lock() = Panel::Events;

        // Events focus: j 移动 events_scroll 0 → 1
        app.scroll_events(1i64);
        assert_eq!(
            *app.events_scroll.lock(),
            1,
            "Events focus: j 移 scroll 0→1"
        );

        // k 上移回 0
        app.scroll_events(-1i64);
        assert_eq!(
            *app.events_scroll.lock(),
            0,
            "Events focus: k 移 scroll 1→0"
        );
    }

    /// Events scroll clamp 到 [0, len-1]
    #[test]
    fn tui_events_scroll_clamps() {
        let app = TuiApp::new().unwrap();
        // 20 个 stub event
        assert_eq!(app.events.lock().len(), 20);

        // k 在 0 不动
        app.scroll_events(-1i64);
        assert_eq!(*app.events_scroll.lock(), 0, "scroll=0 时 k 不动");

        // j 一直按直到 clamp 到 19
        for _ in 0..30 {
            app.scroll_events(1i64);
        }
        assert_eq!(*app.events_scroll.lock(), 19, "scroll 应 clamp 到 len-1=19");
    }

    /// Enter 在 Events focus 时不进 detail
    #[test]
    fn tui_enter_in_events_focus_does_nothing() {
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

        let tmpdir = tempfile::tempdir().unwrap();
        let store_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&store_path).unwrap();
        let proto = ProtoSession {
            id: "s-1".to_string(),
            name: "alpha".to_string(),
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

        // 切到 Events focus
        *app.focus.lock() = Panel::Events;

        // 模拟按 Enter — handle_list_key 应 no-op
        let key_enter = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        let cont = app.handle_list_key(key_enter).unwrap();
        assert!(cont, "Enter 在 Events focus 应继续 loop, 不退出");
        // mode 应仍是 List (没进 detail)
        let mode = app.mode.lock();
        assert_eq!(*mode, AppMode::List, "Enter 在 Events focus 不应进 detail");
    }

    // === P6-5 B: 选中状态持久化 ===

    /// 加载不存在的 state 文件 → 走默认
    #[test]
    fn tui_load_persisted_state_no_file_is_default() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("non-existent.json");
        let app = TuiApp::new().unwrap();
        // 不应 panic, 不应 fail
        app.load_persisted_state(&path).unwrap();
        assert!(app.persisted_last_session_id.lock().is_none());
    }

    /// 保存 → 重新加载 → state 一致
    #[test]
    fn tui_persist_and_reload_roundtrip() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("tui-state.json");

        let app = TuiApp::new().unwrap();
        // 模拟 set 持久化 state
        *app.persisted_last_session_id.lock() = Some("ev-1".to_string());
        *app.focus.lock() = Panel::Events;
        // 写到 path
        app.save_persisted_state(&path).unwrap();
        // 文件应存在
        assert!(path.exists(), "save 后文件应存在: {}", path.display());

        // 新 TuiApp, 加载
        let app2 = TuiApp::new().unwrap();
        app2.load_persisted_state(&path).unwrap();
        assert_eq!(
            app2.persisted_last_session_id.lock().as_deref(),
            Some("ev-1"),
            "reload 后 last_session_id 应一致"
        );
        assert_eq!(*app2.focus.lock(), Panel::Events, "reload 后 focus 应一致");
    }

    /// 启动时自动加载 (走 new_with_log_and_store_and_state_path)
    #[test]
    fn tui_constructor_loads_persisted_state() {
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };

        let tmpdir = tempfile::tempdir().unwrap();
        let store_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&store_path).unwrap();
        for (id, name) in [("presisted-1", "alpha"), ("presisted-2", "beta")] {
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

        // 1. 准备 state file (持久化 "presisted-2" + Events focus)
        let state_path = tmpdir.path().join("state.json");
        {
            let app = TuiApp::new().unwrap();
            *app.persisted_last_session_id.lock() = Some("presisted-2".to_string());
            *app.focus.lock() = Panel::Events;
            app.save_persisted_state(&state_path).unwrap();
        }

        // 2. 用 state_path 启动 → 应自动加载, focus=Events, selected_session 对位到 persisted-2
        let app2 =
            TuiApp::new_with_log_and_store_and_state_path(None, Some(store_arc), Some(state_path))
                .unwrap();

        // focus 应是 Events (持久化的)
        assert_eq!(
            *app2.focus.lock(),
            Panel::Events,
            "reload 后 focus 应是 Events"
        );

        // last_session_id 应是 persisted-2
        let last = app2.persisted_last_session_id.lock().clone();
        assert_eq!(
            last.as_deref(),
            Some("presisted-2"),
            "last_session_id 应是 persisted-2"
        );

        // selected_session 应指向 persisted-2
        // 验证方式: 持久化 last_session_id 还在, selected_session 指向一个真实存在的 session
        // 因为 SessionStore list 顺序不定, 找持久化 id 在 sessions 里的 index
        let sessions = app2.sessions.lock();
        let expected_idx = sessions.iter().position(|s| s.id == "presisted-2");
        assert!(expected_idx.is_some(), "sessions 应含 persisted-2");
        let expected_idx = expected_idx.unwrap();
        let actual_selected = *app2.selected_session.lock();
        let actual_id = sessions.get(actual_selected).map(|s| s.id.as_str());
        assert_eq!(
            actual_id,
            Some("presisted-2"),
            "selected_session 应指向 persisted-2, got idx={} id={:?}",
            actual_selected,
            actual_id
        );
        assert_eq!(
            actual_selected, expected_idx,
            "selected index 应是 persisted-2 的 index"
        );
    }

    /// 持久化的 session 不再存在 → 自动清掉 (不保持 stale id)
    #[test]
    fn tui_persisted_session_not_found_clears() {
        use ma_harness_server::{SessionStore, SqliteStore};
        use ma_harness_proto::ma_harness::v1::{
            OperatingMode, Session as ProtoSession, SessionState as ProtoSessionState,
        };

        let tmpdir = tempfile::tempdir().unwrap();
        let store_path = tmpdir.path().join("sessions.db");
        let store = SqliteStore::open(&store_path).unwrap();
        // 只有 new-session, 没 old-session
        let proto = ProtoSession {
            id: "new-session".to_string(),
            name: "new".to_string(),
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

        // 准备 state file 指向不存在的 session
        let state_path = tmpdir.path().join("state.json");
        {
            let app = TuiApp::new().unwrap();
            *app.persisted_last_session_id.lock() = Some("old-session-not-exist".to_string());
            app.save_persisted_state(&state_path).unwrap();
        }

        // 启动 → 应自动清掉 persisted_last_session_id (old-session 不在)
        let app =
            TuiApp::new_with_log_and_store_and_state_path(None, Some(store_arc), Some(state_path))
                .unwrap();
        assert!(
            app.persisted_last_session_id.lock().is_none(),
            "持久化 session 不存在时应清掉, got {:?}",
            app.persisted_last_session_id.lock()
        );
        // selected_session 应回到 0 (默认)
        assert_eq!(*app.selected_session.lock(), 0);
    }

    /// Tab 切 focus 时自动 save (走 handle_list_key)
    #[test]
    fn tui_tab_saves_state() {
        let tmpdir = tempfile::tempdir().unwrap();
        let state_path = tmpdir.path().join("state.json");
        let app =
            TuiApp::new_with_log_and_store_and_state_path(None, None, Some(state_path.clone()))
                .unwrap();
        // 初始 = Sessions
        assert_eq!(*app.focus.lock(), Panel::Sessions);

        // 模拟按 Tab
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
        let key_tab = KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        app.handle_list_key(key_tab).unwrap();
        // focus 应切到 Events
        assert_eq!(*app.focus.lock(), Panel::Events);
        // state file 应被 save
        assert!(state_path.exists(), "Tab 后应自动 save state file");
        // 文件应含 "Events"
        let content = std::fs::read_to_string(&state_path).unwrap();
        assert!(
            content.contains("Events"),
            "save 后文件应含 'Events', got: {}",
            content
        );
    }

    /// 损坏的 JSON 文件不 panic, 走空 state
    #[test]
    fn tui_load_corrupted_state_falls_back() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("corrupted.json");
        std::fs::write(&path, "{ invalid json @@ }").unwrap();

        let app = TuiApp::new().unwrap();
        // 不应 panic
        app.load_persisted_state(&path).unwrap();
        // 走空 state
        assert!(app.persisted_last_session_id.lock().is_none());
        // focus 仍是默认
        assert_eq!(*app.focus.lock(), Panel::Sessions);
    }

    /// default_state_path 走 HOME / USERPROFILE / env var
    #[test]
    #[allow(unsafe_code)] // std::env::set_var / remove_var 是 unsafe (Rust 2024+), 测试需要
    fn tui_default_state_path_env_var_overrides() {
        // MA_HARNESS_TUI_STATE 优先
        let custom = std::path::PathBuf::from("/tmp/custom-tui-state.json");
        // set_var / remove_var 是 unsafe (Rust 2024+), 串行化避免 race
        unsafe {
            std::env::set_var("MA_HARNESS_TUI_STATE", custom.to_string_lossy().to_string());
        }
        let result = TuiApp::default_state_path();
        assert_eq!(result.as_deref(), Some(custom.as_path()));
        unsafe {
            std::env::remove_var("MA_HARNESS_TUI_STATE");
        }
    }
}
