use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::api::client::{ApiClient, CreateTaskData, TaskListParams, UpdateTaskData};
use crate::api::types::{Task, TaskStatus};
use crate::local::directory::LocalDirectory;
use crate::local::taskfile::TaskFrontmatter;

/// In-memory cache of last synced task state, keyed by task ID.
/// Used to detect local changes since last sync.
type MemoryCache = HashMap<String, CachedTask>;

#[derive(Debug, Clone)]
struct CachedTask {
    status: TaskStatus,
    priority: crate::api::types::TaskPriority,
    title: String,
    assignee: Option<String>,
    task_type: crate::api::types::TaskType,
    updated: String,
}

impl CachedTask {
    fn from_frontmatter(fm: &TaskFrontmatter) -> Option<Self> {
        fm.id.as_ref()?; // Only cache tasks with server IDs
        Some(Self {
            status: fm.status,
            priority: fm.priority,
            title: fm.title.clone(),
            assignee: fm.assignee.clone(),
            task_type: fm.task_type,
            updated: fm.updated.clone(),
        })
    }
}

/// Bidirectional sync engine for a single workspace.
pub struct SyncEngine {
    local_dir: LocalDirectory,
    project_id: String,
    cache: MemoryCache,
}

/// Result of a full sync operation.
#[derive(Debug, Default)]
pub struct SyncResult {
    pub pulled: usize,
    pub pushed: usize,
    pub archived: usize,
    pub conflicts: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for SyncResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "↓{} 拉取  ↑{} 推送  📦{} 归档",
            self.pulled, self.pushed, self.archived
        )?;
        if self.conflicts > 0 {
            write!(f, "  ⚠{} 冲突", self.conflicts)?;
        }
        Ok(())
    }
}

impl SyncEngine {
    pub fn new(workspace_root: &Path, project_id: &str) -> Self {
        Self {
            local_dir: LocalDirectory::new(workspace_root),
            project_id: project_id.to_string(),
            cache: HashMap::new(),
        }
    }

    /// Initialize the memory cache from current local files.
    pub fn init_cache(&mut self) -> Result<()> {
        let tasks = self.local_dir.scan_tasks()?;
        self.cache.clear();
        for task in &tasks {
            if let Some(cached) = CachedTask::from_frontmatter(&task.frontmatter) {
                if let Some(id) = &task.frontmatter.id {
                    self.cache.insert(id.clone(), cached);
                }
            }
        }
        Ok(())
    }

    /// Full bidirectional sync.
    ///
    /// 1. Detect local changes BEFORE pulling (compare current files with cache)
    /// 2. Pull all cloud tasks → create/update/archive local files
    /// 3. Push local-only tasks (id=None) to cloud
    /// 4. Push pre-detected local changes (with LWW conflict resolution)
    /// 5. Update cache + last_sync timestamp
    pub async fn full_sync(&mut self, client: &ApiClient) -> Result<SyncResult> {
        let mut result = SyncResult::default();

        // 1. Detect local changes BEFORE pulling — pull overwrites local files,
        //    so we must capture edits first to avoid silent data loss.
        let pre_pull_local = self.local_dir.scan_tasks()?;
        let mut local_changes: Vec<(String, String, Vec<FieldChange>)> = Vec::new();
        let mut new_local_tasks: Vec<crate::local::taskfile::LocalTaskFile> = Vec::new();
        for local in &pre_pull_local {
            if local.frontmatter.id.is_none() {
                new_local_tasks.push(local.clone());
                continue;
            }
            let task_id = local.frontmatter.id.as_ref().unwrap();
            if let Some(cached) = self.cache.get(task_id) {
                let changes = detect_changes(&local.frontmatter, cached);
                if !changes.is_empty() {
                    local_changes.push((task_id.clone(), local.frontmatter.title.clone(), changes));
                }
            }
        }

        // 2. Pull from cloud
        let cloud_tasks = client
            .list_tasks(&self.project_id, &TaskListParams::default())
            .await
            .context("Failed to fetch tasks from server")?;

        let pull_result = self.local_dir.pull_tasks(&cloud_tasks)?;
        result.pulled = pull_result.created + pull_result.updated;
        result.archived = pull_result.archived;

        // Build cloud task map for conflict detection
        let cloud_map: HashMap<String, &Task> =
            cloud_tasks.iter().map(|t| (t.id.clone(), t)).collect();

        // 2b. Archive orphaned local files (server-deleted tasks)
        let post_pull_local = self.local_dir.scan_tasks()?;
        for local in &post_pull_local {
            if let Some(id) = &local.frontmatter.id {
                if !cloud_map.contains_key(id.as_str()) {
                    if let Err(e) = self.local_dir.archive_task(&local.file_path) {
                        result.errors.push(format!(
                            "归档孤儿任务 '{}' 失败: {e}",
                            local.frontmatter.title
                        ));
                    } else {
                        result.archived += 1;
                    }
                }
            }
        }

        // 3. Push local-only tasks (no server ID) to cloud
        for local in &new_local_tasks {
            match self.push_new_task(client, local).await {
                Ok(_) => result.pushed += 1,
                Err(e) => result.errors.push(format!(
                    "推送新任务 '{}' 失败: {}",
                    local.frontmatter.title, e
                )),
            }
        }

        // 4. Push pre-detected local changes
        // Note: conflict resolution uses cloud-priority strategy. Local edits don't
        // update the YAML `updated` field, so when both sides changed, cloud wins.
        // Local-only changes (cloud unchanged since last sync) are always pushed.
        for (task_id, title, changes) in &local_changes {
            if let Some(cloud_task) = cloud_map.get(task_id.as_str()) {
                if let Some(cached) = self.cache.get(task_id) {
                    if cloud_task.updated_at != cached.updated {
                        // Cloud changed since last sync — cloud wins, skip local push
                        result.conflicts += 1;
                        continue;
                    }
                }
            }

            match self.push_changes(client, task_id, changes).await {
                Ok(_) => result.pushed += 1,
                Err(e) => result
                    .errors
                    .push(format!("推送任务 '{title}' 变更失败: {e}",)),
            }
        }

        // 5. Update cache
        self.init_cache()?;
        self.local_dir.update_last_sync()?;

        Ok(result)
    }

