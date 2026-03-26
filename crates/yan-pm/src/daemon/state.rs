use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::config::config_dir;

/// Daemon runtime state, persisted to daemon.state for CLI queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonState {
    pub pid: u32,
    pub started_at: String,
    pub workspaces: Vec<DaemonWorkspaceState>,
}

/// Per-workspace state inside daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonWorkspaceState {
    pub path: String,
    pub project_id: String,
    pub last_sync: Option<String>,
    pub auto_run: bool,
}

fn state_file() -> PathBuf {
    config_dir().join("daemon.state")
}

impl DaemonState {
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            started_at: Utc::now().to_rfc3339(),
            workspaces: Vec::new(),
        }
    }

    /// Load state from disk.
    pub fn load() -> Option<Self> {
        let path = state_file();
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save state atomically.
    pub fn save(&self) -> Result<()> {
        let path = state_file();
        let content = serde_json::to_string_pretty(self)? + "\n";
        let tmp = path.with_extension("state.tmp");
        fs::write(&tmp, &content)?;
        fs::rename(&tmp, &path).context("Failed to write daemon.state")?;
        Ok(())
    }

    /// Remove the state file.
    pub fn remove() {
        let _ = fs::remove_file(state_file());
    }

    /// Update workspace sync timestamp.
    #[allow(dead_code)]
    pub fn update_workspace_sync(&mut self, path: &str) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| w.path == path) {
            ws.last_sync = Some(Utc::now().to_rfc3339());
        }
    }
}
