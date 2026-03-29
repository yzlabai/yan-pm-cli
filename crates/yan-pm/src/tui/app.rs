use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::cli::dashboard::{self, DashboardData};
use crate::config::workspace::list_all_workspace_links;
use crate::daemon::event_store::{self, EventStore};
use crate::daemon::pid;

/// Which screen the TUI is showing.
pub enum ViewMode {
    Dashboard,
    LogView,
}

/// State for the log viewer screen.
pub struct LogViewState {
    pub task_id: String,
    pub workspace_id: String,
    pub title: String,
    pub events: Vec<event_store::Event>,
    pub last_seq: Option<i64>,
    pub scroll_offset: u16,
    pub auto_scroll: bool,
    /// Search mode: Some(query) when active
    pub search: Option<String>,
    /// Actively typing search input
    pub search_input: Option<String>,
    /// Event type filter: None = show all
    pub filter: Option<String>,
}

/// TUI application state.
pub struct App {
    pub data: DashboardData,
    pub selected: usize,
    pub expanded: HashSet<usize>,
    pub should_quit: bool,
    pub mode: ViewMode,
    pub log_view: Option<LogViewState>,
    /// Transient status message shown in footer (auto-clears after a few ticks)
    pub status_message: Option<(String, Instant)>,
    event_store: Option<Arc<EventStore>>,
}

/// Max events kept in log view buffer (ring buffer behavior).
const MAX_LOG_EVENTS: usize = 1000;

impl App {
    pub fn new(event_store: Option<Arc<EventStore>>) -> Self {
        let data = Self::collect(&event_store);
        Self {
            data,
            selected: 0,
            expanded: HashSet::new(),
            should_quit: false,
            mode: ViewMode::Dashboard,
            log_view: None,
            status_message: None,
            event_store,
        }
    }

    /// Refresh dashboard data from EventStore + workspace links.
    pub fn refresh(&mut self) {
        self.data = Self::collect(&self.event_store);
        if !self.data.workspaces.is_empty() && self.selected >= self.data.workspaces.len() {
            self.selected = self.data.workspaces.len() - 1;
        }
    }

    /// Refresh log view: fetch incremental events.
    pub fn refresh_log(&mut self) {
        let Some(store) = &self.event_store else {
            return;
        };
        let Some(log) = &mut self.log_view else {
            return;
        };

        let new_events = store
            .query(&log.task_id, log.last_seq, 50)
            .unwrap_or_default();

        if !new_events.is_empty() {
            if let Some(last) = new_events.last() {
                log.last_seq = Some(last.id);
            }
            log.events.extend(new_events);
            // Ring buffer: trim oldest if over limit
            if log.events.len() > MAX_LOG_EVENTS {
                let excess = log.events.len() - MAX_LOG_EVENTS;
                log.events.drain(..excess);
            }
        }
    }

