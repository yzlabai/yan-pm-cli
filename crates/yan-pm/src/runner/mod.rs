use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{bail, Result};
use colored::Colorize;
use tokio::time;

use crate::agent::{self, AgentDefinition, AgentOptions, AgentResult};
use crate::api::client::*;
use crate::api::types::*;
use crate::output::truncate_utf8;

/// Options for task execution
pub struct TaskRunnerOptions {
    pub cwd: String,
    pub workspace_id: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub permission_mode: Option<String>,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub mcp_configs: Option<Vec<String>>,
    pub verbose: bool,
    pub agent: AgentDefinition,
}

/// Options for the start command
pub struct StartOptions {
    pub project_id: String,
    pub task_id: Option<String>,
    pub auto: bool,
    pub total_budget_usd: Option<f64>,
    pub runner: TaskRunnerOptions,
}

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);

/// Priority order for task sorting
fn priority_order(p: &TaskPriority) -> u8 {
    p.order()
}

/// Pick the next task to execute (highest priority, oldest first)
fn pick_next_task(tasks: &[Task]) -> Option<&Task> {
    let mut todo: Vec<&Task> = tasks.iter().filter(|t| t.status == TaskStatus::Todo).collect();
    todo.sort_by(|a, b| {
        let pa = priority_order(&a.priority);
        let pb = priority_order(&b.priority);
        pa.cmp(&pb).then_with(|| a.created_at.cmp(&b.created_at))
    });
    todo.first().copied()
}

/// Build the task prompt for the agent
fn build_task_prompt(task: &Task, project_name: &str) -> String {
    let type_label = match task.task_type {
        TaskType::Feature => "功能",
        TaskType::Bug => "Bug 修复",
        TaskType::Improvement => "改进",
        TaskType::Task => "任务",
    };
    let priority_label = match task.priority {
        TaskPriority::Urgent => "紧急",
        TaskPriority::High => "高",
        TaskPriority::Medium => "中",
        TaskPriority::Low => "低",
    };

    let mut prompt = format!(
        "# 任务: {title}\n\n- **项目**: {project}\n- **类型**: {type_label}\n- **优先级**: {priority_label}\n",
        title = task.title,
        project = project_name,
    );

    if !task.tags.is_empty() {
        prompt.push_str(&format!("- **标签**: {}\n", task.tags.join(", ")));
    }
    if let Some(due) = &task.due_date {
        prompt.push_str(&format!("- **截止日期**: {due}\n"));
    }

    prompt.push_str("\n## 需求描述\n\n");
    prompt.push_str(task.description.as_deref().unwrap_or("(无描述)"));
    prompt.push_str("\n\n## 执行要求\n\n");
    prompt.push_str("1. 仔细阅读需求描述，理解任务目标\n");
    prompt.push_str("2. 在当前代码库中实现所需的变更\n");
    prompt.push_str("3. 确保代码通过类型检查（如适用）\n");
    prompt.push_str("4. 不要修改与任务无关的代码\n");
    prompt.push_str("5. 完成后简要总结你做了什么\n");

    prompt
}

