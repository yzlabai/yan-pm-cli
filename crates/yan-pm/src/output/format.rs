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

pub fn print_dashboard(data: &crate::cli::dashboard::DashboardData) {
    let daemon_icon = if data.daemon_running { "✓" } else { "✗" };
    println!("╭─────────────────────────────────────────────────────────────────╮");
    println!("│{:^65}│", format!("yan-pm Dashboard"));
    println!(
        "│{:^65}│",
        format!(
            "{} workspaces · daemon {}",
            data.summary.total_workspaces, daemon_icon
        )
    );
    println!("╰─────────────────────────────────────────────────────────────────╯");
    println!();

    if data.workspaces.is_empty() {
        println!(
            "{}",
            "没有已关联的工作区。使用 `yan-pm link <project>` 关联。".yellow()
        );
        return;
    }

    for (i, ws) in data.workspaces.iter().enumerate() {
        let project_display = ws.project_name.as_deref().unwrap_or(&ws.project_id);
        let auto_run = if ws.auto_run_enabled {
            let agent = ws.auto_run_agent.as_deref().unwrap_or("claude");
            let budget = ws
                .auto_run_budget
                .map(|b| format!(", budget: ${:.0}", b))
                .unwrap_or_default();
            format!("ON ({agent}{budget})").green().to_string()
        } else {
            "OFF".dimmed().to_string()
        };

        println!(
            " {} {} — {}",
            "①②③④⑤⑥⑦⑧⑨⑩"
                .to_string()
                .chars()
                .nth(i)
                .map(|c| c.to_string())
                .unwrap_or_else(|| format!("({})", i + 1))
                .bold(),
            ws.name.bold(),
            ws.path.dimmed()
        );
        println!("   项目: {project_display} · auto-run: {auto_run}");

        if !ws.active_tasks.is_empty() || !ws.recent_completed.is_empty() {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_header(vec!["Agent", "任务", "状态", "耗时", "花费"]);

            for task in &ws.active_tasks {
                let agent = task.agent.as_deref().unwrap_or("-");
                let title = task
                    .title
                    .as_deref()
                    .map(|t| {
                        format!(
                            "#{} {}",
                            truncate_utf8(&task.task_id, 8),
                            truncate_utf8(t, 20)
                        )
                    })
                    .unwrap_or_else(|| format!("#{}", truncate_utf8(&task.task_id, 8)));
                let elapsed = task
                    .started_at
                    .as_ref()
                    .map(|s| format_elapsed(s))
                    .unwrap_or_default();
                let cost = task
                    .cost_usd
                    .map(|c| format!("${:.2}", c))
                    .unwrap_or_else(|| "-".into());
                table.add_row(vec![
                    Cell::new(agent),
                    Cell::new(&title),
                    Cell::new("● 执行中").fg(Color::Yellow),
                    Cell::new(&elapsed),
                    Cell::new(&cost),
                ]);
            }

            for task in &ws.recent_completed {
                let agent = task.agent.as_deref().unwrap_or("-");
                let title = task
                    .title
                    .as_deref()
                    .map(|t| {
                        format!(
                            "#{} {}",
                            truncate_utf8(&task.task_id, 8),
                            truncate_utf8(t, 20)
                        )
                    })
                    .unwrap_or_else(|| format!("#{}", truncate_utf8(&task.task_id, 8)));
                let elapsed = task
                    .started_at
                    .as_ref()
                    .map(|s| format_elapsed(s))
                    .unwrap_or_default();
                let cost = task
                    .cost_usd
                    .map(|c| format!("${:.2}", c))
                    .unwrap_or_else(|| "-".into());
                let (status_text, status_color) = if task.status == "completed" {
                    ("✓ 完成", Color::Green)
                } else {
                    ("✗ 失败", Color::Red)
                };
                table.add_row(vec![
                    Cell::new(agent),
                    Cell::new(&title),
                    Cell::new(status_text).fg(status_color),
                    Cell::new(&elapsed),
                    Cell::new(&cost),
                ]);
            }

            println!("   {table}");
        } else {
            println!("   {}", "无正在执行的任务".dimmed());
        }
        println!();
    }

    // Summary line
    println!(
        "  汇总: {} running · {} completed · ${:.2} total",
        data.summary.running_tasks, data.summary.completed_tasks, data.summary.total_cost
    );
}

pub fn print_dashboard_compact(data: &crate::cli::dashboard::DashboardData) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            "Workspace",
            "Project",
            "Daemon",
            "Auto",
            "Running",
            "Done",
            "Cost",
        ]);

    let daemon_str = if data.daemon_running { "✓" } else { "✗" };

    for ws in &data.workspaces {
        let project = ws.project_name.as_deref().unwrap_or(&ws.project_id);
        let auto = if ws.auto_run_enabled { "ON" } else { "OFF" };
        let running = ws.active_tasks.len();
        let done = ws.recent_completed.len();
        let cost: f64 = ws
            .active_tasks
            .iter()
            .chain(ws.recent_completed.iter())
            .filter_map(|t| t.cost_usd)
            .sum();

        table.add_row(vec![
            Cell::new(&ws.name),
            Cell::new(truncate_utf8(project, 20)),
            Cell::new(daemon_str).fg(if data.daemon_running {
                Color::Green
            } else {
                Color::DarkGrey
            }),
            Cell::new(auto).fg(if ws.auto_run_enabled {
                Color::Green
            } else {
                Color::DarkGrey
            }),
            Cell::new(running).fg(if running > 0 {
                Color::Yellow
            } else {
                Color::DarkGrey
            }),
            Cell::new(done),
            Cell::new(format!("${:.2}", cost)),
        ]);
    }

    println!("{table}");
}

/// Format elapsed time from an RFC3339 timestamp to now.
fn format_elapsed(rfc3339: &str) -> String {
    use chrono::{DateTime, Utc};
    let Ok(start) = DateTime::parse_from_rfc3339(rfc3339) else {
        return "-".into();
    };
    let elapsed = Utc::now() - start.with_timezone(&Utc);
    let secs = elapsed.num_seconds();
    if secs < 0 {
        return "0s".into();
    }
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn issue_status_color(s: IssueStatus) -> Color {
    match s {
        IssueStatus::Open => Color::Cyan,
        IssueStatus::Accepted => Color::Yellow,
        IssueStatus::Delivered => Color::Green,
        IssueStatus::Closed => Color::Blue,
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
