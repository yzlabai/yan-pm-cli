use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::api::types::{TaskPriority, TaskStatus, TaskType};

/// YAML frontmatter for a local task file (.yan-pm/tasks/*.md)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFrontmatter {
    /// Server-side task ID. None = locally created, awaiting cloud backfill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Sequential task number within the project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<i32>,
    pub title: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    /// Agent capability requirements (e.g. ["images", "mcp", "worktree"])
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    pub created: String,
    pub updated: String,
}

/// A local task file = frontmatter + markdown body.
#[derive(Debug, Clone)]
pub struct LocalTaskFile {
    pub frontmatter: TaskFrontmatter,
    pub body: String,
    pub file_path: std::path::PathBuf,
}

/// Parse a task file content into frontmatter + body.
/// Format:
/// ```text
/// ---
/// id: abc-123
/// title: Fix login bug
/// ...
/// ---
///
/// Markdown body here.
/// ```
pub fn parse_task_file(content: &str) -> Result<(TaskFrontmatter, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        anyhow::bail!("Task file missing YAML frontmatter (must start with ---)");
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let close_idx = after_first
        .find("\n---")
        .context("Task file missing closing --- for frontmatter")?;

    let yaml_str = &after_first[..close_idx].trim();
    let body_start = 3 + close_idx + 4; // skip "---" + "\n---"
    let body = if body_start < trimmed.len() {
        trimmed[body_start..]
            .trim_start_matches(&['\r', '\n'][..])
            .to_string()
    } else {
        String::new()
    };

    let frontmatter: TaskFrontmatter =
        serde_yaml::from_str(yaml_str).context("Failed to parse YAML frontmatter")?;

    Ok((frontmatter, body))
}

/// Render a task file from frontmatter + body back to Markdown string.
pub fn render_task_file(frontmatter: &TaskFrontmatter, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(frontmatter).context("Failed to serialize frontmatter")?;
    // serde_yaml may prepend "---\n" on some versions — strip to avoid duplication
    let yaml = yaml.strip_prefix("---\n").unwrap_or(&yaml);
    let mut output = String::new();
    output.push_str("---\n");
    output.push_str(yaml);
    output.push_str("---\n");
    if !body.is_empty() {
        output.push('\n');
        output.push_str(body);
        if !body.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok(output)
}

/// Generate a slug from a title for use in filenames.
/// Rules: lowercase, keep [a-z0-9-_], spaces → '-', truncate to 50 chars.
/// Falls back to "task" if the title produces an empty slug (e.g. CJK-only titles).
pub fn slugify(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            ' ' | '\t' => '-',
            _ => '-',
        })
        .collect();

    // Collapse multiple dashes
    let mut result = String::with_capacity(slug.len());
    let mut prev_dash = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_dash {
                result.push(c);
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }

    // Trim leading/trailing dashes
    let trimmed = result.trim_matches('-');

    // Fallback for empty slugs (e.g. CJK-only titles)
    if trimmed.is_empty() {
        return "task".to_string();
    }

    // Truncate to 50 chars on a word boundary
    if trimmed.len() <= 50 {
        trimmed.to_string()
    } else {
        let truncated = &trimmed[..50];
        // Try to cut at a dash boundary
        if let Some(last_dash) = truncated.rfind('-') {
            truncated[..last_dash].to_string()
        } else {
            truncated.to_string()
        }
    }
}

