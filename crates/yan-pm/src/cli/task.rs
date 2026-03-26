use std::path::Path;

use anyhow::Result;

use crate::api::client::{CreateTaskData, TaskListParams, UpdateTaskData};
use crate::api::types::*;
use crate::config;
use crate::local::directory::LocalDirectory;
use crate::output;
use super::make_client;

pub async fn list(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: Option<&str>,
    status: Option<&str>,
    task_type: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    keyword: Option<&str>,
    local: bool,
) -> Result<()> {
    // If --local or no project_id given, try reading from local files
    let cwd = std::env::current_dir()?;
    let link = config::find_workspace_link(Some(Path::new(&cwd.to_string_lossy().as_ref())));

    if local || (project_id.is_none() && link.is_some()) {
        let local_dir = LocalDirectory::new(&cwd);
        if !local_dir.is_initialized() {
            anyhow::bail!("本地任务目录未初始化。请先运行 `yan-pm link <project>` 或 `yan-pm sync`");
        }

        let mut tasks = local_dir.scan_tasks()?;

        // Apply status filter
        if let Some(s) = status {
            if let Ok(status_val) = serde_json::from_value::<TaskStatus>(serde_json::json!(s)) {
                tasks.retain(|t| t.frontmatter.status == status_val);
            }
        }

        // Apply type filter
        if let Some(t) = task_type {
            if let Ok(type_val) = serde_json::from_value::<TaskType>(serde_json::json!(t)) {
                tasks.retain(|task| task.frontmatter.task_type == type_val);
            }
        }

        // Apply priority filter
        if let Some(p) = priority {
            if let Ok(prio_val) = serde_json::from_value::<TaskPriority>(serde_json::json!(p)) {
                tasks.retain(|t| t.frontmatter.priority == prio_val);
            }
        }

        // Apply keyword filter
        if let Some(kw) = keyword {
            let kw_lower = kw.to_lowercase();
            tasks.retain(|t| {
                t.frontmatter.title.to_lowercase().contains(&kw_lower)
                    || t.body.to_lowercase().contains(&kw_lower)
            });
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
                        "tags": t.frontmatter.tags,
                        "file": t.file_path.display().to_string(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json_tasks)?);
        } else {
            output::print_local_tasks(&tasks);
        }
        return Ok(());
    }

    // Fall back to remote API
    let pid = project_id.ok_or_else(|| {
        anyhow::anyhow!("请指定项目 ID 或在已关联目录中运行（使用 `yan-pm link <project>` 关联）")
    })?;

    let client = make_client(url, token)?;
    let params = TaskListParams {
        status: status.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        task_type: task_type.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        priority: priority.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        assignee_id: assignee.map(String::from),
        search: keyword.map(String::from),
    };
    let tasks = client.list_tasks(pid, &params).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&tasks)?);
    } else {
        output::print_tasks(&tasks);
    }
    Ok(())
}

pub async fn create(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    title: &str,
    description: Option<&str>,
    task_type: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    due: Option<&str>,
    tags: Option<&str>,
) -> Result<()> {
    let client = make_client(url, token)?;
    let tags_vec = tags.map(|t| t.split(',').map(|s| s.trim().to_string()).collect());
    let data = CreateTaskData {
        title: title.into(),
        description: description.map(String::from),
        task_type: task_type.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        priority: priority.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        assignee_id: assignee.map(String::from),
        due_date: due.map(String::from),
        tags: tags_vec,
    };
    let task = client.create_task(project_id, &data).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&task)?);
    } else {
        println!("✓ 任务已创建: {} [{}]", task.title, &task.id[..8.min(task.id.len())]);
    }
    Ok(())
}

pub async fn update(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    task_id: &str,
    title: Option<&str>,
    status: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    task_type: Option<&str>,
) -> Result<()> {
    let client = make_client(url, token)?;
    let resolved_id = client.resolve_task_id(project_id, task_id).await?;
    let data = UpdateTaskData {
        title: title.map(String::from),
        status: status.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        priority: priority.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        assignee_id: assignee.map(String::from),
        task_type: task_type.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
    };
    let task = client.update_task(project_id, &resolved_id, &data).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&task)?);
    } else {
        println!("✓ 任务已更新: {}", task.title);
    }
    Ok(())
}

pub async fn comment(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    task_id: &str,
    content: &str,
) -> Result<()> {
    let client = make_client(url, token)?;
    let resolved_id = client.resolve_task_id(project_id, task_id).await?;
    let comment = client.add_comment(project_id, &resolved_id, content).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&comment)?);
    } else {
        println!("✓ 评论已添加");
    }
    Ok(())
}

pub async fn status(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    task_id: Option<&str>,
) -> Result<()> {
    let client = make_client(url, token)?;
    if let Some(tid) = task_id {
        // Show specific task detail
        let resolved_id = client.resolve_task_id(project_id, tid).await?;
        let detail = client.get_task(project_id, &resolved_id).await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&detail)?);
        } else {
            let t = &detail.task;
            println!("任务: {} [{}]", t.title, &t.id[..8.min(t.id.len())]);
            println!("状态: {}  优先级: {}  类型: {}", t.status, t.priority, t.task_type);
            if let Some(desc) = &t.description {
                println!("描述: {desc}");
            }
        }
    } else {
        // Show project-level execution status with stale detection
        let exec_status = client.get_execution_status(project_id).await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&exec_status)?);
        } else {
            output::print_execution_status(&exec_status);
        }
    }
    Ok(())
}

pub async fn force_unlock(
    url: Option<&str>,
    token: Option<&str>,
    project_id: &str,
    task_id: &str,
) -> Result<()> {
    let client = make_client(url, token)?;
    let resolved_id = client.resolve_task_id(project_id, task_id).await?;
    client.force_unlock(project_id, &resolved_id).await?;
    println!("✓ 任务已强制解锁");
    Ok(())
}