    /// Pull only: fetch cloud tasks and update local files.
    #[allow(dead_code)]
    pub async fn pull(&mut self, client: &ApiClient) -> Result<SyncResult> {
        let mut result = SyncResult::default();

        let cloud_tasks = client
            .list_tasks(&self.project_id, &TaskListParams::default())
            .await
            .context("Failed to fetch tasks from server")?;

        let pull_result = self.local_dir.pull_tasks(&cloud_tasks)?;
        result.pulled = pull_result.created + pull_result.updated;
        result.archived = pull_result.archived;

        self.init_cache()?;
        self.local_dir.update_last_sync()?;

        Ok(result)
    }

    /// Push a locally-created task (no server ID) to cloud.
    /// Updates the local file with the returned server ID.
    async fn push_new_task(
        &self,
        client: &ApiClient,
        local: &crate::local::taskfile::LocalTaskFile,
    ) -> Result<()> {
        let data = CreateTaskData {
            title: local.frontmatter.title.clone(),
            description: if local.body.is_empty() {
                None
            } else {
                Some(local.body.clone())
            },
            task_type: Some(local.frontmatter.task_type),
            priority: Some(local.frontmatter.priority),
            assignee_id: local.frontmatter.assignee.clone(),
            due_date: local.frontmatter.due.clone(),
            tags: if local.frontmatter.tags.is_empty() {
                None
            } else {
                Some(local.frontmatter.tags.clone())
            },
        };

        let created = client.create_task(&self.project_id, &data).await?;

        // Update local file with server ID and number
        let mut fm = local.frontmatter.clone();
        fm.id = Some(created.id);
        fm.number = created.number;
        fm.updated = created.updated_at;

        // Write new file first, then remove old (atomic: crash-safe)
        let new_path = self.local_dir.write_task(&fm, &local.body)?;
        if new_path != local.file_path {
            self.local_dir.remove_task_file(&local.file_path)?;
        }

        Ok(())
    }

    /// Push detected field changes to cloud.
    async fn push_changes(
        &self,
        client: &ApiClient,
        task_id: &str,
        changes: &[FieldChange],
    ) -> Result<()> {
        let mut data = UpdateTaskData {
            title: None,
            status: None,
            priority: None,
            assignee_id: None,
            task_type: None,
        };

        for change in changes {
            match change {
                FieldChange::Title(v) => data.title = Some(v.clone()),
                FieldChange::Status(v) => data.status = Some(*v),
                FieldChange::Priority(v) => data.priority = Some(*v),
                FieldChange::Assignee(v) => data.assignee_id = v.clone(),
                FieldChange::TaskType(v) => data.task_type = Some(*v),
            }
        }

        client.update_task(&self.project_id, task_id, &data).await?;
        Ok(())
    }
}

/// Types of field changes detected between local file and cache.
#[derive(Debug)]
enum FieldChange {
    Title(String),
    Status(TaskStatus),
    Priority(crate::api::types::TaskPriority),
    Assignee(Option<String>),
    TaskType(crate::api::types::TaskType),
}

/// Compare a local task's frontmatter with the cached version.
fn detect_changes(local: &TaskFrontmatter, cached: &CachedTask) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    if local.title != cached.title {
        changes.push(FieldChange::Title(local.title.clone()));
    }
    if local.status != cached.status {
        changes.push(FieldChange::Status(local.status));
    }
    if local.priority != cached.priority {
        changes.push(FieldChange::Priority(local.priority));
    }
    if local.assignee != cached.assignee {
        changes.push(FieldChange::Assignee(local.assignee.clone()));
    }
    if local.task_type != cached.task_type {
        changes.push(FieldChange::TaskType(local.task_type));
    }

    changes
}

