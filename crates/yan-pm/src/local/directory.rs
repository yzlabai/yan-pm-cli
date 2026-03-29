use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::api::types::Task;

use super::taskfile::{
    parse_task_file, render_task_file, task_filename, LocalTaskFile, TaskFrontmatter,
};

const YAN_PM_DIR: &str = ".yan-pm";
const TASKS_DIR: &str = "tasks";
const DONE_DIR: &str = "done";
const LOCAL_CONFIG: &str = "config.json";

/// Auto-run configuration for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRunConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Max budget in USD (None = unlimited)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<f64>,
    /// Max concurrent agent executions
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    /// Only run tasks with these priorities (empty = all)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter_priority: Vec<String>,
    /// Agent to use (default: "claude")
    #[serde(default = "default_agent")]
    pub agent: String,
}

fn default_concurrency() -> u32 {
    1
}

fn default_agent() -> String {
    "claude".to_string()
}

impl Default for AutoRunConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            budget: None,
            concurrency: 1,
            filter_priority: Vec::new(),
            agent: "claude".to_string(),
        }
    }
}

/// Local workspace config stored at .yan-pm/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalWorkspaceConfig {
    pub project_id: String,
    pub project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<String>,
    #[serde(default, skip_serializing_if = "is_auto_run_default")]
    pub auto_run: AutoRunConfig,
}

fn is_auto_run_default(config: &AutoRunConfig) -> bool {
    !config.enabled
        && config.budget.is_none()
        && config.concurrency == 1
        && config.filter_priority.is_empty()
        && config.agent == "claude"
}

/// Manages the .yan-pm/ directory for a workspace.
pub struct LocalDirectory {
    /// Root of the workspace (where .yan-pm/ lives)
    root: PathBuf,
}

