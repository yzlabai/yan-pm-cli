use anyhow::Result;

use crate::agent::registry::find_backend;
use crate::agent::session::{execute_agent, AgentOptions};
use crate::local::directory::LocalDirectory;
use crate::local::taskfile::{render_task_file, LocalTaskFile, TaskFrontmatter};

pub async fn run(
    issue: Option<i32>,
    task_id: Option<&str>,
    agent_name: &str,
    permission_mode: &str,
    auto: bool,
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

    if auto {
        run_auto(
            &cwd,
            &local_dir,
            issue,
            backend.as_ref(),
            permission_mode,
            verbose,
        )
        .await
    } else {
        run_single(
            &cwd,
            &local_dir,
            issue,
            task_id,
            backend.as_ref(),
            permission_mode,
            verbose,
        )
        .await
    }
}

/// Execute a single task (original behavior).
async fn run_single(
    cwd: &std::path::Path,
    local_dir: &LocalDirectory,
    issue: Option<i32>,
    task_id: Option<&str>,
    backend: &dyn crate::agent::backend::AgentBackend,
    permission_mode: &str,
    verbose: bool,
) -> Result<()> {
    let tasks = local_dir.scan_tasks()?;
    let target_task = find_target_task(&tasks, issue, task_id)?;

    execute_and_update(
        cwd,
        local_dir,
        target_task,
        issue,
        backend,
        permission_mode,
        verbose,
    )
    .await
}

/// Auto-execute all todo tasks for an issue sequentially.
async fn run_auto(
    cwd: &std::path::Path,
    local_dir: &LocalDirectory,
    issue: Option<i32>,
    backend: &dyn crate::agent::backend::AgentBackend,
    permission_mode: &str,
    verbose: bool,
) -> Result<()> {
    let issue_num = issue.ok_or_else(|| anyhow::anyhow!("--auto 模式需要指定 --issue 参数"))?;

    // Auto-generate tasks from spec if none exist
    if !local_dir.has_tasks_for_issue(issue_num)? {
        if let Some(spec) = local_dir.find_spec_by_issue(issue_num)? {
            match crate::local::task_parser::parse_tasks_from_spec(&spec.body) {
                Ok(parsed) if !parsed.is_empty() => {
                    let paths = local_dir.generate_tasks_from_spec(issue_num, &parsed)?;
                    println!("✓ 从 Spec 生成了 {} 个任务", paths.len());
                }
                Ok(_) => {
                    anyhow::bail!("Spec 的 \"任务拆分\" 部分为空，请先编辑 Spec 添加任务");
                }
                Err(e) => {
                    anyhow::bail!("无法从 Spec 解析任务: {}", e);
                }
            }
        } else {
            anyhow::bail!("Issue #{} 没有 Spec 也没有任务", issue_num);
        }
    }

    let mut completed = 0;
    let mut failed = 0;

    loop {
        // Re-scan tasks each iteration to pick up status changes
        let tasks = local_dir.scan_tasks()?;
        let next = find_next_executable_task(&tasks, issue_num);

        let task = match next {
            Some(t) => t,
            None => break,
        };

        println!(
            "\n▶ [{}/{}] 执行任务: {}",
            completed + failed + 1,
            count_todo_tasks(&tasks, issue_num) + completed + failed,
            task.frontmatter.title
        );

        match execute_and_update(
            cwd,
            local_dir,
            task,
            Some(issue_num),
            backend,
            permission_mode,
            verbose,
        )
        .await
        {
            Ok(()) => {
                completed += 1;
            }
            Err(e) => {
                failed += 1;
                eprintln!("✗ 任务失败: {}", e);
                eprintln!("  停止自动执行。");
                break;
            }
        }
    }

    // Print summary
    println!("\n━━━ 自动执行完成 ━━━");
    println!("  ✓ 完成: {}", completed);
    if failed > 0 {
        println!("  ✗ 失败: {}", failed);
    }

    // Check remaining
    let tasks = local_dir.scan_tasks()?;
    let remaining = count_todo_tasks(&tasks, issue_num);
    if remaining > 0 {
        println!("  ○ 剩余: {}", remaining);
    }

    if failed > 0 {
        anyhow::bail!("自动执行中有 {} 个任务失败", failed);
    }

    Ok(())
}

