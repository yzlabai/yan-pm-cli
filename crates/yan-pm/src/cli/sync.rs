use std::path::Path;

use anyhow::{bail, Result};
use colored::Colorize;

use crate::api::client::RegisterWorkspaceData;
use crate::config;

pub async fn run(url: Option<&str>, token: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let link = config::find_workspace_link(Some(Path::new(&cwd.to_string_lossy().as_ref())));

    let entry = match link {
        Some(e) => e,
        None => bail!("当前目录未关联到项目。请先运行 `yan-pm link <project>`"),
    };

    let client = super::make_client(url, token)?;

    // Send workspace heartbeat
    let machine_id = config::get_machine_id();
    let name = Path::new(&entry.path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());

    let data = RegisterWorkspaceData {
        name,
        local_path: entry.path.to_string(),
        machine_id,
        metadata: None,
    };

    match client.register_workspace(&entry.project_id, &data).await {
        Ok(ws) => {
            let _ = client
                .workspace_heartbeat(&entry.project_id, &ws.id, None)
                .await;
            println!(
                "{}",
                format!(
                    "✓ 工作区心跳已发送 (workspace={})",
                    &ws.id[..8.min(ws.id.len())]
                )
                .green()
            );
        }
        Err(e) => {
            eprintln!("{}", format!("⚠ 工作区心跳失败: {e}").yellow());
        }
    }

    println!(
        "{}",
        "提示: Task 同步已移除，任务现在仅通过本地文件管理".yellow()
    );

    Ok(())
}
