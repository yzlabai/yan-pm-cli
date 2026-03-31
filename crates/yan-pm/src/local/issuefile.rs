use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::api::types::{IssueStatus, IssueType, TaskPriority};

/// YAML frontmatter for a local issue file (.yan-pm/issues/*.md)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueFrontmatter {
    pub id: String,
    pub number: i32,
    pub title: String,
    #[serde(rename = "type")]
    pub issue_type: IssueType,
    pub priority: TaskPriority,
    pub status: IssueStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_criteria: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    pub created: String,
    pub updated: String,
}

/// A local issue file = frontmatter + markdown body.
#[derive(Debug, Clone)]
pub struct LocalIssueFile {
    pub frontmatter: IssueFrontmatter,
    pub body: String,
    pub file_path: std::path::PathBuf,
}

/// Parse an issue file content into frontmatter + body.
pub fn parse_issue_file(content: &str) -> Result<(IssueFrontmatter, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        anyhow::bail!("Issue file missing YAML frontmatter (must start with ---)");
    }

    let after_first = &trimmed[3..];
    let close_idx = after_first
        .find("\n---")
        .context("Issue file missing closing --- for frontmatter")?;

    let yaml_str = &after_first[..close_idx].trim();
    let body_start = 3 + close_idx + 4;
    let body = if body_start < trimmed.len() {
        trimmed[body_start..]
            .trim_start_matches(&['\r', '\n'][..])
            .to_string()
    } else {
        String::new()
    };

    let frontmatter: IssueFrontmatter =
        serde_yaml::from_str(yaml_str).context("Failed to parse issue YAML frontmatter")?;

    Ok((frontmatter, body))
}

/// Render an issue file from frontmatter + body back to Markdown string.
pub fn render_issue_file(frontmatter: &IssueFrontmatter, body: &str) -> Result<String> {
    let yaml =
        serde_yaml::to_string(frontmatter).context("Failed to serialize issue frontmatter")?;
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

/// Generate the filename for an issue: `{number:03d}-{slug}.md`
pub fn issue_filename(number: i32, title: &str) -> String {
    let slug = super::taskfile::slugify(title);
    format!("{:03}-{}.md", number, slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frontmatter() -> IssueFrontmatter {
        IssueFrontmatter {
            id: "issue-abc".to_string(),
            number: 1,
            title: "Add OAuth login".to_string(),
            issue_type: IssueType::Feature,
            priority: TaskPriority::High,
            status: IssueStatus::Open,
            labels: vec!["auth".to_string()],
            acceptance_criteria: vec!["Users can log in with Google".to_string()],
            assignee: None,
            created: "2026-01-01T00:00:00Z".to_string(),
            updated: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_parse_issue_file() {
        let content = r#"---
id: issue-abc
number: 1
title: Add OAuth login
type: feature
priority: high
status: open
labels:
  - auth
acceptance_criteria:
  - Users can log in with Google
created: "2026-01-01T00:00:00Z"
updated: "2026-01-01T00:00:00Z"
---

OAuth login description here.
"#;
        let (fm, body) = parse_issue_file(content).unwrap();
        assert_eq!(fm.id, "issue-abc");
        assert_eq!(fm.number, 1);
        assert_eq!(fm.title, "Add OAuth login");
        assert!(body.contains("OAuth login description here."));
    }

    #[test]
    fn test_render_issue_file() {
        let fm = sample_frontmatter();
        let body = "OAuth login description here.";
        let rendered = render_issue_file(&fm, body).unwrap();
        assert!(rendered.starts_with("---\n"));
        assert!(rendered.contains("title: Add OAuth login"));
        assert!(rendered.contains("type: feature"));
        assert!(rendered.contains("OAuth login description here."));
    }

    #[test]
    fn test_roundtrip() {
        let fm = sample_frontmatter();
        let body = "Description here.\n";
        let rendered = render_issue_file(&fm, body).unwrap();
        let (fm2, body2) = parse_issue_file(&rendered).unwrap();
        assert_eq!(fm.id, fm2.id);
        assert_eq!(fm.number, fm2.number);
        assert_eq!(fm.title, fm2.title);
        assert!(body2.contains("Description here."));
    }

    #[test]
    fn test_issue_filename() {
        assert_eq!(
            issue_filename(1, "Add OAuth login"),
            "001-add-oauth-login.md"
        );
        assert_eq!(issue_filename(12, "Fix bug"), "012-fix-bug.md");
    }
}