/// Execute a single task and update its status.
async fn execute_and_update(
    cwd: &std::path::Path,
    local_dir: &LocalDirectory,
    target_task: &LocalTaskFile,
    issue: Option<i32>,
    backend: &dyn crate::agent::backend::AgentBackend,
    permission_mode: &str,
    verbose: bool,
) -> Result<()> {
    let task_fm = &target_task.frontmatter;

    // Read the spec for context
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

    println!("🚀 执行任务: {} (agent: {})", task_fm.title, backend.name());

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

    let result = execute_agent(backend, options, None).await?;

    if result.success {
        // Update task status to done
        let mut updated_fm = task_fm.clone();
        updated_fm.status = crate::api::types::TaskStatus::Done;
        updated_fm.updated = chrono::Utc::now().to_rfc3339();

        let content = render_task_file(&updated_fm, &target_task.body)?;
        std::fs::write(&target_task.file_path, &content)?;

        println!("✓ 任务完成: {}", task_fm.title);
        println!("  状态已更新: todo → done");

        let summary = &result.summary;
        if !summary.is_empty() {
            let display = if summary.len() > 500 {
                &summary[..500]
            } else {
                summary
            };
            println!("\n📋 Agent 总结:\n{}", display);
        }

        Ok(())
    } else {
        let summary = &result.summary;
        let display = if summary.len() > 500 {
            &summary[..500]
        } else {
            summary
        };
        if !summary.is_empty() {
            eprintln!("  {}", display);
        }
        anyhow::bail!(
            "任务执行失败: {} (exit_code: {})",
            task_fm.title,
            result.exit_code
        )
    }
}

/// Find the next executable task for an issue.
/// A task is executable if it's `todo` and all its dependencies are `done`.
fn find_next_executable_task(tasks: &[LocalTaskFile], issue_number: i32) -> Option<&LocalTaskFile> {
    let issue_tasks: Vec<_> = tasks
        .iter()
        .filter(|t| t.frontmatter.issue == Some(issue_number))
        .collect();

    // Build a set of done task identifiers (file stems)
    let done_ids: std::collections::HashSet<String> = issue_tasks
        .iter()
        .filter(|t| t.frontmatter.status == crate::api::types::TaskStatus::Done)
        .map(|t| task_stem_id(t))
        .collect();

    // Find the first todo task whose dependencies are all met
    let mut candidates: Vec<_> = issue_tasks
        .iter()
        .filter(|t| t.frontmatter.status == crate::api::types::TaskStatus::Todo)
        .filter(|t| {
            t.frontmatter.depends_on.iter().all(|dep| {
                // Check if any done task's stem starts with this dep
                done_ids.iter().any(|done_id| done_id.starts_with(dep))
            })
        })
        .collect();

    // Sort by priority then by filename
    candidates.sort_by(|a, b| {
        priority_rank(a.frontmatter.priority)
            .cmp(&priority_rank(b.frontmatter.priority))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    candidates.into_iter().next().copied()
}

/// Get a stem identifier for a task (filename without extension).
fn task_stem_id(task: &LocalTaskFile) -> String {
    task.file_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

/// Count todo tasks for an issue.
fn count_todo_tasks(tasks: &[LocalTaskFile], issue_number: i32) -> usize {
    tasks
        .iter()
        .filter(|t| {
            t.frontmatter.issue == Some(issue_number)
                && t.frontmatter.status == crate::api::types::TaskStatus::Todo
        })
        .count()
}

/// Find the target task based on filters (for single-task mode).
fn find_target_task<'a>(
    tasks: &'a [LocalTaskFile],
    issue: Option<i32>,
    task_id: Option<&str>,
) -> Result<&'a LocalTaskFile> {
    // If a specific task ID/number is provided, find it
    if let Some(tid) = task_id {
        let task = tasks
            .iter()
            .find(|t| {
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

    // Find the first todo task
    let mut todo_tasks: Vec<_> = candidates
        .iter()
        .filter(|t| t.frontmatter.status == crate::api::types::TaskStatus::Todo)
        .collect();

    if todo_tasks.is_empty() {
        anyhow::bail!("没有 todo 状态的任务可执行");
    }

    // Sort by priority (urgent > high > medium > low), then by number
    todo_tasks.sort_by(|a, b| {
        priority_rank(a.frontmatter.priority)
            .cmp(&priority_rank(b.frontmatter.priority))
            .then_with(|| a.frontmatter.number.cmp(&b.frontmatter.number))
    });

    Ok(todo_tasks[0])
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