impl LocalDirectory {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            root: workspace_root.to_path_buf(),
        }
    }

    /// Path to .yan-pm/
    fn yan_pm_dir(&self) -> PathBuf {
        self.root.join(YAN_PM_DIR)
    }

    /// Path to .yan-pm/tasks/
    fn tasks_dir(&self) -> PathBuf {
        self.yan_pm_dir().join(TASKS_DIR)
    }

    /// Path to .yan-pm/done/
    fn done_dir(&self) -> PathBuf {
        self.yan_pm_dir().join(DONE_DIR)
    }

    /// Path to .yan-pm/config.json
    fn config_path(&self) -> PathBuf {
        self.yan_pm_dir().join(LOCAL_CONFIG)
    }

    /// Check whether .yan-pm/ directory exists
    pub fn is_initialized(&self) -> bool {
        self.yan_pm_dir().exists()
    }

    /// Create .yan-pm/tasks/ and .yan-pm/done/ directories.
    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(self.tasks_dir()).context("Failed to create .yan-pm/tasks/")?;
        fs::create_dir_all(self.done_dir()).context("Failed to create .yan-pm/done/")?;

        // Add .yan-pm to .gitignore if it exists and doesn't already contain it
        let gitignore = self.root.join(".gitignore");
        if gitignore.exists() {
            let content = fs::read_to_string(&gitignore).unwrap_or_default();
            if !content
                .lines()
                .any(|l| l.trim() == ".yan-pm" || l.trim() == ".yan-pm/")
            {
                let mut new_content = content;
                if !new_content.ends_with('\n') {
                    new_content.push('\n');
                }
                new_content.push_str(".yan-pm/\n");
                fs::write(&gitignore, new_content)?;
            }
        }
        Ok(())
    }

    /// Read local workspace config.
    pub fn load_config(&self) -> Option<LocalWorkspaceConfig> {
        let path = self.config_path();
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save local workspace config (atomic write).
    pub fn save_config(&self, config: &LocalWorkspaceConfig) -> Result<()> {
        let path = self.config_path();
        let content = serde_json::to_string_pretty(config)? + "\n";
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &content)?;
        fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// Update the last_sync timestamp in local config.
    pub fn update_last_sync(&self) -> Result<()> {
        if let Some(mut config) = self.load_config() {
            config.last_sync = Some(chrono::Utc::now().to_rfc3339());
            self.save_config(&config)?;
        }
        Ok(())
    }

    /// Scan all task files in .yan-pm/tasks/.
    pub fn scan_tasks(&self) -> Result<Vec<LocalTaskFile>> {
        let dir = self.tasks_dir();
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut tasks = Vec::new();
        for entry in fs::read_dir(&dir).context("Failed to read tasks directory")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(content) => match parse_task_file(&content) {
                    Ok((fm, body)) => {
                        tasks.push(LocalTaskFile {
                            frontmatter: fm,
                            body,
                            file_path: path,
                        });
                    }
                    Err(e) => {
                        eprintln!("⚠ 跳过无效任务文件 {}: {}", path.display(), e);
                    }
                },
                Err(e) => {
                    eprintln!("⚠ 无法读取 {}: {}", path.display(), e);
                }
            }
        }

        // Sort by number (ascending), then by title
        tasks.sort_by(|a, b| {
            a.frontmatter
                .number
                .cmp(&b.frontmatter.number)
                .then_with(|| a.frontmatter.title.cmp(&b.frontmatter.title))
        });

        Ok(tasks)
    }

    /// Write a task file to .yan-pm/tasks/.
    /// Returns the file path written.
    pub fn write_task(&self, frontmatter: &TaskFrontmatter, body: &str) -> Result<PathBuf> {
        let filename = task_filename(frontmatter.number, &frontmatter.title);
        let path = self.tasks_dir().join(&filename);
        let content = render_task_file(frontmatter, body)?;

        // Atomic write
        let tmp_path = path.with_extension("md.tmp");
        fs::write(&tmp_path, &content)?;
        fs::rename(&tmp_path, &path)?;
        Ok(path)
    }

    /// Archive a task file to .yan-pm/done/ (for Done/Cancelled tasks).
    pub fn archive_task(&self, task_path: &Path) -> Result<()> {
        if !task_path.exists() {
            return Ok(());
        }
        let filename = task_path.file_name().context("Invalid task file path")?;
        let dest = self.done_dir().join(filename);
        fs::rename(task_path, &dest).context("Failed to archive task file")?;
        Ok(())
    }

    /// Find a task file by server task ID.
    pub fn find_task_by_id(&self, task_id: &str) -> Result<Option<LocalTaskFile>> {
        let tasks = self.scan_tasks()?;
        Ok(tasks
            .into_iter()
            .find(|t| t.frontmatter.id.as_deref() == Some(task_id)))
    }

    /// Remove a task file by path (for cleanup).
    pub fn remove_task_file(&self, path: &Path) -> Result<()> {
        if path.exists() {
            fs::remove_file(path).context("Failed to remove task file")?;
        }
        Ok(())
    }

    /// Convert a server Task to a TaskFrontmatter.
    pub fn task_to_frontmatter(task: &Task) -> TaskFrontmatter {
        TaskFrontmatter {
            id: Some(task.id.clone()),
            number: task.number,
            title: task.title.clone(),
            task_type: task.task_type,
            priority: task.priority,
            status: task.status,
            tags: task.tags.clone(),
            depends_on: task.depends_on.clone(),
            assignee: task.assignee_id.clone(),
            issue: None,
            due: task.due_date.clone(),
            requires: vec![],
            created: task.created_at.clone(),
            updated: task.updated_at.clone(),
        }
    }

    /// Write all cloud tasks to local files (full pull).
    /// Archives Done/Cancelled tasks, creates/updates active ones.
    pub fn pull_tasks(&self, tasks: &[Task]) -> Result<PullResult> {
        let mut created = 0;
        let mut updated = 0;
        let mut archived = 0;

        // Build a map of existing local tasks by server ID
        let existing = self.scan_tasks()?;
        let existing_by_id: std::collections::HashMap<String, LocalTaskFile> = existing
            .into_iter()
            .filter_map(|t| t.frontmatter.id.clone().map(|id| (id, t)))
            .collect();

        for task in tasks {
            let fm = Self::task_to_frontmatter(task);
            let body = task.description.as_deref().unwrap_or("");

            match (fm.status, existing_by_id.get(&task.id)) {
                // Done/Cancelled → archive if exists locally
                (
                    crate::api::types::TaskStatus::Done | crate::api::types::TaskStatus::Cancelled,
                    Some(local),
                ) => {
                    self.archive_task(&local.file_path)?;
                    archived += 1;
                }
                // Done/Cancelled but not local → write to done/ directly
                (
                    crate::api::types::TaskStatus::Done | crate::api::types::TaskStatus::Cancelled,
                    None,
                ) => {
                    let filename = task_filename(fm.number, &fm.title);
                    let path = self.done_dir().join(&filename);
                    let content = render_task_file(&fm, body)?;
                    fs::write(&path, &content)?;
                    archived += 1;
                }
                // Active task exists locally → update
                (_, Some(local)) => {
                    // Remove old file if filename changed (due to number/title change)
                    let new_filename = task_filename(fm.number, &fm.title);
                    let old_filename = local
                        .file_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if new_filename != old_filename {
                        self.remove_task_file(&local.file_path)?;
                    }
                    self.write_task(&fm, body)?;
                    updated += 1;
                }
                // Active task not local → create
                (_, None) => {
                    self.write_task(&fm, body)?;
                    created += 1;
                }
            }
        }

        Ok(PullResult {
            created,
            updated,
            archived,
        })
    }
}

