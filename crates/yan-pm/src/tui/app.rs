use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::cli::dashboard::{self, DashboardData};
use crate::config::workspace::list_all_workspace_links;
use crate::daemon::event_store::EventStore;
use crate::daemon::pid;

/// TUI application state.
pub struct App {
    pub data: DashboardData,
    pub selected: usize,
    pub expanded: HashSet<usize>,
    pub should_quit: bool,
    event_store: Option<Arc<EventStore>>,
}

impl App {
    pub fn new(event_store: Option<Arc<EventStore>>) -> Self {
        let data = Self::collect(&event_store);
        Self {
            data,
            selected: 0,
            expanded: HashSet::new(),
            should_quit: false,
            event_store,
        }
    }

    /// Refresh dashboard data from EventStore + workspace links.
    pub fn refresh(&mut self) {
        self.data = Self::collect(&self.event_store);
        // Clamp selection to valid range
        if !self.data.workspaces.is_empty() && self.selected >= self.data.workspaces.len() {
            self.selected = self.data.workspaces.len() - 1;
        }
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

    pub fn handle_key(&mut self, key: KeyCode) {
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
                if self.expanded.contains(&self.selected) {
                    self.expanded.remove(&self.selected);
                } else {
                    self.expanded.insert(self.selected);
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
                self.refresh();
                last_tick = Instant::now();
            }

            if self.should_quit {
                return Ok(());
            }
        }
    }
}