/// Execute a single task: lock → transition → heartbeat → agent → report → unlock
async fn run_task(
    client: &ApiClient,
    task: &Task,
    project_name: &str,
    opts: &TaskRunnerOptions,
) -> AgentResult {
    let task_id = &task.id;
    let project_id = &task.project_id;
    let short_id = &task_id[..8.min(task_id.len())];

    println!(
        "{}",
        format!("▸ 开始执行任务: {} [{}]", task.title, short_id).cyan()
    );

    // 1. Lock task
    match client.lock_task(project_id, task_id, opts.workspace_id.as_deref()).await {
        Ok(_) => {}
        Err(e) => {
            if e.is_conflict() {
                eprintln!("{}", format!("  ⚠ 任务被锁定，跳过: {e}").yellow());
            } else {
                eprintln!("{}", format!("  ✗ 锁定失败: {e}").red());
            }
            return AgentResult {
                success: false,
                summary: format!("锁定失败: {e}"),
                cost_usd: None,
                session_id: None,
                exit_code: 1,
            };
        }
    }

    // 2. Transition to in_progress
    let _ = client
        .update_task(
            project_id,
            task_id,
            &UpdateTaskData {
                status: Some(TaskStatus::InProgress),
                ..Default::default()
            },
        )
        .await;

    // 3. Start heartbeat
    let heartbeat_running = Arc::new(AtomicBool::new(true));
    let hb_flag = heartbeat_running.clone();
    let hb_client_url = client.base_url().to_string();
    let hb_client_token = client.token().to_string();
    let hb_project = project_id.to_string();
    let hb_task = task_id.to_string();
    let heartbeat_handle = tokio::spawn(async move {
        let mut interval = time::interval(HEARTBEAT_INTERVAL);
        interval.tick().await; // skip first immediate tick
        while hb_flag.load(Ordering::Acquire) {
            interval.tick().await;
            if !hb_flag.load(Ordering::Acquire) {
                break;
            }
            if let Ok(hb_client) = ApiClient::new(&hb_client_url, &hb_client_token) {
                let _ = hb_client.heartbeat(&hb_project, &hb_task).await;
            }
        }
    });

    // 4. Execute agent via ACP
    let prompt = build_task_prompt(task, project_name);
    let _verbose = opts.verbose;
    let agent_result = agent::execute_agent(
        &opts.agent,
        AgentOptions {
            cwd: opts.cwd.clone(),
            prompt,
            max_budget_usd: opts.max_budget_usd,
            permission_mode: opts.permission_mode.clone(),
            allowed_tools: opts.allowed_tools.clone(),
            mcp_configs: opts.mcp_configs.clone(),
            model: opts.model.clone(),
            verbose: opts.verbose,
        },
    )
    .await;

    // 5. Stop heartbeat gracefully
    heartbeat_running.store(false, Ordering::Release);
    // Give heartbeat loop time to finish any in-flight request
    let _ = time::timeout(Duration::from_secs(5), heartbeat_handle).await;

    let result = match agent_result {
        Ok(r) => r,
        Err(e) => AgentResult {
            success: false,
            summary: format!("Agent 执行错误: {e}"),
            cost_usd: None,
            session_id: None,
            exit_code: 1,
        },
    };

    // 6. Report result
    let new_status = if result.success {
        TaskStatus::Done
    } else {
        TaskStatus::Todo
    };

    let _ = client
        .update_task(
            project_id,
            task_id,
            &UpdateTaskData {
                status: Some(new_status),
                ..Default::default()
            },
        )
        .await;

    // Add comment with result summary
    let cost_str = result
        .cost_usd
        .map(|c| format!(" | 费用: ${:.4}", c))
        .unwrap_or_default();
    let status_emoji = if result.success { "✅" } else { "❌" };
    let comment = format!(
        "{status_emoji} 自动执行{result_str}{cost_str}\n\n{summary}",
        result_str = if result.success { "完成" } else { "失败" },
        summary = truncate_utf8(&result.summary, 2000),
    );
    let _ = client.add_comment(project_id, task_id, &comment).await;

    // 7. Unlock
    let _ = client.unlock_task(project_id, task_id).await;

    if result.success {
        println!(
            "{}",
            format!(
                "  ✓ 完成: {}{cost_str}",
                truncate_utf8(&result.summary, 100)
            )
            .green()
        );
    } else {
        eprintln!(
            "{}",
            format!("  ✗ 失败: {}", truncate_utf8(&result.summary, 100)).red()
        );
    }

    result
}

