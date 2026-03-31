use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;

use crate::api::types::{TaskPriority, TaskStatus, TaskType};
use crate::local::directory::LocalDirectory;
use crate::local::taskfile::TaskFrontmatter;

/// In-memory cache of last synced task state, keyed by task ID.
/// Used to detect local changes since last sync.
type MemoryCache = HashMap<String, CachedTask>;

#[derive(Debug, Clone)]
struct CachedTask {
    status: TaskStatus,
    priority: TaskPriority,
    title: String,
    assignee: Option<String>,
    task_type: TaskType,
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

/// Sync engine for a single workspace.
/// Currently only manages local task files. Cloud sync will be repurposed for Issues.
pub struct SyncEngine {
    local_dir: LocalDirectory,
    project_id: String,
    cache: MemoryCache,
}

/// Result of a sync operation.
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

    /// Full sync — currently a no-op placeholder. Will be repurposed for Issue sync.
    pub async fn full_sync(
        &mut self,
        _client: &crate::api::client::ApiClient,
    ) -> Result<SyncResult> {
        let result = SyncResult::default();
        self.init_cache()?;
        self.local_dir.update_last_sync()?;
        Ok(result)
    }
}

/// Types of field changes detected between local file and cache.
#[derive(Debug)]
enum FieldChange {
    Title(String),
    Status(TaskStatus),
    Priority(TaskPriority),
    Assignee(Option<String>),
    TaskType(TaskType),
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
            requires: vec![],
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
