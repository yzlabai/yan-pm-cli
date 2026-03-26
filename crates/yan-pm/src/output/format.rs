use colored::Colorize;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};

use crate::api::types::*;
use crate::local::taskfile::LocalTaskFile;

/// Truncate a UTF-8 string to at most `max_bytes` bytes on a char boundary.
pub fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // floor_char_boundary finds the largest index <= max_bytes that is a char boundary
    &s[..s.floor_char_boundary(max_bytes)]
}

pub fn print_projects(projects: &[Project]) {
    if projects.is_empty() {
        println!("{}", "没有找到项目".yellow());
        return;
    }
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Slug", "名称", "状态", "角色"]);

    for p in projects {
        table.add_row(vec![
            Cell::new(&p.slug),
            Cell::new(&p.name),
            Cell::new(match p.status {
                ProjectStatus::Planning => "规划中",
                ProjectStatus::Active => "活跃",
                ProjectStatus::Completed => "已完成",
                ProjectStatus::Archived => "归档",
            })
            .fg(match p.status {
                ProjectStatus::Planning => Color::Yellow,
                ProjectStatus::Active => Color::Green,
                ProjectStatus::Completed => Color::Blue,
                ProjectStatus::Archived => Color::DarkGrey,
            }),
            Cell::new(p.my_role.as_deref().unwrap_or("-")),
        ]);
    }
    println!("{table}");
}

pub fn print_tasks(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("{}", "没有找到任务".yellow());
        return;
    }
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["ID", "标题", "类型", "优先级", "状态", "标签"]);

    for t in tasks {
        let short_id = &t.id[..8.min(t.id.len())];
        table.add_row(vec![
            Cell::new(short_id).fg(Color::DarkGrey),
            Cell::new(&t.title),
            Cell::new(format!("{}", t.task_type)),
            Cell::new(format!("{}", t.priority)).fg(priority_color(t.priority)),
            Cell::new(format!("{}", t.status)).fg(status_color(t.status)),
            Cell::new(t.tags.join(", ")),
        ]);
    }
    println!("{table}");
}

pub fn print_issues(issues: &[Issue]) {
    if issues.is_empty() {
        println!("{}", "没有找到需求".yellow());
        return;
    }
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["#", "标题", "类型", "优先级", "状态", "标签"]);

    for i in issues {
        table.add_row(vec![
            Cell::new(format!("#{}", i.number)),
            Cell::new(&i.title),
            Cell::new(format!("{}", i.issue_type)),
            Cell::new(format!("{}", i.priority)).fg(priority_color(i.priority)),
            Cell::new(format!("{}", i.status)).fg(issue_status_color(i.status)),
            Cell::new(i.labels.join(", ")),
        ]);
    }
    println!("{table}");
}

pub fn print_workspaces(workspaces: &[Workspace]) {
    if workspaces.is_empty() {
        println!("{}", "没有找到工作区".yellow());
        return;
    }
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["名称", "路径", "用户", "在线", "机器"]);

    for w in workspaces {
        let online = w.online.unwrap_or(false);
        table.add_row(vec![
            Cell::new(&w.name),
            Cell::new(&w.local_path),
            Cell::new(w.user_name.as_deref().unwrap_or("-")),
            Cell::new(if online { "✓" } else { "✗" }).fg(if online {
                Color::Green
            } else {
                Color::DarkGrey
            }),
            Cell::new(&w.machine_id),
        ]);
    }
    println!("{table}");
}

pub fn print_execution_status(status: &ExecutionStatus) {
    if status.tasks.is_empty() {
        println!("{}", "没有正在执行的任务".yellow());
        return;
    }
    let threshold = status.stale_threshold_ms.unwrap_or(300_000);
    let now = chrono::Utc::now();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["ID", "标题", "状态", "锁定者", "上次心跳", "是否过期"]);

    for t in &status.tasks {
        let short_id = &t.id[..8.min(t.id.len())];
        let is_stale = t
            .last_heartbeat
            .as_ref()
            .and_then(|h| chrono::DateTime::parse_from_rfc3339(h).ok())
            .map(|h| (now - h.with_timezone(&chrono::Utc)).num_milliseconds() as u64 > threshold)
            .unwrap_or(true);

        table.add_row(vec![
            Cell::new(short_id).fg(Color::DarkGrey),
            Cell::new(&t.title),
            Cell::new(format!("{}", t.status)),
            Cell::new(t.locked_by.as_deref().unwrap_or("-")),
            Cell::new(t.last_heartbeat.as_deref().unwrap_or("-")),
            Cell::new(if is_stale { "⚠ 过期" } else { "✓ 正常" }).fg(if is_stale {
                Color::Red
            } else {
                Color::Green
            }),
        ]);
    }
    println!("{table}");
}

pub fn print_local_tasks(tasks: &[LocalTaskFile]) {
    if tasks.is_empty() {
        println!("{}", "没有找到本地任务文件".yellow());
        return;
    }
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["#", "标题", "类型", "优先级", "状态", "标签"]);

    for t in tasks {
        let fm = &t.frontmatter;
        let num = fm
            .number
            .map(|n| format!("#{n}"))
            .unwrap_or_else(|| "new".to_string());
        table.add_row(vec![
            Cell::new(&num).fg(Color::DarkGrey),
            Cell::new(&fm.title),
            Cell::new(format!("{}", fm.task_type)),
            Cell::new(format!("{}", fm.priority)).fg(priority_color(fm.priority)),
            Cell::new(format!("{}", fm.status)).fg(status_color(fm.status)),
            Cell::new(fm.tags.join(", ")),
        ]);
    }
    println!("{table}");
}

fn priority_color(p: TaskPriority) -> Color {
    match p {
        TaskPriority::Urgent => Color::Red,
        TaskPriority::High => Color::Yellow,
        TaskPriority::Medium => Color::Blue,
        TaskPriority::Low => Color::DarkGrey,
    }
}

fn status_color(s: TaskStatus) -> Color {
    match s {
        TaskStatus::Todo => Color::Cyan,
        TaskStatus::InProgress => Color::Yellow,
        TaskStatus::InReview => Color::Magenta,
        TaskStatus::Done => Color::Green,
        TaskStatus::Cancelled => Color::DarkGrey,
    }
}

fn issue_status_color(s: IssueStatus) -> Color {
    match s {
        IssueStatus::Open => Color::Cyan,
        IssueStatus::Analyzing => Color::Yellow,
        IssueStatus::TasksCreated => Color::Green,
        IssueStatus::NeedsManual => Color::Red,
        IssueStatus::Cancelled => Color::DarkGrey,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_utf8_ascii() {
        assert_eq!(truncate_utf8("hello", 10), "hello");
        assert_eq!(truncate_utf8("hello", 5), "hello");
        assert_eq!(truncate_utf8("hello", 3), "hel");
    }

    #[test]
    fn test_truncate_utf8_cjk() {
        // '中' is 3 bytes in UTF-8
        let s = "中文测试";
        assert_eq!(truncate_utf8(s, 100), "中文测试");
        assert_eq!(truncate_utf8(s, 6), "中文");
        // 5 bytes: can't fit 2nd char fully, truncates to 1 char (3 bytes)
        assert_eq!(truncate_utf8(s, 5), "中");
        assert_eq!(truncate_utf8(s, 3), "中");
        assert_eq!(truncate_utf8(s, 2), "");
        assert_eq!(truncate_utf8(s, 0), "");
    }
}
