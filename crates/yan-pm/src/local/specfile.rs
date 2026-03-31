use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecStatus {
    Draft,
    Ready,
    InProgress,
    Done,
}

impl std::fmt::Display for SpecStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Ready => write!(f, "ready"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Done => write!(f, "done"),
        }
    }
}

/// YAML frontmatter for a local spec file (.yan-pm/specs/*.md)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecFrontmatter {
    pub issue: i32,
    pub title: String,
    pub status: SpecStatus,
    pub created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
}

/// A local spec file = frontmatter + markdown body.
#[derive(Debug, Clone)]
pub struct LocalSpecFile {
    pub frontmatter: SpecFrontmatter,
    pub body: String,
    pub file_path: std::path::PathBuf,
}

/// Parse a spec file content into frontmatter + body.
pub fn parse_spec_file(content: &str) -> Result<(SpecFrontmatter, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        anyhow::bail!("Spec file missing YAML frontmatter (must start with ---)");
    }

    let after_first = &trimmed[3..];
    let close_idx = after_first
        .find("\n---")
        .context("Spec file missing closing --- for frontmatter")?;

    let yaml_str = &after_first[..close_idx].trim();
    let body_start = 3 + close_idx + 4;
    let body = if body_start < trimmed.len() {
        trimmed[body_start..]
            .trim_start_matches(&['\r', '\n'][..])
            .to_string()
    } else {
        String::new()
    };

    let frontmatter: SpecFrontmatter =
        serde_yaml::from_str(yaml_str).context("Failed to parse spec YAML frontmatter")?;

    Ok((frontmatter, body))
}

/// Render a spec file from frontmatter + body back to Markdown string.
pub fn render_spec_file(frontmatter: &SpecFrontmatter, body: &str) -> Result<String> {
    let yaml =
        serde_yaml::to_string(frontmatter).context("Failed to serialize spec frontmatter")?;
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

/// Generate the filename for a spec: `{issue_number:03d}-{slug}.md`
pub fn spec_filename(issue_number: i32, title: &str) -> String {
    let slug = super::taskfile::slugify(title);
    format!("{:03}-{}.md", issue_number, slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frontmatter() -> SpecFrontmatter {
        SpecFrontmatter {
            issue: 1,
            title: "Add OAuth login".to_string(),
            status: SpecStatus::Draft,
            created: "2026-01-01T00:00:00Z".to_string(),
            updated: None,
        }
    }

    #[test]
    fn test_parse_spec_file() {
        let content = r#"---
issue: 1
title: Add OAuth login
status: draft
created: "2026-01-01T00:00:00Z"
---

## 背景

OAuth login spec.
"#;
        let (fm, body) = parse_spec_file(content).unwrap();
        assert_eq!(fm.issue, 1);
        assert_eq!(fm.title, "Add OAuth login");
        assert_eq!(fm.status, SpecStatus::Draft);
        assert!(body.contains("OAuth login spec."));
    }

    #[test]
    fn test_render_spec_file() {
        let fm = sample_frontmatter();
        let body = "## 背景\n\nOAuth login spec.";
        let rendered = render_spec_file(&fm, body).unwrap();
        assert!(rendered.starts_with("---\n"));
        assert!(rendered.contains("title: Add OAuth login"));
        assert!(rendered.contains("status: draft"));
        assert!(rendered.contains("OAuth login spec."));
    }

    #[test]
    fn test_roundtrip() {
        let fm = sample_frontmatter();
        let body = "Spec body.\n";
        let rendered = render_spec_file(&fm, body).unwrap();
        let (fm2, body2) = parse_spec_file(&rendered).unwrap();
        assert_eq!(fm.issue, fm2.issue);
        assert_eq!(fm.title, fm2.title);
        assert!(body2.contains("Spec body."));
    }

    #[test]
    fn test_spec_filename() {
        assert_eq!(
            spec_filename(1, "Add OAuth login"),
            "001-add-oauth-login.md"
        );
        assert_eq!(spec_filename(12, "Fix bug"), "012-fix-bug.md");
    }

    #[test]
    fn test_spec_status_display() {
        assert_eq!(format!("{}", SpecStatus::Draft), "draft");
        assert_eq!(format!("{}", SpecStatus::Ready), "ready");
        assert_eq!(format!("{}", SpecStatus::InProgress), "in_progress");
        assert_eq!(format!("{}", SpecStatus::Done), "done");
    }
}
