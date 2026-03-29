use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use super::app::App;

/// Main render entry point — pure function, no side effects.
pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),  // header
        Constraint::Min(5),    // workspace list
        Constraint::Length(1), // footer
    ])
    .split(f.area());

    render_header(f, chunks[0], app);
    render_workspace_list(f, chunks[1], app);
    render_footer(f, chunks[2]);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let daemon_status = if app.data.daemon_running {
        Span::styled("● online", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ offline", Style::default().fg(Color::Red))
    };

    let pid_span = app
        .data
        .daemon_pid
        .map(|p| Span::raw(format!(" (pid {})", p)))
        .unwrap_or_default();

    let summary = &app.data.summary;
    let info = Span::raw(format!(
        " · {} workspaces · {} active · {} completed · ${:.2}",
        summary.total_workspaces, summary.running_tasks, summary.completed_tasks, summary.total_cost,
    ));

    let header = Paragraph::new(Line::from(vec![
        Span::raw("Daemon: "),
        daemon_status,
        pid_span,
        info,
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" yan-pm Dashboard "),
    );

    f.render_widget(header, area);
}

fn render_workspace_list(f: &mut Frame, area: Rect, app: &App) {
    if app.data.workspaces.is_empty() {
        let msg = Paragraph::new("  (no linked workspaces)")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(msg, area);
        return;
    }

    // Calculate how much space each workspace needs
    let mut constraints: Vec<Constraint> = Vec::new();
    for (i, ws) in app.data.workspaces.iter().enumerate() {
        let is_expanded = app.expanded.contains(&i);
        let task_rows = if is_expanded {
            ws.active_tasks.len() + ws.recent_completed.len()
        } else {
            ws.active_tasks.len()
        };
        // 2 for header line + border, + task rows (min 1 for idle message)
        let height = 3 + task_rows.max(1) as u16;
        constraints.push(Constraint::Length(height));
    }
    constraints.push(Constraint::Min(0)); // spacer

    let ws_chunks = Layout::vertical(constraints).split(area);

    for (i, ws) in app.data.workspaces.iter().enumerate() {
        let is_selected = i == app.selected;
        let is_expanded = app.expanded.contains(&i);

        let border_style = if is_selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let auto_str = if ws.auto_run_enabled {
            format!(
                "auto:ON  {}",
                ws.auto_run_agent.as_deref().unwrap_or("?")
            )
        } else {
            "auto:OFF".to_string()
        };

        let title = format!(
            " [{}] {}  {}  {} ",
            i + 1,
            ws.name,
            ws.path,
            auto_str,
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title);

        // Build task rows
        let mut rows: Vec<Row> = Vec::new();

        for task in &ws.active_tasks {
            let id_short = &task.task_id[..8.min(task.task_id.len())];
            let title_str = task.title.as_deref().unwrap_or("(untitled)");
            let cost_str = task
                .cost_usd
                .map(|c| format!("${:.2}", c))
                .unwrap_or_default();
            rows.push(
                Row::new(vec![
                    format!("● #{}", id_short),
                    title_str.to_string(),
                    "running".to_string(),
                    cost_str,
                ])
                .style(Style::default().fg(Color::Yellow)),
            );
        }

        if is_expanded {
            for task in &ws.recent_completed {
                let id_short = &task.task_id[..8.min(task.task_id.len())];
                let title_str = task.title.as_deref().unwrap_or("(untitled)");
                let cost_str = task
                    .cost_usd
                    .map(|c| format!("${:.2}", c))
                    .unwrap_or_default();
                let (symbol, color) = if task.status == "completed" {
                    ("✓", Color::Green)
                } else {
                    ("✗", Color::Red)
                };
                rows.push(
                    Row::new(vec![
                        format!("{} #{}", symbol, id_short),
                        title_str.to_string(),
                        task.status.clone(),
                        cost_str,
                    ])
                    .style(Style::default().fg(color)),
                );
            }
        }

        if rows.is_empty() {
            rows.push(
                Row::new(vec!["  (idle)".to_string(), String::new(), String::new(), String::new()])
                    .style(Style::default().fg(Color::DarkGray)),
            );
        }

        let widths = [
            Constraint::Length(14),
            Constraint::Min(20),
            Constraint::Length(10),
            Constraint::Length(8),
        ];

        let table = Table::new(rows, widths).block(block);
        f.render_widget(table, ws_chunks[i]);
    }
}

fn render_footer(f: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":退出  "),
        Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":刷新  "),
        Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":选择  "),
        Span::styled("enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":展开/折叠"),
    ]))
    .style(Style::default().fg(Color::DarkGray));

    f.render_widget(footer, area);
}