/// Print sync result to terminal with colors.
pub fn print_sync_result(result: &SyncResult) {
    println!("{}", format!("✓ 同步完成: {result}").green());
    if !result.errors.is_empty() {
        for err in &result.errors {
            eprintln!("  {} {}", "⚠".yellow(), err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{TaskPriority, TaskStatus, TaskType};

    fn make_frontmatter(
        title: &str,
        status: TaskStatus,
        priority: TaskPriority,
        assignee: Option<&str>,
    ) -> TaskFrontmatter {
        TaskFrontmatter {
            id: Some("task-1".into()),
            number: Some(1),
            title: title.into(),
            task_type: TaskType::Feature,
            priority,
            status,
            tags: vec![],
            depends_on: vec![],
            assignee: assignee.map(String::from),
            issue: None,
            due: None,
            created: "2026-03-25T00:00:00Z".into(),
            updated: "2026-03-25T10:00:00Z".into(),
        }
    }

    fn make_cached(
        title: &str,
        status: TaskStatus,
        priority: TaskPriority,
        assignee: Option<&str>,
    ) -> CachedTask {
        CachedTask {
            status,
            priority,
            title: title.into(),
            assignee: assignee.map(String::from),
            task_type: TaskType::Feature,
            updated: "2026-03-25T00:00:00Z".into(),
        }
    }

    #[test]
    fn test_detect_no_changes() {
        let fm = make_frontmatter("Task A", TaskStatus::Todo, TaskPriority::Medium, None);
        let cached = make_cached("Task A", TaskStatus::Todo, TaskPriority::Medium, None);
        let changes = detect_changes(&fm, &cached);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_detect_status_change() {
        let fm = make_frontmatter("Task A", TaskStatus::InProgress, TaskPriority::Medium, None);
        let cached = make_cached("Task A", TaskStatus::Todo, TaskPriority::Medium, None);
        let changes = detect_changes(&fm, &cached);
        assert_eq!(changes.len(), 1);
        assert!(matches!(
            changes[0],
            FieldChange::Status(TaskStatus::InProgress)
        ));
    }

    #[test]
    fn test_detect_priority_change() {
        let fm = make_frontmatter("Task A", TaskStatus::Todo, TaskPriority::Urgent, None);
        let cached = make_cached("Task A", TaskStatus::Todo, TaskPriority::Medium, None);
        let changes = detect_changes(&fm, &cached);
        assert_eq!(changes.len(), 1);
        assert!(matches!(
            changes[0],
            FieldChange::Priority(TaskPriority::Urgent)
        ));
    }

    #[test]
    fn test_detect_title_change() {
        let fm = make_frontmatter("Task B", TaskStatus::Todo, TaskPriority::Medium, None);
        let cached = make_cached("Task A", TaskStatus::Todo, TaskPriority::Medium, None);
        let changes = detect_changes(&fm, &cached);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FieldChange::Title(t) if t == "Task B"));
    }

    #[test]
    fn test_detect_assignee_change() {
        let fm = make_frontmatter(
            "Task A",
            TaskStatus::Todo,
            TaskPriority::Medium,
            Some("user-1"),
        );
        let cached = make_cached("Task A", TaskStatus::Todo, TaskPriority::Medium, None);
        let changes = detect_changes(&fm, &cached);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FieldChange::Assignee(Some(a)) if a == "user-1"));
    }

    #[test]
    fn test_detect_multiple_changes() {
        let fm = make_frontmatter(
            "New Title",
            TaskStatus::Done,
            TaskPriority::High,
            Some("user-2"),
        );
        let cached = make_cached("Old Title", TaskStatus::Todo, TaskPriority::Low, None);
        let changes = detect_changes(&fm, &cached);
        assert_eq!(changes.len(), 4);
    }

    #[test]
    fn test_cached_task_from_frontmatter_with_id() {
        let fm = make_frontmatter("Task", TaskStatus::Todo, TaskPriority::Medium, None);
        let cached = CachedTask::from_frontmatter(&fm);
        assert!(cached.is_some());
        let c = cached.unwrap();
        assert_eq!(c.title, "Task");
        assert!(matches!(c.status, TaskStatus::Todo));
    }

    #[test]
    fn test_cached_task_from_frontmatter_without_id() {
        let mut fm = make_frontmatter("Task", TaskStatus::Todo, TaskPriority::Medium, None);
        fm.id = None;
        let cached = CachedTask::from_frontmatter(&fm);
        assert!(cached.is_none());
    }

    #[test]
    fn test_sync_result_display() {
        let result = SyncResult {
            pulled: 3,
            pushed: 1,
            archived: 2,
            conflicts: 0,
            errors: vec![],
        };
        let s = format!("{result}");
        assert!(s.contains("↓3"));
        assert!(s.contains("↑1"));
        assert!(s.contains("📦2"));
        assert!(!s.contains("⚠"));
    }

    #[test]
    fn test_sync_result_display_with_conflicts() {
        let result = SyncResult {
            pulled: 0,
            pushed: 0,
            archived: 0,
            conflicts: 2,
            errors: vec![],
        };
        let s = format!("{result}");
        assert!(s.contains("⚠2"));
    }
}
