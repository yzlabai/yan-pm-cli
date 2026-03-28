use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::task::JoinHandle;
use tokio::time;

use crate::agent::{self, find_agent, AgentOptions, AgentResult};
use crate::api::client::{ApiClient, UpdateTaskData};
use crate::api::types::{ExecutionReport, TaskStatus};
use crate::local::directory::{AutoRunConfig, LocalDirectory};

use super::event_store::EventStore;

/// A single running task execution.
struct RunningTask {
    task_id: String,
    project_id: String,
    workspace_id: Option<String>,
    /// Agent runs in a dedicated OS thread (ACP LocalSet is !Send)
    thread_handle: Option<std::thread::JoinHandle<AgentResult>>,
    heartbeat_running: Arc<AtomicBool>,
    heartbeat_handle: JoinHandle<()>,
    started_at: chrono::DateTime<chrono::Utc>,
}

/// Per-workspace runner slot.
struct RunnerSlot {
    workspace_path: String,
    project_id: String,
    config: AutoRunConfig,
    running: Vec<RunningTask>,
    total_cost: f64,
    /// Task IDs that failed in this session — skip to avoid infinite retry
    failed_task_ids: HashSet<String>,
}

/// AutoRunner manages automatic task execution across workspaces in the daemon.
pub struct AutoRunner {
    client: Arc<ApiClient>,
    slots: HashMap<String, RunnerSlot>,
    event_store: Option<Arc<EventStore>>,
}

