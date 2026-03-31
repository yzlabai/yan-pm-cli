use anyhow::Result;

/// A task parsed from the "## 任务拆分" section of a spec.
#[derive(Debug, Clone)]
pub struct ParsedTask {
    pub title: String,
    /// Whether this task is marked as parallel-safe with [P]
    #[allow(dead_code)]
    pub parallel: bool,
    /// Dependency markers like [D:001-01]
    pub depends_on: Vec<String>,
    /// Any indented continuation lines after the checkbox line
    pub description: String,
    /// Whether the checkbox was already checked
    pub checked: bool,
}

/// Parse tasks from the "## 任务拆分" section of a spec body.
///
/// Supports two formats:
/// ```text
/// - [ ] [P] 001-01 配置 OAuth Provider
/// - [ ] [D:001-01] 001-02 实现 Callback 端点
/// ```
/// Or simpler:
/// ```text
/// - [ ] 配置 OAuth Provider
/// - [ ] 实现 Callback 端点
/// ```
pub fn parse_tasks_from_spec(spec_body: &str) -> Result<Vec<ParsedTask>> {
    // Find the "## 任务拆分" section
    let section_start = find_section_start(spec_body, "任务拆分");
    let section_body = match section_start {
        Some(start) => extract_section(spec_body, start),
        None => {
            // Also try "## Tasks" as English alternative
            match find_section_start(spec_body, "Tasks") {
                Some(start) => extract_section(spec_body, start),
                None => anyhow::bail!("Spec 中未找到 \"## 任务拆分\" 部分"),
            }
        }
    };

    let mut tasks = Vec::new();
    let lines: Vec<&str> = section_body.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if let Some(task) = parse_checkbox_line(line) {
            // Collect indented continuation lines as description
            let mut desc_lines = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let next = lines[j];
                // Continuation: indented (2+ spaces or tab) and NOT a new checkbox
                if (next.starts_with("  ") || next.starts_with('\t'))
                    && !is_checkbox_line(next.trim_start())
                {
                    desc_lines.push(next.trim());
                    j += 1;
                } else {
                    break;
                }
            }
            let mut parsed = task;
            if !desc_lines.is_empty() {
                parsed.description = desc_lines.join("\n");
            }
            tasks.push(parsed);
            i = j;
        } else {
            i += 1;
        }
    }

    Ok(tasks)
}

/// Find the byte offset where a ## section starts
fn find_section_start(body: &str, section_name: &str) -> Option<usize> {
    // Match "## 任务拆分" or "## Tasks" etc
    body.find(&format!("## {}", section_name))
}

/// Extract content of a section (from ## header to next ## or end)
fn extract_section(body: &str, start: usize) -> String {
    let after_header = &body[start..];
    // Skip the header line
    let content_start = after_header.find('\n').map(|p| p + 1).unwrap_or(0);
    let content = &after_header[content_start..];

    // Find the next ## heading (end of section)
    if let Some(next_heading) = content.find("\n## ") {
        content[..next_heading].to_string()
    } else {
        content.to_string()
    }
}

/// Check if a line is a checkbox line
fn is_checkbox_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches("- ");
    trimmed.starts_with("[ ]") || trimmed.starts_with("[x]") || trimmed.starts_with("[X]")
}

/// Parse a single checkbox line into a ParsedTask
fn parse_checkbox_line(line: &str) -> Option<ParsedTask> {
    let trimmed = line.trim();
    if !trimmed.starts_with("- [") {
        return None;
    }

    // Extract checkbox state
    let after_dash = trimmed.strip_prefix("- ")?;
    let (checked, rest) = if let Some(r) = after_dash.strip_prefix("[ ] ") {
        (false, r)
    } else if let Some(r) = after_dash
        .strip_prefix("[x] ")
        .or_else(|| after_dash.strip_prefix("[X] "))
    {
        (true, r)
    } else if let Some(r) = after_dash.strip_prefix("[ ]") {
        (false, r)
    } else if let Some(r) = after_dash
        .strip_prefix("[x]")
        .or_else(|| after_dash.strip_prefix("[X]"))
    {
        (true, r)
    } else {
        return None;
    };

    let rest = rest.trim();
    if rest.is_empty() {
        return None; // Skip empty checkbox items (template placeholders)
    }

    let mut parallel = false;
    let mut depends_on = Vec::new();
    let mut remaining = rest.to_string();

    // Parse markers: [P], [D:xxx]
    loop {
        let trimmed_m = remaining.trim_start();
        if let Some(after_p) = trimmed_m.strip_prefix("[P]") {
            parallel = true;
            remaining = after_p.to_string();
        } else if trimmed_m.starts_with("[D:") {
            let trimmed = trimmed_m;
            if let Some(close) = trimmed.find(']') {
                let dep = trimmed[3..close].to_string();
                depends_on.push(dep);
                remaining = trimmed[close + 1..].to_string();
            } else {
                break;
            }
        } else {
            break;
        }
    }

    let remaining = remaining.trim();

    // Try to strip a number prefix like "001-01 " or "001-02 "
    let title = strip_number_prefix(remaining);

    if title.is_empty() {
        return None;
    }

    Some(ParsedTask {
        title: title.to_string(),
        parallel,
        depends_on,
        description: String::new(),
        checked,
    })
}

