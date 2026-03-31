use std::path::Path;

use anyhow::Result;

use crate::config;
use crate::local::directory::LocalDirectory;
use crate::local::task_parser;
use crate::output;

pub async fn list_local(json: bool, issue_number: Option<i32>, regenerate: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let link = config::find_workspace_link(Some(Path::new(&cwd.to_string_lossy().as_ref())));

    if link.is_none() {
        anyhow::bail!("当前目录未关联到项目。请先运行 `yan link <project>` 关联");
    }

    let local_dir = LocalDirectory::new(&cwd);
    if !local_dir.is_initialized() {
        println!("本地任务目录未初始化。请先运行 `yan sync` 同步任务文件。");
        println!("提示: .yan-pm/tasks/ 目录不存在");
        return Ok(());
    }

    // Auto-generate tasks from spec if:
    // - issue_number is provided
    // - no tasks exist for this issue (or --regenerate is set)
    // - a spec exists for this issue
    if let Some(num) = issue_number {
        let has_tasks = local_dir.has_tasks_for_issue(num)?;

        if !has_tasks || regenerate {
            if let Some(spec) = local_dir.find_spec_by_issue(num)? {
                match task_parser::parse_tasks_from_spec(&spec.body) {
                    Ok(parsed) if !parsed.is_empty() => {
                        if regenerate && has_tasks {
                            // Remove existing tasks for this issue before regenerating
                            remove_tasks_for_issue(&local_dir, num)?;
                        }
                        let paths = local_dir.generate_tasks_from_spec(num, &parsed)?;
                        if !json {
                            println!("✓ 从 Spec 生成了 {} 个任务", paths.len());
                        }
                    }
                    Ok(_) => {
                        if !json {
                            println!("⚠ Spec 的 \"任务拆分\" 部分为空，请先编辑 Spec 添加任务");
                        }
                    }
                    Err(e) => {
                        if !json {
                            println!("⚠ 无法从 Spec 解析任务: {}", e);
                        }
                    }
                }
            }
        }
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
                    "depends_on": t.frontmatter.depends_on,
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

/// Remove all task files for a given issue number (used during --regenerate).
fn remove_tasks_for_issue(local_dir: &LocalDirectory, issue_number: i32) -> Result<()> {
    let tasks = local_dir.scan_tasks()?;
    for task in &tasks {
        if task.frontmatter.issue == Some(issue_number) {
            local_dir.remove_task_file(&task.file_path)?;
        }
    }
    Ok(())
}