impl AutoRunner {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            slots: HashMap::new(),
            event_store: None,
        }
    }

    pub fn set_event_store(&mut self, store: Arc<EventStore>) {
        self.event_store = Some(store);
    }

    /// Register or update a workspace slot.
    pub fn set_workspace(&mut self, path: &str, project_id: &str, config: AutoRunConfig) {
        if let Some(slot) = self.slots.get_mut(path) {
            slot.config = config;
        } else {
            self.slots.insert(
                path.to_string(),
                RunnerSlot {
                    workspace_path: path.to_string(),
                    project_id: project_id.to_string(),
                    config,
                    running: Vec::new(),
                    total_cost: 0.0,
                    failed_task_ids: HashSet::new(),
                },
            );
        }
    }

    /// Remove a workspace slot (cleanup running tasks first).
    #[allow(dead_code)]
    pub fn remove_workspace(&mut self, path: &str) {
        if let Some(mut slot) = self.slots.remove(path) {
            for mut task in slot.running.drain(..) {
                task.heartbeat_running.store(false, Ordering::Release);
                // Thread handle will be dropped (thread continues but won't affect state)
                task.thread_handle.take();
                task.heartbeat_handle.abort();
            }
        }
    }

    /// Check all slots and start new tasks if possible. Called every 30s.
    pub async fn check_and_run(&mut self) {
        let paths: Vec<String> = self.slots.keys().cloned().collect();
        for path in paths {
            if let Err(e) = self.check_slot(&path).await {
                tracing::error!("AutoRunner check error for {path}: {e}");
            }
        }
    }

    /// Process completed tasks across all slots.
    pub async fn collect_completed(&mut self) {
        let paths: Vec<String> = self.slots.keys().cloned().collect();
        for path in paths {
            self.collect_slot_completed(&path).await;
        }
    }

    /// Check if any workspace has auto-run enabled.
    #[allow(dead_code)]
    pub fn has_any_enabled(&self) -> bool {
        self.slots.values().any(|s| s.config.enabled)
    }

    async fn check_slot(&mut self, path: &str) -> Result<()> {
        let slot = match self.slots.get(path) {
            Some(s) => s,
            None => return Ok(()),
        };

        if !slot.config.enabled {
            return Ok(());
        }

        // Check concurrency
        let active_count = slot
            .running
            .iter()
            .filter(|t| {
                t.thread_handle
                    .as_ref()
                    .map(|h| !h.is_finished())
                    .unwrap_or(false)
            })
            .count();
        if active_count >= slot.config.concurrency as usize {
            return Ok(());
        }

        // Check budget
        if let Some(budget) = slot.config.budget {
            if slot.total_cost >= budget {
                tracing::info!(
                    "AutoRunner: budget exhausted for {path} (${:.2} / ${:.2})",
                    slot.total_cost,
                    budget
                );
                return Ok(());
            }
        }

        // Find next todo task from local files
        let local_dir = LocalDirectory::new(Path::new(&slot.workspace_path));
        let local_tasks = local_dir.scan_tasks().unwrap_or_default();
        let failed_ids = &slot.failed_task_ids;
        let mut todo_tasks: Vec<_> = local_tasks
            .iter()
            .filter(|t| t.frontmatter.status == TaskStatus::Todo)
            .filter(|t| t.frontmatter.id.is_some()) // Must have server ID
            .filter(|t| {
                !t.frontmatter
                    .id
                    .as_ref()
                    .is_some_and(|id| failed_ids.contains(id))
            })
            .collect();

        // Filter out tasks with unfinished dependencies
        {
            let done_ids: HashSet<&str> = local_tasks
                .iter()
                .filter(|t| t.frontmatter.status == TaskStatus::Done)
                .filter_map(|t| t.frontmatter.id.as_deref())
                .collect();
            todo_tasks.retain(|t| {
                t.frontmatter.depends_on.is_empty()
                    || t.frontmatter
                        .depends_on
                        .iter()
                        .all(|dep| done_ids.contains(dep.as_str()))
            });
        }

        // Apply priority filter
        if !slot.config.filter_priority.is_empty() {
            todo_tasks.retain(|t| {
                let pri_str = t.frontmatter.priority.to_string();
                slot.config
                    .filter_priority
                    .iter()
                    .any(|f| f.to_lowercase() == pri_str)
            });
        }

        // Sort by priority (urgent first), then by created date
        todo_tasks.sort_by(|a, b| {
            a.frontmatter
                .priority
                .order()
                .cmp(&b.frontmatter.priority.order())
                .then_with(|| a.frontmatter.created.cmp(&b.frontmatter.created))
        });

        let next_task = match todo_tasks.first() {
            Some(t) => t,
            None => return Ok(()),
        };

        let task_id = next_task.frontmatter.id.as_ref().unwrap().clone();
        let project_id = slot.project_id.clone();
        let workspace_path = slot.workspace_path.clone();
        let agent_name = slot.config.agent.clone();

        // Skip if already running this task
        if slot.running.iter().any(|r| r.task_id == task_id) {
            return Ok(());
        }

        // Try to lock task
        let workspace_entry = crate::config::find_workspace_link(Some(Path::new(&workspace_path)));
        let ws_id = workspace_entry.and_then(|w| w.workspace_id);
        match self
            .client
            .lock_task(&project_id, &task_id, ws_id.as_deref())
            .await
        {
            Ok(_) => {}
            Err(e) => {
                if e.is_conflict() {
                    tracing::debug!("AutoRunner: task {task_id} already locked, skipping");
                } else {
                    tracing::warn!("AutoRunner: failed to lock task {task_id}: {e}");
                }
                return Ok(());
            }
        }

        // Transition to in_progress
        let _ = self
            .client
            .update_task(
                &project_id,
                &task_id,
                &UpdateTaskData {
                    status: Some(TaskStatus::InProgress),
                    ..Default::default()
                },
            )
            .await;

        tracing::info!(
            "AutoRunner: starting task {} in {}",
            task_id,
            workspace_path
        );

        // Resolve agent
        let agent = match find_agent(&agent_name) {
            Some(a) => a,
            None => {
                tracing::error!("AutoRunner: agent '{agent_name}' not found");
                let _ = self.client.unlock_task(&project_id, &task_id).await;
                return Ok(());
            }
        };

        // Start heartbeat
        let heartbeat_running = Arc::new(AtomicBool::new(true));
        let hb_flag = heartbeat_running.clone();
        let hb_url = self.client.base_url().to_string();
        let hb_token = self.client.token().to_string();
        let hb_project = project_id.clone();
        let hb_task = task_id.clone();
        let heartbeat_handle = tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(60));
            interval.tick().await;
            while hb_flag.load(Ordering::Acquire) {
                interval.tick().await;
                if !hb_flag.load(Ordering::Acquire) {
                    break;
                }
                if let Ok(hb_client) = ApiClient::new(&hb_url, &hb_token) {
                    let _ = hb_client.heartbeat(&hb_project, &hb_task).await;
                }
            }
        });

        // Build prompt
        let title = next_task.frontmatter.title.clone();
        let description = next_task.body.clone();
        let prompt = format!(
            "# 任务: {title}\n\n## 描述\n\n{description}\n\n## 要求\n\n\
             1. 在当前代码库中实现所需的变更\n\
             2. 确保代码通过类型检查\n\
             3. 不要修改与任务无关的代码\n\
             4. 完成后简要总结你做了什么"
        );

        // Spawn agent execution in a dedicated thread with its own runtime
        // (ACP uses LocalSet which is !Send, can't use tokio::spawn)
        let cwd = workspace_path.clone();
        let agent_clone = agent.clone();
        let remaining_budget = self
            .slots
            .get(path)
            .and_then(|s| s.config.budget.map(|b| (b - s.total_cost).max(0.0)));
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build tokio runtime for agent");
            rt.block_on(async move {
                match agent::execute_agent(
                    &agent_clone,
                    AgentOptions {
                        cwd,
                        prompt,
                        max_budget_usd: remaining_budget,
                        permission_mode: Some("auto".into()),
                        allowed_tools: None,
                        mcp_configs: None,
                        model: None,
                        verbose: false,
                    },
                    None,
                )
                .await
                {
                    Ok(result) => result,
                    Err(e) => AgentResult {
                        success: false,
                        summary: format!("Agent error: {e}"),
                        cost_usd: None,
                        session_id: None,
                        exit_code: 1,
                    },
                }
            })
        });

        // Record running task
        let slot = self.slots.get_mut(path).unwrap();

        if let Some(store) = &self.event_store {
            let payload = serde_json::json!({
                "project_id": &project_id,
                "agent": &agent_name,
                "title": &title,
            });
            if let Err(e) = store.insert(
                &task_id,
                ws_id.as_deref().unwrap_or(""),
                "task_started",
                &payload.to_string(),
            ) {
                tracing::warn!("Failed to record task_started event: {e}");
            }
        }

        slot.running.push(RunningTask {
            task_id,
            project_id,
            workspace_id: ws_id,
            thread_handle: Some(handle),
            heartbeat_running,
            heartbeat_handle,
            started_at: chrono::Utc::now(),
        });

        Ok(())
    }

    async fn collect_slot_completed(&mut self, path: &str) {
        let slot = match self.slots.get_mut(path) {
            Some(s) => s,
            None => return,
        };

        let mut i = 0;
        while i < slot.running.len() {
            let is_finished = slot.running[i]
                .thread_handle
                .as_ref()
                .map(|h| h.is_finished())
                .unwrap_or(true);

            if is_finished {
                let mut task = slot.running.remove(i);
                task.heartbeat_running.store(false, Ordering::Release);
                let _ = time::timeout(Duration::from_secs(5), task.heartbeat_handle).await;

                let result = match task.thread_handle.take() {
                    Some(h) => match h.join() {
                        Ok(r) => r,
                        Err(_) => AgentResult {
                            success: false,
                            summary: "Agent thread panicked".to_string(),
                            cost_usd: None,
                            session_id: None,
                            exit_code: 1,
                        },
                    },
                    None => AgentResult {
                        success: false,
                        summary: "Agent handle missing".to_string(),
                        cost_usd: None,
                        session_id: None,
                        exit_code: 1,
                    },
                };

                // Track cost
                if let Some(cost) = result.cost_usd {
                    slot.total_cost += cost;
                }

                // Update task status
                let new_status = if result.success {
                    TaskStatus::Done
                } else {
                    // Remember failed task to avoid infinite retry
                    slot.failed_task_ids.insert(task.task_id.clone());
                    TaskStatus::Todo
                };

                if let Some(store) = &self.event_store {
                    let event_type = if result.success { "task_completed" } else { "task_failed" };
                    let payload = serde_json::json!({
                        "project_id": &task.project_id,
                        "exit_code": result.exit_code,
                        "cost_usd": result.cost_usd,
                    });
                    if let Err(e) = store.insert(
                        &task.task_id,
                        task.workspace_id.as_deref().unwrap_or(""),
                        event_type,
                        &payload.to_string(),
                    ) {
                        tracing::warn!("Failed to record {event_type} event: {e}");
                    }
                }

                let _ = self
                    .client
                    .update_task(
                        &task.project_id,
                        &task.task_id,
                        &UpdateTaskData {
                            status: Some(new_status),
                            ..Default::default()
                        },
                    )
                    .await;

                // Add comment
                let cost_str = result
                    .cost_usd
                    .map(|c| format!(" | ${:.4}", c))
                    .unwrap_or_default();
                let emoji = if result.success { "✅" } else { "❌" };
                let summary_truncated = crate::output::truncate_utf8(&result.summary, 2000);
                let comment = format!(
                    "{emoji} AutoRunner {}{cost_str}\n\n{summary_truncated}",
                    if result.success { "完成" } else { "失败" },
                );
                let _ = self
                    .client
                    .add_comment(&task.project_id, &task.task_id, &comment)
                    .await;

                // Report execution (best-effort, failure does not block unlock)
                let finished_at = chrono::Utc::now();
                let exec_report = ExecutionReport {
                    workspace_id: task.workspace_id.clone(),
                    started_at: task.started_at.to_rfc3339(),
                    finished_at: Some(finished_at.to_rfc3339()),
                    status: if result.success {
                        "succeeded"
                    } else {
                        "failed"
                    }
                    .to_string(),
                    exit_code: Some(result.exit_code),
                    cost_usd: result.cost_usd,
                    summary: Some(crate::output::truncate_utf8(&result.summary, 500).to_string()),
                    error_message: if result.success {
                        None
                    } else {
                        Some(crate::output::truncate_utf8(&result.summary, 500).to_string())
                    },
                };
                if let Err(e) = self
                    .client
                    .report_execution(&task.project_id, &task.task_id, &exec_report)
                    .await
                {
                    tracing::warn!(
                        "AutoRunner: failed to report execution for {}: {e}",
                        task.task_id
                    );
                }

                // Unlock
                let _ = self
                    .client
                    .unlock_task(&task.project_id, &task.task_id)
                    .await;

                // Archive local file if done
                if result.success {
                    let local_dir = LocalDirectory::new(Path::new(&slot.workspace_path));
                    if let Ok(Some(local_task)) = local_dir.find_task_by_id(&task.task_id) {
                        let _ = local_dir.archive_task(&local_task.file_path);
                    }
                }

                let dur = chrono::Utc::now() - task.started_at;
                tracing::info!(
                    "AutoRunner: task {} {} in {}s",
                    task.task_id,
                    if result.success {
                        "completed"
                    } else {
                        "failed"
                    },
                    dur.num_seconds()
                );
            } else {
                i += 1;
            }
        }
    }

    /// Gracefully shut down all running tasks.
    pub async fn shutdown(&mut self) {
        for slot in self.slots.values_mut() {
            for mut task in slot.running.drain(..) {
                task.heartbeat_running.store(false, Ordering::Release);
                // Drop thread handle — the thread may still be running but will finish
                task.thread_handle.take();
                task.heartbeat_handle.abort();
                // Unlock task
                let _ = self
                    .client
                    .unlock_task(&task.project_id, &task.task_id)
                    .await;
            }
        }
    }
}
