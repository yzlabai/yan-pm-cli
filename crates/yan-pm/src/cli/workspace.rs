use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::api::client::{RegisterWorkspaceData, TaskListParams};
use crate::config;
use crate::local::directory::{LocalDirectory, LocalWorkspaceConfig};
use crate::output;
use super::make_client;

pub async fn link(url: Option<&str>, token: Option<&str>, project_id: &str, custom_path: Option<&str>, custom_name: Option<&str>) -> Result<()> {
    let client = make_client(url, token)?;
    let target_path = match custom_path {
        Some(p) => std::fs::canonicalize(p)
            .with_context(|| format!("无法解析路径: {p}"))?,
        None => std::env::current_dir()?,
    };
    let target_str = target_path.to_string_lossy().to_string();

    // Verify project exists
    let project = client.get_project(project_id).await?;

    // Register workspace on server
    let machine_id = config::get_machine_id();
    let ws_name = match custom_name {
        Some(n) => n.to_string(),
        None => format!("{}@{}", target_path.file_name().unwrap_or_default().to_string_lossy(), machine_id),
    };
    let data = RegisterWorkspaceData {
        name: ws_name,
        local_path: target_str.clone(),
        machine_id: machine_id.clone(),
        metadata: None,
    };
    let ws = client.register_workspace(&project.project.id, &data).await?;

    // Save local link
    config::save_workspace_link(&project.project.id, &target_str, Some(&ws.id))?;

    // Initialize .yan-pm/ directory
    let local_dir = LocalDirectory::new(&target_path);
    local_dir.init()?;
    local_dir.save_config(&LocalWorkspaceConfig {
        project_id: project.project.id.clone(),
        project_name: project.project.name.clone(),
        last_sync: None,
        auto_run: Default::default(),
    })?;

    println!(
        "{}",
        format!("✓ 已关联: {} → {}", target_path.display(), project.project.name).green()
    );

    // Full pull: fetch all tasks and write local files
    println!("{}", "⟳ 正在拉取任务文件...".cyan());
    let tasks = client
        .list_tasks(&project.project.id, &TaskListParams::default())
        .await?;
    let pull_result = local_dir.pull_tasks(&tasks)?;
    local_dir.save_config(&LocalWorkspaceConfig {
        project_id: project.project.id.clone(),
        project_name: project.project.name.clone(),
        last_sync: Some(chrono::Utc::now().to_rfc3339()),
        auto_run: Default::default(),
    })?;

    println!(
        "{}",
        format!("✓ {pull_result}").green()
    );

    Ok(())
}

pub async fn unlink(url: Option<&str>, token: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let cwd_str = cwd.to_string_lossy().to_string();

    if let Some(entry) = config::find_workspace_link(Some(Path::new(&cwd_str))) {
        // Remove workspace from server if we have both project_id and workspace_id
        if let Some(ws_id) = &entry.workspace_id {
            if let Ok(client) = super::make_client(url, token) {
                match client.remove_workspace(&entry.project_id, ws_id).await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("{}", format!("⚠ 服务端工作区移除失败（将继续取消本地关联）: {e}").yellow());
                    }
                }
            }
        }
        config::remove_workspace_link(&cwd_str)?;
        println!("{}", "✓ 已取消关联".green());
    } else {
        println!("{}", "当前目录未关联到任何项目".yellow());
    }
    Ok(())
}

pub async fn list(url: Option<&str>, token: Option<&str>, json: bool, project_id: &str) -> Result<()> {
    let client = make_client(url, token)?;
    let workspaces = client.list_workspaces(project_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&workspaces)?);
    } else {
        output::print_workspaces(&workspaces);
    }
    Ok(())
}

pub async fn info(url: Option<&str>, token: Option<&str>, json: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let cwd_str = cwd.to_string_lossy().to_string();

    let link = config::find_workspace_link(Some(Path::new(&cwd_str)));
    match link {
        Some(entry) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&entry)?);
            } else {
                println!("📁 路径: {}", entry.path);
                println!("📋 项目 ID: {}", entry.project_id);
                if let Some(ws_id) = &entry.workspace_id {
                    println!("🔗 工作区 ID: {}", ws_id);
                }
                println!("📅 关联时间: {}", entry.linked_at);

                // Try to fetch project details
                if let Ok(client) = super::make_client(url, token) {
                    if let Ok(project) = client.get_project(&entry.project_id).await {
                        println!("📋 项目名称: {}", project.project.name);
                    }
                }
            }
        }
        None => {
            if json {
                println!("null");
            } else {
                println!("{}", "当前目录未关联到任何项目。使用 `yan-pm link <projectId>` 关联。".yellow());
            }
        }
    }
    Ok(())
}
