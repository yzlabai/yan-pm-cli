use anyhow::Result;

use crate::agent::registry::find_backend;
use crate::agent::session::{execute_agent, AgentOptions};
use crate::local::directory::LocalDirectory;
use crate::local::taskfile::{render_task_file, TaskFrontmatter};

pub async fn run(
    issue: Option<i32>,
    task_id: Option<&str>,
    agent_name: &str,
    permission_mode: &str,
    verbose: bool,
) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let local_dir = LocalDirectory::new(&cwd);

    if !local_dir.is_initialized() {
        anyhow::bail!("当前目录未初始化。请先运行: yan pull");
    }

    let backend = find_backend(agent_name).ok_or_else(|| {
        anyhow::anyhow!("Agent '{}' 未找到。可用: claude, codex, gemini", agent_name)
    })?;

    // Find the target task
    let tasks = local_dir.scan_tasks()?;
    let target_task = find_target_task(&tasks, issue, task_id)?;

    let task_fm = &target_task.frontmatter;

    // Read the spec for context (if the task has an issue association)
    let spec_content = if let Some(issue_num) = task_fm.issue.or(issue) {
        local_dir.find_spec_by_issue(issue_num)?.map(|s| {
            let mut content = String::new();
            content.push_str(&format!("# Spec: {}\n\n", s.frontmatter.title));
            content.push_str(&s.body);
            content
        })
    } else {
        None
    };

    let prompt = build_run_prompt(task_fm, &target_task.body, spec_content.as_deref());

    println!("🚀 执行任务: {} (agent: {})", task_fm.title, agent_name);

    let options = AgentOptions {
        cwd: cwd.to_string_lossy().to_string(),
        prompt,
        max_budget_usd: None,
        permission_mode: Some(permission_mode.to_string()),
        allowed_tools: None,
        mcp_configs: None,
        model: None,
        verbose,
    };

    let result = execute_agent(backend.as_ref(), options, None).await?;

    if result.success {
        // Update task status to done
        let mut updated_fm = task_fm.clone();
        updated_fm.status = crate::api::types::TaskStatus::Done;
        updated_fm.updated = chrono::Utc::now().to_rfc3339();

        // Write the updated task file
        let content = render_task_file(&updated_fm, &target_task.body)?;
        std::fs::write(&target_task.file_path, &content)?;

        println!("✓ 任务完成: {}", task_fm.title);
        println!("  状态已更新: todo → done");

        // Print summary (truncated)
        let summary = &result.summary;
        if !summary.is_empty() {
            let display = if summary.len() > 500 {
                &summary[..500]
            } else {
                summary
            };
            println!("\n📋 Agent 总结:\n{}", display);
        }
    } else {
        eprintln!("❌ 任务执行失败: {}", task_fm.title);
        let summary = &result.summary;
        if !summary.is_empty() {
            let display = if summary.len() > 500 {
                &summary[..500]
            } else {
                summary
            };
            eprintln!("  {}", display);
        }
        anyhow::bail!("任务执行失败 (exit_code: {})", result.exit_code);
    }

    Ok(())
}

/// Find the target task based on filters
fn find_target_task<'a>(
    tasks: &'a [crate::local::taskfile::LocalTaskFile],
    issue: Option<i32>,
    task_id: Option<&str>,
) -> Result<&'a crate::local::taskfile::LocalTaskFile> {
    // If a specific task ID/number is provided, find it
    if let Some(tid) = task_id {
        // Try matching by number prefix (e.g. "001" or "001-01")
        let task = tasks
            .iter()
            .find(|t| {
                // Match by file stem
                let stem = t
                    .file_path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                stem.starts_with(tid)
                    || t.frontmatter.id.as_deref() == Some(tid)
                    || t.frontmatter.number.map(|n| format!("{:03}", n)) == Some(tid.to_string())
            })
            .ok_or_else(|| anyhow::anyhow!("任务 '{}' 未找到", tid))?;
        return Ok(task);
    }

    // Filter by issue if provided
    let candidates: Vec<_> = if let Some(issue_num) = issue {
        tasks
            .iter()
            .filter(|t| t.frontmatter.issue == Some(issue_num))
            .collect()
    } else {
        tasks.iter().collect()
    };

    if candidates.is_empty() {
        if let Some(issue_num) = issue {
            anyhow::bail!("Issue #{} 没有待执行的任务", issue_num);
        } else {
            anyhow::bail!("没有待执行的任务");
        }
    }

    // Find the first todo task (by priority, then number)
    let todo_tasks: Vec<_> = candidates
        .iter()
        .filter(|t| t.frontmatter.status == crate::api::types::TaskStatus::Todo)
        .collect();

    if todo_tasks.is_empty() {
        anyhow::bail!("没有 todo 状态的任务可执行");
    }

    // Sort by priority (urgent > high > medium > low), then by number
    let mut sorted = todo_tasks;
    sorted.sort_by(|a, b| {
        priority_rank(a.frontmatter.priority)
            .cmp(&priority_rank(b.frontmatter.priority))
            .then_with(|| a.frontmatter.number.cmp(&b.frontmatter.number))
    });

    Ok(sorted[0])
}

/// Lower rank = higher priority
fn priority_rank(p: crate::api::types::TaskPriority) -> u8 {
    use crate::api::types::TaskPriority;
    match p {
        TaskPriority::Urgent => 0,
        TaskPriority::High => 1,
        TaskPriority::Medium => 2,
        TaskPriority::Low => 3,
    }
}

/// Build the prompt for task execution
fn build_run_prompt(
    task_fm: &TaskFrontmatter,
    task_body: &str,
    spec_content: Option<&str>,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("你是一个资深软件开发工程师。请完成以下任务。\n\n");

    if let Some(spec) = spec_content {
        prompt.push_str("## Spec（技术规格）\n");
        prompt.push_str(spec);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## 当前任务\n");
    prompt.push_str(&format!("- 标题: {}\n", task_fm.title));
    if !task_body.is_empty() {
        prompt.push_str(&format!("- 描述:\n{}\n", task_body));
    }

    prompt.push_str(
        "\n请根据 Spec 完成这个任务，确保代码质量和测试覆盖。完成后在终端输出一段简要总结。\n",
    );

    prompt
}