/// Resolve task by ID or prefix from a list
fn resolve_task<'a>(tasks: &'a [Task], id_or_prefix: &str) -> Option<&'a Task> {
    // Exact match
    if let Some(t) = tasks.iter().find(|t| t.id == id_or_prefix) {
        return Some(t);
    }
    // Prefix match
    let matches: Vec<&Task> = tasks.iter().filter(|t| t.id.starts_with(id_or_prefix)).collect();
    if matches.len() == 1 {
        return Some(matches[0]);
    }
    if matches.len() > 1 {
        eprintln!("{}", format!("✗ 任务 ID 前缀 \"{id_or_prefix}\" 匹配到多个任务:").red());
        for m in &matches {
            eprintln!("  {} {}", &m.id[..8.min(m.id.len())].dimmed(), m.title);
        }
    }
    None
}

/// Main start entry point
pub async fn start(client: &ApiClient, options: StartOptions) -> Result<()> {
    // Fetch project info
    let project = client.get_project(&options.project_id).await
        .map_err(|e| anyhow::anyhow!("获取项目失败: {e}"))?;
    let project_name = &project.project.name;
    let project_id = &project.project.id;

    println!("{}", format!("📋 项目: {project_name}").bold());

    if let Some(task_id) = &options.task_id {
        // Specific mode
        let tasks = client
            .list_tasks(project_id, &TaskListParams::default())
            .await?;
        let task = resolve_task(&tasks, task_id);
        match task {
            Some(t) => {
                if t.status == TaskStatus::Done || t.status == TaskStatus::Cancelled {
                    bail!("任务状态为 {}，无法执行", t.status);
                }
                let result = run_task(client, t, project_name, &options.runner).await;
                if !result.success {
                    bail!("任务执行失败");
                }
            }
            None => bail!("任务不存在: {task_id}"),
        }
    } else if options.auto {
        // Batch mode
        let mut completed = 0u32;
        let mut failed = 0u32;
        let mut total_cost = 0.0f64;
        let total_budget = options.total_budget_usd;
        let mut failed_task_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        loop {
            if let Some(budget) = total_budget {
                if total_cost >= budget {
                    println!(
                        "{}",
                        format!("\n⚠ 已达总预算上限 ${budget:.2}，停止执行").yellow()
                    );
                    break;
                }
            }

            let tasks = client
                .list_tasks(project_id, &TaskListParams { status: Some(TaskStatus::Todo), ..Default::default() })
                .await?;
            // Skip tasks that already failed in this session
            let eligible: Vec<&Task> = tasks.iter().filter(|t| !failed_task_ids.contains(&t.id)).collect();
            let task = {
                let mut todo: Vec<&&Task> = eligible.iter().collect();
                todo.sort_by(|a, b| {
                    let pa = priority_order(&a.priority);
                    let pb = priority_order(&b.priority);
                    pa.cmp(&pb).then_with(|| a.created_at.cmp(&b.created_at))
                });
                todo.first().map(|t| **t)
            };

            match task {
                Some(t) => {
                    let result = run_task(client, t, project_name, &options.runner).await;
                    if result.success {
                        completed += 1;
                    } else {
                        failed += 1;
                        failed_task_ids.insert(t.id.clone());
                    }
                    if let Some(cost) = result.cost_usd {
                        total_cost += cost;
                    }
                }
                None => {
                    println!("{}", "没有更多待执行的任务".yellow());
                    break;
                }
            }
        }

        println!(
            "\n{} ✓ 成功: {completed}, ✗ 失败: {failed}, 💰 总费用: ${total_cost:.2}",
            "执行完毕".bold()
        );
    } else {
        // Single mode
        let tasks = client
            .list_tasks(project_id, &TaskListParams { status: Some(TaskStatus::Todo), ..Default::default() })
            .await?;
        let task = pick_next_task(&tasks);

        match task {
            Some(t) => {
                let todo_count = tasks.iter().filter(|t| t.status == TaskStatus::Todo).count();
                println!(
                    "{}",
                    format!("  找到 {todo_count} 个待执行任务，选择最高优先级:").dimmed()
                );
                let result = run_task(client, t, project_name, &options.runner).await;
                if !result.success {
                    bail!("任务执行失败");
                }
            }
            None => {
                println!("{}", "⚠ 没有待执行的任务 (todo)".yellow());
            }
        }
    }

    Ok(())
}