/// Generate the filename for a task: `{number:03d}-{slug}.md`
pub fn task_filename(number: Option<i32>, title: &str) -> String {
    let slug = slugify(title);
    match number {
        Some(n) => format!("{:03}-{}.md", n, slug),
        None => format!("000-{}.md", slug),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frontmatter() -> TaskFrontmatter {
        TaskFrontmatter {
            id: Some("abc-123".to_string()),
            number: Some(1),
            title: "Fix login bug".to_string(),
            task_type: TaskType::Bug,
            priority: TaskPriority::Urgent,
            status: TaskStatus::Todo,
            tags: vec!["auth".to_string()],
            depends_on: vec![],
            assignee: None,
            issue: None,
            due: None,
            requires: vec![],
            created: "2026-03-25T10:00:00Z".to_string(),
            updated: "2026-03-25T10:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_parse_task_file() {
        let content = r#"---
id: abc-123
number: 1
title: Fix login bug
type: bug
priority: urgent
status: todo
tags:
  - auth
created: "2026-03-25T10:00:00Z"
updated: "2026-03-25T10:00:00Z"
---

# Fix login bug

Bug description here.
"#;
        let (fm, body) = parse_task_file(content).unwrap();
        assert_eq!(fm.id, Some("abc-123".to_string()));
        assert_eq!(fm.number, Some(1));
        assert_eq!(fm.title, "Fix login bug");
        assert!(matches!(fm.task_type, TaskType::Bug));
        assert!(matches!(fm.priority, TaskPriority::Urgent));
        assert!(matches!(fm.status, TaskStatus::Todo));
        assert_eq!(fm.tags, vec!["auth"]);
        assert!(body.contains("Bug description here."));
    }

    #[test]
    fn test_render_task_file() {
        let fm = sample_frontmatter();
        let body = "# Fix login bug\n\nBug description here.";
        let rendered = render_task_file(&fm, body).unwrap();
        assert!(rendered.starts_with("---\n"));
        assert!(rendered.contains("title: Fix login bug"));
        assert!(rendered.contains("type: bug"));
        assert!(rendered.contains("Bug description here."));
    }

    #[test]
    fn test_roundtrip() {
        let fm = sample_frontmatter();
        let body = "# Fix login bug\n\nBug description here.\n";
        let rendered = render_task_file(&fm, body).unwrap();
        let (fm2, body2) = parse_task_file(&rendered).unwrap();
        assert_eq!(fm.id, fm2.id);
        assert_eq!(fm.number, fm2.number);
        assert_eq!(fm.title, fm2.title);
        assert_eq!(fm.tags, fm2.tags);
        assert!(body2.contains("Bug description here."));
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "Just some text without frontmatter";
        assert!(parse_task_file(content).is_err());
    }

    #[test]
    fn test_parse_no_closing() {
        let content = "---\ntitle: Test\n";
        assert!(parse_task_file(content).is_err());
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Fix Login Bug"), "fix-login-bug");
        assert_eq!(
            slugify("Add search API endpoint"),
            "add-search-api-endpoint"
        );
        assert_eq!(slugify("hello   world"), "hello-world");
        assert_eq!(slugify("Special!@#Characters"), "special-characters");
        assert_eq!(slugify("  leading spaces  "), "leading-spaces");
        // CJK-only titles should fall back to "task"
        assert_eq!(slugify("实现登录功能"), "task");
        assert_eq!(slugify("データ分析"), "task");
        // Mixed CJK + ASCII keeps the ASCII part
        assert_eq!(slugify("实现 login 功能"), "login");
    }

    #[test]
    fn test_slugify_truncation() {
        let long_title = "this is a very very very very very very very very long title that exceeds fifty characters";
        let slug = slugify(long_title);
        assert!(slug.len() <= 50);
    }

    #[test]
    fn test_task_filename() {
        assert_eq!(
            task_filename(Some(1), "Fix login bug"),
            "001-fix-login-bug.md"
        );
        assert_eq!(
            task_filename(Some(12), "Add search API"),
            "012-add-search-api.md"
        );
        assert_eq!(task_filename(None, "New task"), "000-new-task.md");
    }

    #[test]
    fn test_empty_body() {
        let fm = sample_frontmatter();
        let rendered = render_task_file(&fm, "").unwrap();
        let (fm2, body2) = parse_task_file(&rendered).unwrap();
        assert_eq!(fm.title, fm2.title);
        assert!(body2.is_empty());
    }
}