    /// Get filtered events for display.
    pub fn visible_log_events(&self) -> Vec<&event_store::Event> {
        let Some(log) = &self.log_view else {
            return vec![];
        };

        log.events
            .iter()
            .filter(|e| {
                // Apply type filter
                if let Some(filter) = &log.filter {
                    if e.event_type != *filter {
                        return false;
                    }
                }
                // Apply search filter
                if let Some(query) = &log.search {
                    if !e.payload.to_lowercase().contains(&query.to_lowercase())
                        && !e.event_type.to_lowercase().contains(&query.to_lowercase())
                    {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    fn collect(event_store: &Option<Arc<EventStore>>) -> DashboardData {
        let workspace_entries = list_all_workspace_links();
        let daemon_pid = pid::check_running();
        let daemon_running = daemon_pid.is_some();

        let store_ref = event_store.as_ref().map(|s| s.as_ref());
        let mut workspaces = Vec::new();
        for entry in &workspace_entries {
            workspaces.push(dashboard::collect_workspace_data(entry, store_ref));
        }

        let running_tasks: usize = workspaces.iter().map(|w| w.active_tasks.len()).sum();
        let completed_tasks: usize = workspaces.iter().map(|w| w.recent_completed.len()).sum();
        let total_cost: f64 = workspaces
            .iter()
            .flat_map(|w| w.active_tasks.iter().chain(w.recent_completed.iter()))
            .filter_map(|t| t.cost_usd)
            .sum();

        DashboardData {
            daemon_running,
            daemon_pid,
            workspaces,
            summary: dashboard::DashboardSummary {
                total_workspaces: workspace_entries.len(),
                running_tasks,
                completed_tasks,
                total_cost,
            },
        }
    }

    /// Enter log view for a specific task.
    fn enter_log_view(&mut self, task_id: &str, workspace_id: &str, title: &str) {
        self.log_view = Some(LogViewState {
            task_id: task_id.to_string(),
            workspace_id: workspace_id.to_string(),
            title: title.to_string(),
            events: vec![],
            last_seq: None,
            scroll_offset: 0,
            auto_scroll: true,
            search: None,
            search_input: None,
            filter: None,
        });
        self.mode = ViewMode::LogView;
        // Initial fetch
        self.refresh_log();
    }

    /// Export log events to a file.
    fn export_log(&self) -> Option<String> {
        let log = self.log_view.as_ref()?;
        let visible = self.visible_log_events();
        let filename = format!(
            "yan-pm-log-{}-{}.txt",
            &log.task_id[..8.min(log.task_id.len())],
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );

        let mut content = format!("# Task: {} ({})\n\n", log.title, log.task_id);
        for event in &visible {
            content.push_str(&format!(
                "[{}] {} | {}\n",
                event.created_at, event.event_type, event.payload
            ));
        }

        if std::fs::write(&filename, &content).is_ok() {
            Some(filename)
        } else {
            None
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        match &self.mode {
            ViewMode::Dashboard => self.handle_dashboard_key(key),
            ViewMode::LogView => self.handle_log_key(key),
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.data.workspaces.is_empty()
                    && self.selected < self.data.workspaces.len() - 1
                {
                    self.selected += 1;
                }
            }
            KeyCode::Enter => {
                // If workspace has active tasks, enter log view for first active task
                if let Some(ws) = self.data.workspaces.get(self.selected) {
                    if let Some(task) = ws.active_tasks.first() {
                        let task_id = task.task_id.clone();
                        let title = task.title.clone().unwrap_or_default();
                        // Derive workspace_id from the workspace entry
                        let ws_entries = list_all_workspace_links();
                        let ws_id = ws_entries
                            .get(self.selected)
                            .and_then(|e| e.workspace_id.clone())
                            .unwrap_or_default();
                        self.enter_log_view(&task_id, &ws_id, &title);
                    } else {
                        // No active tasks — toggle expand for completed tasks
                        if self.expanded.contains(&self.selected) {
                            self.expanded.remove(&self.selected);
                        } else {
                            self.expanded.insert(self.selected);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_log_key(&mut self, key: KeyCode) {
        // If in search input mode, handle text entry
        if let Some(log) = &mut self.log_view {
            if let Some(input) = &mut log.search_input {
                match key {
                    KeyCode::Enter => {
                        let query = input.clone();
                        log.search = if query.is_empty() { None } else { Some(query) };
                        log.search_input = None;
                        return;
                    }
                    KeyCode::Esc => {
                        log.search_input = None;
                        return;
                    }
                    KeyCode::Backspace => {
                        input.pop();
                        return;
                    }
                    KeyCode::Char(c) => {
                        input.push(c);
                        return;
                    }
                    _ => return,
                }
            }
        }

        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = ViewMode::Dashboard;
                self.log_view = None;
            }
            KeyCode::Char('r') => self.refresh_log(),
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(log) = &mut self.log_view {
                    log.auto_scroll = false;
                    log.scroll_offset = log.scroll_offset.saturating_add(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(log) = &mut self.log_view {
                    if log.scroll_offset > 0 {
                        log.scroll_offset -= 1;
                    } else {
                        log.auto_scroll = true;
                    }
                }
            }
            KeyCode::Char('G') => {
                // Jump to bottom
                if let Some(log) = &mut self.log_view {
                    log.scroll_offset = 0;
                    log.auto_scroll = true;
                }
            }
            KeyCode::Char('/') => {
                // Enter search mode
                if let Some(log) = &mut self.log_view {
                    log.search_input = Some(String::new());
                }
            }
            KeyCode::Char('f') => {
                // Cycle event type filter
                if let Some(log) = &mut self.log_view {
                    log.filter = match &log.filter {
                        None => Some("tool_call".to_string()),
                        Some(f) if f == "tool_call" => Some("agent_output".to_string()),
                        Some(f) if f == "agent_output" => Some("state_change".to_string()),
                        _ => None,
                    };
                }
            }
            KeyCode::Char('e') => {
                // Export log
                if let Some(filename) = self.export_log() {
                    self.status_message =
                        Some((format!("Exported to {}", filename), Instant::now()));
                } else {
                    self.status_message =
                        Some(("Export failed".to_string(), Instant::now()));
                }
            }
            _ => {}
        }
    }

    /// Main event loop: poll for keyboard events and tick every 1s.
    pub fn run_loop<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut ratatui::Terminal<B>,
    ) -> anyhow::Result<()> {
        let tick_rate = Duration::from_secs(1);
        let mut last_tick = Instant::now();

        loop {
            terminal.draw(|f| super::ui::render(f, self))?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code);
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                match &self.mode {
                    ViewMode::Dashboard => self.refresh(),
                    ViewMode::LogView => self.refresh_log(),
                }
                // Auto-clear status message after 3 seconds
                if let Some((_, created)) = &self.status_message {
                    if created.elapsed() > Duration::from_secs(3) {
                        self.status_message = None;
                    }
                }
                last_tick = Instant::now();
            }

            if self.should_quit {
                return Ok(());
            }
        }
    }
}