/// Strip optional number prefix (e.g. "001-01 " or "001 ")
fn strip_number_prefix(s: &str) -> &str {
    // Match patterns like "001-01 ", "001-02 ", "001 "
    let bytes = s.as_bytes();
    let mut i = 0;

    // Must start with digits
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return s; // No digits found, return as-is
    }

    // Optional: dash + more digits
    if i < bytes.len() && bytes[i] == b'-' {
        let j = i + 1;
        let mut k = j;
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k > j {
            i = k;
        }
    }

    // Must be followed by a space
    if i < bytes.len() && bytes[i] == b' ' {
        &s[i + 1..]
    } else {
        s // No space after number, treat the whole thing as title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_tasks() {
        let spec = r#"## 背景

Some background.

## 任务拆分

- [ ] 配置 OAuth Provider
- [ ] 实现 Callback 端点
- [ ] 添加前端登录按钮
"#;
        let tasks = parse_tasks_from_spec(spec).unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].title, "配置 OAuth Provider");
        assert_eq!(tasks[1].title, "实现 Callback 端点");
        assert_eq!(tasks[2].title, "添加前端登录按钮");
        assert!(!tasks[0].parallel);
        assert!(tasks[0].depends_on.is_empty());
    }

    #[test]
    fn test_parse_tasks_with_markers() {
        let spec = r#"## 任务拆分

- [ ] [P] 001-01 配置 OAuth Provider
- [ ] [D:001-01] 001-02 实现 Callback 端点
- [ ] [P] 001-03 添加前端登录按钮
"#;
        let tasks = parse_tasks_from_spec(spec).unwrap();
        assert_eq!(tasks.len(), 3);

        assert!(tasks[0].parallel);
        assert!(tasks[0].depends_on.is_empty());
        assert_eq!(tasks[0].title, "配置 OAuth Provider");

        assert!(!tasks[1].parallel);
        assert_eq!(tasks[1].depends_on, vec!["001-01"]);
        assert_eq!(tasks[1].title, "实现 Callback 端点");

        assert!(tasks[2].parallel);
        assert_eq!(tasks[2].title, "添加前端登录按钮");
    }

    #[test]
    fn test_parse_tasks_with_descriptions() {
        let spec = r#"## 任务拆分

- [ ] 配置 OAuth Provider
  设置 Google/GitHub OAuth 应用
  获取 client_id 和 secret
- [ ] 实现 Callback 端点
"#;
        let tasks = parse_tasks_from_spec(spec).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            tasks[0].description,
            "设置 Google/GitHub OAuth 应用\n获取 client_id 和 secret"
        );
        assert!(tasks[1].description.is_empty());
    }

    #[test]
    fn test_parse_checked_tasks() {
        let spec = r#"## 任务拆分

- [x] 已完成的任务
- [ ] 未完成的任务
"#;
        let tasks = parse_tasks_from_spec(spec).unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].checked);
        assert!(!tasks[1].checked);
    }

    #[test]
    fn test_parse_no_section() {
        let spec = "## 背景\n\nSome text.\n";
        let result = parse_tasks_from_spec(spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_checkboxes_skipped() {
        let spec = r#"## 任务拆分

- [ ]
- [ ] 有效的任务
"#;
        let tasks = parse_tasks_from_spec(spec).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "有效的任务");
    }

    #[test]
    fn test_parse_multiple_dependencies() {
        let spec = r#"## 任务拆分

- [ ] [D:001-01] [D:001-02] 001-03 需要两个依赖的任务
"#;
        let tasks = parse_tasks_from_spec(spec).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].depends_on, vec!["001-01", "001-02"]);
    }

    #[test]
    fn test_section_stops_at_next_heading() {
        let spec = r#"## 任务拆分

- [ ] Task A
- [ ] Task B

## 其他部分

- [ ] Not a task
"#;
        let tasks = parse_tasks_from_spec(spec).unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_strip_number_prefix() {
        assert_eq!(strip_number_prefix("001-01 配置"), "配置");
        assert_eq!(strip_number_prefix("001 配置"), "配置");
        assert_eq!(strip_number_prefix("配置"), "配置");
        assert_eq!(strip_number_prefix("12-34 Title"), "Title");
    }
}
