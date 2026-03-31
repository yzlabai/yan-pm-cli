use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::api::types::Issue;

use super::issuefile::{
    issue_filename, parse_issue_file, render_issue_file, IssueFrontmatter, LocalIssueFile,
};
use super::specfile::{
    parse_spec_file, render_spec_file, spec_filename, LocalSpecFile, SpecFrontmatter,
};
use super::taskfile::{parse_task_file, LocalTaskFile, TaskFrontmatter};

const YAN_PM_DIR: &str = ".yan-pm";
const TASKS_DIR: &str = "tasks";
const DONE_DIR: &str = "done";
const ISSUES_DIR: &str = "issues";
const SPECS_DIR: &str = "specs";
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

    /// Path to .yan-pm/issues/
    fn issues_dir(&self) -> PathBuf {
        self.yan_pm_dir().join(ISSUES_DIR)
    }

    /// Path to .yan-pm/specs/
    fn specs_dir(&self) -> PathBuf {
        self.yan_pm_dir().join(SPECS_DIR)
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
        fs::create_dir_all(self.issues_dir()).context("Failed to create .yan-pm/issues/")?;
        fs::create_dir_all(self.specs_dir()).context("Failed to create .yan-pm/specs/")?;

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

    /// Remove a task file by path (for cleanup).
    pub fn remove_task_file(&self, path: &Path) -> Result<()> {
        if path.exists() {
            fs::remove_file(path).context("Failed to remove task file")?;
        }
        Ok(())
    }

    // ---- Issue methods ----

    /// Scan all issue files in .yan-pm/issues/.
    pub fn scan_issues(&self) -> Result<Vec<LocalIssueFile>> {
        let dir = self.issues_dir();
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut issues = Vec::new();
        for entry in fs::read_dir(&dir).context("Failed to read issues directory")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(content) => match parse_issue_file(&content) {
                    Ok((fm, body)) => {
                        issues.push(LocalIssueFile {
                            frontmatter: fm,
                            body,
                            file_path: path,
                        });
                    }
                    Err(e) => {
                        eprintln!("⚠ 跳过无效 Issue 文件 {}: {}", path.display(), e);
                    }
                },
                Err(e) => {
                    eprintln!("⚠ 无法读取 {}: {}", path.display(), e);
                }
            }
        }

        issues.sort_by_key(|i| i.frontmatter.number);
        Ok(issues)
    }

    /// Write an issue file to .yan-pm/issues/.
    pub fn write_issue(&self, frontmatter: &IssueFrontmatter, body: &str) -> Result<PathBuf> {
        let filename = issue_filename(frontmatter.number, &frontmatter.title);
        let path = self.issues_dir().join(&filename);
        let content = render_issue_file(frontmatter, body)?;

        let tmp_path = path.with_extension("md.tmp");
        fs::write(&tmp_path, &content)?;
        fs::rename(&tmp_path, &path)?;
        Ok(path)
    }

    /// Convert a cloud Issue to IssueFrontmatter.
    fn issue_to_frontmatter(issue: &Issue) -> IssueFrontmatter {
        IssueFrontmatter {
            id: issue.id.clone(),
            number: issue.number,
            title: issue.title.clone(),
            issue_type: issue.issue_type,
            priority: issue.priority,
            status: issue.status,
            labels: issue.labels.clone(),
            acceptance_criteria: issue.acceptance_criteria.clone(),
            assignee: issue.assignee_id.clone(),
            created: issue.created_at.clone(),
            updated: issue.updated_at.clone(),
        }
    }

    /// Pull cloud issues to local files.
    pub fn pull_issues(&self, cloud_issues: &[Issue]) -> Result<PullIssueResult> {
        fs::create_dir_all(self.issues_dir()).context("Failed to create issues directory")?;

        let existing = self.scan_issues()?;
        let existing_by_id: std::collections::HashMap<String, LocalIssueFile> = existing
            .into_iter()
            .map(|i| (i.frontmatter.id.clone(), i))
            .collect();

        let mut created = 0;
        let mut updated = 0;
        let mut unchanged = 0;

        for issue in cloud_issues {
            let fm = Self::issue_to_frontmatter(issue);
            let body = issue.description.as_deref().unwrap_or("");

            match existing_by_id.get(&issue.id) {
                Some(local) => {
                    // Check if content changed (compare updated timestamps)
                    if local.frontmatter.updated == fm.updated {
                        unchanged += 1;
                    } else {
                        // Remove old file if filename changed
                        let new_filename = issue_filename(fm.number, &fm.title);
                        let old_filename = local
                            .file_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        if new_filename != old_filename {
                            let _ = fs::remove_file(&local.file_path);
                        }
                        self.write_issue(&fm, body)?;
                        updated += 1;
                    }
                }
                None => {
                    self.write_issue(&fm, body)?;
                    created += 1;
                }
            }
        }

        Ok(PullIssueResult {
            created,
            updated,
            unchanged,
        })
    }

    // ---- Spec methods ----

    /// Scan all spec files in .yan-pm/specs/.
    pub fn scan_specs(&self) -> Result<Vec<LocalSpecFile>> {
        let dir = self.specs_dir();
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut specs = Vec::new();
        for entry in fs::read_dir(&dir).context("Failed to read specs directory")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(content) => match parse_spec_file(&content) {
                    Ok((fm, body)) => {
                        specs.push(LocalSpecFile {
                            frontmatter: fm,
                            body,
                            file_path: path,
                        });
                    }
                    Err(e) => {
                        eprintln!("⚠ 跳过无效 Spec 文件 {}: {}", path.display(), e);
                    }
                },
                Err(e) => {
                    eprintln!("⚠ 无法读取 {}: {}", path.display(), e);
                }
            }
        }

        specs.sort_by_key(|s| s.frontmatter.issue);
        Ok(specs)
    }

    /// Write a spec file to .yan-pm/specs/.
    pub fn write_spec(&self, frontmatter: &SpecFrontmatter, body: &str) -> Result<PathBuf> {
        fs::create_dir_all(self.specs_dir()).context("Failed to create specs directory")?;
        let filename = spec_filename(frontmatter.issue, &frontmatter.title);
        let path = self.specs_dir().join(&filename);
        let content = render_spec_file(frontmatter, body)?;

        let tmp_path = path.with_extension("md.tmp");
        fs::write(&tmp_path, &content)?;
        fs::rename(&tmp_path, &path)?;
        Ok(path)
    }

    /// Generate task files from parsed spec tasks.
    ///
    /// Creates task files in `.yan-pm/tasks/` with filenames like `{issue:03d}-{seq:02d}-{slug}.md`.
    /// Returns the list of file paths created.
    pub fn generate_tasks_from_spec(
        &self,
        issue_number: i32,
        parsed_tasks: &[super::task_parser::ParsedTask],
    ) -> Result<Vec<PathBuf>> {
        use super::taskfile::{render_task_file, slugify};

        fs::create_dir_all(self.tasks_dir()).context("Failed to create tasks directory")?;

        let now = chrono::Utc::now().to_rfc3339();
        let mut paths = Vec::new();

        for (idx, parsed) in parsed_tasks.iter().enumerate() {
            let seq = idx + 1;
            let slug = slugify(&parsed.title);
            let filename = format!("{:03}-{:02}-{}.md", issue_number, seq, slug);

            // Build depends_on: convert dependency markers to our task ID format
            let depends_on: Vec<String> = parsed
                .depends_on
                .iter()
                .map(|d| {
                    // If dep is like "001-01", keep as-is; otherwise prefix with issue number
                    if d.contains('-') {
                        d.clone()
                    } else {
                        format!("{:03}-{}", issue_number, d)
                    }
                })
                .collect();

            let fm = TaskFrontmatter {
                id: None,
                number: None,
                title: parsed.title.clone(),
                task_type: crate::api::types::TaskType::Task,
                priority: crate::api::types::TaskPriority::Medium,
                status: if parsed.checked {
                    crate::api::types::TaskStatus::Done
                } else {
                    crate::api::types::TaskStatus::Todo
                },
                tags: Vec::new(),
                depends_on,
                assignee: None,
                issue: Some(issue_number),
                due: None,
                requires: Vec::new(),
                created: now.clone(),
                updated: now.clone(),
            };

            let body = if parsed.description.is_empty() {
                String::new()
            } else {
                parsed.description.clone()
            };

            let path = self.tasks_dir().join(&filename);
            let content = render_task_file(&fm, &body)?;
            let tmp_path = path.with_extension("md.tmp");
            fs::write(&tmp_path, &content)?;
            fs::rename(&tmp_path, &path)?;

            paths.push(path);
        }

        Ok(paths)
    }

    /// Check if tasks already exist for a given issue number.
    pub fn has_tasks_for_issue(&self, issue_number: i32) -> Result<bool> {
        let tasks = self.scan_tasks()?;
        Ok(tasks
            .iter()
            .any(|t| t.frontmatter.issue == Some(issue_number)))
    }

    /// Find spec file by issue number.
    pub fn find_spec_by_issue(&self, issue_number: i32) -> Result<Option<LocalSpecFile>> {
        let specs = self.scan_specs()?;
        Ok(specs
            .into_iter()
            .find(|s| s.frontmatter.issue == issue_number))
    }
}

/// Result of an issue pull operation.
#[derive(Debug)]
pub struct PullIssueResult {
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

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