/// Result of a pull operation.
#[derive(Debug)]
pub struct PullResult {
    pub created: usize,
    pub updated: usize,
    pub archived: usize,
}

impl std::fmt::Display for PullResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "拉取完成: {} 新建, {} 更新, {} 归档",
            self.created, self.updated, self.archived
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::*;

    fn make_task(id: &str, number: Option<i32>, title: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            project_id: "proj-1".to_string(),
            title: title.to_string(),
            description: Some("Test body".to_string()),
            task_type: TaskType::Task,
            priority: TaskPriority::Medium,
            status,
            tags: vec![],
            depends_on: vec![],
            sort_order: None,
            due_date: None,
            locked_by: None,
            locked_at: None,
            last_heartbeat: None,
            number,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            assignee_id: None,
            creator_id: None,
        }
    }

    #[test]
    fn test_init_creates_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let ld = LocalDirectory::new(tmp.path());
        assert!(!ld.is_initialized());

        ld.init().unwrap();
        assert!(ld.is_initialized());
        assert!(ld.tasks_dir().exists());
        assert!(ld.done_dir().exists());
    }

    #[test]
    fn test_init_appends_gitignore() {
        let tmp = tempfile::tempdir().unwrap();
        let gitignore = tmp.path().join(".gitignore");
        std::fs::write(&gitignore, "node_modules/\n").unwrap();

        let ld = LocalDirectory::new(tmp.path());
        ld.init().unwrap();

        let content = std::fs::read_to_string(&gitignore).unwrap();
        assert!(content.contains(".yan-pm/"));

        // Second init should NOT duplicate
        ld.init().unwrap();
        let content2 = std::fs::read_to_string(&gitignore).unwrap();
        assert_eq!(
            content2.matches(".yan-pm/").count(),
            1,
            "should not duplicate .yan-pm/ entry"
        );
    }

    #[test]
    fn test_save_and_load_config() {
        let tmp = tempfile::tempdir().unwrap();
        let ld = LocalDirectory::new(tmp.path());
        ld.init().unwrap();

        assert!(ld.load_config().is_none());

        let cfg = LocalWorkspaceConfig {
            project_id: "proj-abc".into(),
            project_name: "My Project".into(),
            last_sync: None,
            auto_run: AutoRunConfig::default(),
        };
        ld.save_config(&cfg).unwrap();

        let loaded = ld.load_config().unwrap();
        assert_eq!(loaded.project_id, "proj-abc");
        assert_eq!(loaded.project_name, "My Project");
        assert!(loaded.last_sync.is_none());
    }

    #[test]
    fn test_scan_tasks_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let ld = LocalDirectory::new(tmp.path());
        ld.init().unwrap();

        let tasks = ld.scan_tasks().unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_write_and_scan_task() {
        let tmp = tempfile::tempdir().unwrap();
        let ld = LocalDirectory::new(tmp.path());
        ld.init().unwrap();

        let fm = TaskFrontmatter {
            id: Some("task-001".into()),
            number: Some(1),
            title: "Implement feature".into(),
            task_type: TaskType::Feature,
            priority: TaskPriority::High,
            status: TaskStatus::Todo,
            tags: vec!["v1".into()],
            depends_on: vec![],
            assignee: None,
            issue: None,
            due: None,
            requires: vec![],
            created: "2026-01-01T00:00:00Z".into(),
            updated: "2026-01-01T00:00:00Z".into(),
        };
        let path = ld.write_task(&fm, "Description here").unwrap();
        assert!(path.exists());

        let tasks = ld.scan_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].frontmatter.title, "Implement feature");
        assert_eq!(tasks[0].frontmatter.id.as_deref(), Some("task-001"));
    }

    #[test]
    fn test_archive_task() {
        let tmp = tempfile::tempdir().unwrap();
        let ld = LocalDirectory::new(tmp.path());
        ld.init().unwrap();

        let fm = TaskFrontmatter {
            id: Some("task-002".into()),
            number: Some(2),
            title: "Fix bug".into(),
            task_type: TaskType::Bug,
            priority: TaskPriority::Urgent,
            status: TaskStatus::InProgress,
            tags: vec![],
            depends_on: vec![],
            assignee: None,
            issue: None,
            due: None,
            requires: vec![],
            created: "2026-01-01T00:00:00Z".into(),
            updated: "2026-01-01T00:00:00Z".into(),
        };
        let path = ld.write_task(&fm, "Bug details").unwrap();
        assert!(path.exists());

        ld.archive_task(&path).unwrap();
        assert!(!path.exists());

        // The file should be in done/
        let done_files: Vec<_> = std::fs::read_dir(ld.done_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(done_files.len(), 1);
    }

    #[test]
    fn test_find_task_by_id() {
        let tmp = tempfile::tempdir().unwrap();
        let ld = LocalDirectory::new(tmp.path());
        ld.init().unwrap();

        let fm = TaskFrontmatter {
            id: Some("task-xyz".into()),
            number: Some(5),
            title: "Searchable".into(),
            task_type: TaskType::Task,
            priority: TaskPriority::Low,
            status: TaskStatus::Todo,
            tags: vec![],
            depends_on: vec![],
            assignee: None,
            issue: None,
            due: None,
            requires: vec![],
            created: "2026-01-01T00:00:00Z".into(),
            updated: "2026-01-01T00:00:00Z".into(),
        };
        ld.write_task(&fm, "").unwrap();

        assert!(ld.find_task_by_id("task-xyz").unwrap().is_some());
        assert!(ld.find_task_by_id("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_task_to_frontmatter() {
        let task = make_task("id-1", Some(3), "My Task", TaskStatus::InProgress);
        let fm = LocalDirectory::task_to_frontmatter(&task);
        assert_eq!(fm.id.as_deref(), Some("id-1"));
        assert_eq!(fm.number, Some(3));
        assert_eq!(fm.title, "My Task");
        assert_eq!(fm.status, TaskStatus::InProgress);
        assert_eq!(fm.priority, TaskPriority::Medium);
    }

    #[test]
    fn test_pull_tasks_creates_and_archives() {
        let tmp = tempfile::tempdir().unwrap();
        let ld = LocalDirectory::new(tmp.path());
        ld.init().unwrap();

        let tasks = vec![
            make_task("t1", Some(1), "Active one", TaskStatus::Todo),
            make_task("t2", Some(2), "Done one", TaskStatus::Done),
            make_task("t3", Some(3), "Active two", TaskStatus::InProgress),
        ];

        let result = ld.pull_tasks(&tasks).unwrap();
        assert_eq!(result.created, 2); // t1, t3
        assert_eq!(result.archived, 1); // t2

        let active = ld.scan_tasks().unwrap();
        assert_eq!(active.len(), 2);

        let done_files: Vec<_> = std::fs::read_dir(ld.done_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(done_files.len(), 1);
    }

    #[test]
    fn test_pull_result_display() {
        let r = PullResult {
            created: 3,
            updated: 1,
            archived: 2,
        };
        let s = format!("{}", r);
        assert!(s.contains("3 新建"));
        assert!(s.contains("1 更新"));
        assert!(s.contains("2 归档"));
    }

    #[test]
    fn test_auto_run_config_defaults() {
        let cfg = AutoRunConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.budget.is_none());
        assert_eq!(cfg.concurrency, 1);
        assert!(cfg.filter_priority.is_empty());
        assert_eq!(cfg.agent, "claude");
        assert!(is_auto_run_default(&cfg));
    }

    #[test]
    fn test_auto_run_config_serde() {
        let json = r#"{"enabled":true,"budget":10.5,"concurrency":3,"filterPriority":["urgent","high"],"agent":"gpt"}"#;
        let cfg: AutoRunConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.budget, Some(10.5));
        assert_eq!(cfg.concurrency, 3);
        assert_eq!(cfg.filter_priority, vec!["urgent", "high"]);
        assert_eq!(cfg.agent, "gpt");
        assert!(!is_auto_run_default(&cfg));
    }
}
