use anyhow::Result;
use serde::Serialize;

use crate::config::workspace::{list_all_workspace_links, WorkspaceEntry};
use crate::daemon::event_store::{Event, EventStore};
use crate::daemon::pid;
use crate::local::directory::LocalDirectory;
use crate::output::format::{print_dashboard, print_dashboard_compact};

/// Collected data for a single workspace in the dashboard.
#[derive(Debug, Serialize)]
pub struct WorkspaceDashboard {
    pub name: String,
    pub path: String,
    pub project_id: String,
    pub project_name: Option<String>,
    pub auto_run_enabled: bool,
    pub auto_run_agent: Option<String>,
    pub auto_run_budget: Option<f64>,
    pub active_tasks: Vec<TaskExecution>,
    pub recent_completed: Vec<TaskExecution>,
}

/// An agent task execution (active or completed).
#[derive(Debug, Serialize)]
pub struct TaskExecution {
    pub task_id: String,
    pub agent: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub cost_usd: Option<f64>,
}

/// Full dashboard data.
#[derive(Debug, Serialize)]
pub struct DashboardData {
    pub daemon_running: bool,
    pub daemon_pid: Option<u32>,
    pub workspaces: Vec<WorkspaceDashboard>,
    pub summary: DashboardSummary,
}

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub total_workspaces: usize,
    pub running_tasks: usize,
    pub completed_tasks: usize,
    pub total_cost: f64,
}

pub fn parse_payload_field(payload: &str, field: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v.get(field).and_then(|f| f.as_str()).map(String::from))
}

fn parse_payload_f64(payload: &str, field: &str) -> Option<f64> {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v.get(field).and_then(|f| f.as_f64()))
}

fn event_to_execution(event: &Event, status: &str) -> TaskExecution {
    TaskExecution {
        task_id: event.task_id.clone(),
        agent: parse_payload_field(&event.payload, "agent"),
        title: parse_payload_field(&event.payload, "title"),
        status: status.to_string(),
        started_at: Some(event.created_at.clone()),
        completed_at: if status != "running" {
            Some(event.created_at.clone())
        } else {
            None
        },
        cost_usd: parse_payload_f64(&event.payload, "cost_usd"),
    }
}

pub fn collect_workspace_data(
    entry: &WorkspaceEntry,
    event_store: Option<&EventStore>,
) -> WorkspaceDashboard {
    let local_dir = LocalDirectory::new(std::path::Path::new(&entry.path));
    let config = local_dir.load_config();

    let project_name = config.as_ref().map(|c| c.project_name.clone());
    let auto_run_enabled = config
        .as_ref()
        .map(|c| c.auto_run.enabled)
        .unwrap_or(false);
    let auto_run_agent = config
        .as_ref()
        .filter(|c| c.auto_run.enabled)
        .map(|c| c.auto_run.agent.clone());
    let auto_run_budget = config
        .as_ref()
        .filter(|c| c.auto_run.enabled)
        .and_then(|c| c.auto_run.budget);

    // Collect execution data from event store
    let (active_tasks, recent_completed) = if let Some(store) = event_store {
        let active: Vec<TaskExecution> = store
            .query_active_tasks()
            .unwrap_or_default()
            .iter()
            .filter(|e| e.workspace_id == entry.workspace_id.as_deref().unwrap_or(""))
            .map(|e| event_to_execution(e, "running"))
            .collect();

        let completed: Vec<TaskExecution> = store
            .query_recent_completed(5)
            .unwrap_or_default()
            .iter()
            .filter(|e| e.workspace_id == entry.workspace_id.as_deref().unwrap_or(""))
            .map(|e| {
                let status = if e.event_type == "task_completed" {
                    "completed"
                } else {
                    "failed"
                };
                event_to_execution(e, status)
            })
            .collect();

        (active, completed)
    } else {
        (vec![], vec![])
    };

    // Derive workspace name from path
    let name = std::path::Path::new(&entry.path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| entry.project_id.clone());

    WorkspaceDashboard {
        name,
        path: entry.path.clone(),
        project_id: entry.project_id.clone(),
        project_name,
        auto_run_enabled,
        auto_run_agent,
        auto_run_budget,
        active_tasks,
        recent_completed,
    }
}

pub fn open_event_store() -> Option<EventStore> {
    let db_path = crate::config::config_dir().join("events.db");
    if db_path.exists() {
        EventStore::open(&db_path).ok()
    } else {
        None
    }
}

pub async fn run(json: bool, compact: bool, live: bool) -> Result<()> {
    // TUI live mode
    if live {
        let event_store = open_event_store().map(std::sync::Arc::new);
        return crate::tui::run_tui(event_store);
    }

    let workspace_entries = list_all_workspace_links();
    let daemon_pid = pid::check_running();
    let daemon_running = daemon_pid.is_some();

    let event_store = open_event_store();

    let mut workspaces = Vec::new();
    for entry in &workspace_entries {
        workspaces.push(collect_workspace_data(entry, event_store.as_ref()));
    }

    // Build summary
    let running_tasks: usize = workspaces.iter().map(|w| w.active_tasks.len()).sum();
    let completed_tasks: usize = workspaces.iter().map(|w| w.recent_completed.len()).sum();
    let total_cost: f64 = workspaces
        .iter()
        .flat_map(|w| w.active_tasks.iter().chain(w.recent_completed.iter()))
        .filter_map(|t| t.cost_usd)
        .sum();

    let data = DashboardData {
        daemon_running,
        daemon_pid,
        workspaces,
        summary: DashboardSummary {
            total_workspaces: workspace_entries.len(),
            running_tasks,
            completed_tasks,
            total_cost,
        },
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else if compact {
        print_dashboard_compact(&data);
    } else {
        print_dashboard(&data);
    }

    Ok(())
}
