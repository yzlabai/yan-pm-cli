use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::config::config_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub path: String,
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub linked_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WorkspacesFile {
    workspaces: Vec<WorkspaceEntry>,
}

fn workspaces_file() -> PathBuf {
    config_dir().join("workspaces.json")
}

fn load_workspaces_file() -> WorkspacesFile {
    let path = workspaces_file();
    if !path.exists() {
        return WorkspacesFile::default();
    }
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => WorkspacesFile::default(),
    }
}

fn save_workspaces_file(data: &WorkspacesFile) -> Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir).context("Failed to create config directory")?;
    let content = serde_json::to_string_pretty(data)? + "\n";
    let path = workspaces_file();
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn save_workspace_link(
    project_id: &str,
    local_path: &str,
    workspace_id: Option<&str>,
) -> Result<()> {
    let mut data = load_workspaces_file();
    let abs_path = fs::canonicalize(local_path)
        .unwrap_or_else(|_| PathBuf::from(local_path))
        .to_string_lossy()
        .to_string();

    if let Some(existing) = data.workspaces.iter_mut().find(|w| w.path == abs_path) {
        existing.project_id = project_id.to_string();
        existing.linked_at = chrono::Utc::now().to_rfc3339();
        if let Some(wid) = workspace_id {
            existing.workspace_id = Some(wid.to_string());
        }
    } else {
        data.workspaces.push(WorkspaceEntry {
            path: abs_path,
            project_id: project_id.to_string(),
            workspace_id: workspace_id.map(String::from),
            linked_at: chrono::Utc::now().to_rfc3339(),
        });
    }
    save_workspaces_file(&data)
}

pub fn remove_workspace_link(local_path: &str) -> Result<bool> {
    let mut data = load_workspaces_file();
    let abs_path = fs::canonicalize(local_path)
        .unwrap_or_else(|_| PathBuf::from(local_path))
        .to_string_lossy()
        .to_string();
    let len_before = data.workspaces.len();
    data.workspaces.retain(|w| w.path != abs_path);
    if data.workspaces.len() < len_before {
        save_workspaces_file(&data)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn find_workspace_link(cwd: Option<&Path>) -> Option<WorkspaceEntry> {
    let data = load_workspaces_file();
    let abs_path = if let Some(p) = cwd {
        fs::canonicalize(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .to_string()
    } else {
        std::env::current_dir()
            .ok()
            .and_then(|p| fs::canonicalize(&p).ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    data.workspaces.into_iter().find(|w| w.path == abs_path)
}

pub fn list_all_workspace_links() -> Vec<WorkspaceEntry> {
    load_workspaces_file().workspaces
}
