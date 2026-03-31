use std::path::Path;

use anyhow::Result;

use crate::config;
use crate::local::directory::LocalDirectory;
use crate::output;

pub async fn list_local(json: bool, issue_number: Option<i32>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let link = config::find_workspace_link(Some(Path::new(&cwd.to_string_lossy().as_ref())));

    if link.is_none() {
        anyhow::bail!("当前目录未关联到项目。请先运行 `yan-pm link <project>` 关联");
    }

    let local_dir = LocalDirectory::new(&cwd);
    if !local_dir.is_initialized() {
        println!("本地任务目录未初始化。请先运行 `yan-pm sync` 同步任务文件。");
        println!("提示: .yan-pm/tasks/ 目录不存在");
        return Ok(());
    }

    let mut tasks = local_dir.scan_tasks()?;

    // Filter by issue number if provided
    if let Some(num) = issue_number {
        tasks.retain(|t| t.frontmatter.issue == Some(num));
    }

    if json {
        let json_tasks: Vec<_> = tasks
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.frontmatter.id,
                    "number": t.frontmatter.number,
                    "title": t.frontmatter.title,
                    "type": t.frontmatter.task_type,
                    "priority": t.frontmatter.priority,
                    "status": t.frontmatter.status,
                    "issue": t.frontmatter.issue,
                    "tags": t.frontmatter.tags,
                    "file": t.file_path.display().to_string(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_tasks)?);
    } else {
        output::print_local_tasks(&tasks);
    }
    Ok(())
}
