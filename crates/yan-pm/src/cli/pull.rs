use std::path::Path;

use anyhow::Result;

use crate::api::client::IssueListParams;
use crate::config;
use crate::local::directory::LocalDirectory;

pub async fn handle_pull(url: Option<&str>, token: Option<&str>, json: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let link = config::find_workspace_link(Some(Path::new(&cwd.to_string_lossy().as_ref())))
        .ok_or_else(|| anyhow::anyhow!("当前目录未关联项目。请先运行: yan link <project>"))?;

    let api = super::make_client(url, token)?;

    let local_dir = LocalDirectory::new(&cwd);
    if !local_dir.is_initialized() {
        local_dir.init()?;
    }

    let params = IssueListParams::default();
    let issues = api.list_issues(&link.project_id, &params).await?;

    let result = local_dir.pull_issues(&issues)?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "created": result.created,
                "updated": result.updated,
                "unchanged": result.unchanged,
            })
        );
    } else {
        println!(
            "✓ Issue 同步完成: {} 新增, {} 更新, {} 未变",
            result.created, result.updated, result.unchanged
        );
    }
    Ok(())
}
