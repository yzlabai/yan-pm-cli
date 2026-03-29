use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use super::app::{App, ViewMode};
use crate::cli::dashboard::parse_payload_field;

/// Main render entry point — pure function, no side effects.
pub fn render(f: &mut Frame, app: &App) {
    match &app.mode {
        ViewMode::Dashboard => render_dashboard(f, app),
        ViewMode::LogView => render_log(f, app),
    }
}

// ── Dashboard view ──────────────────────────────────────────────────

fn render_dashboard(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),  // header
        Constraint::Min(5),    // workspace list
        Constraint::Length(1), // footer
    ])
    .split(f.area());

    render_header(f, chunks[0], app);
    render_workspace_list(f, chunks[1], app);
    render_dashboard_footer(f, chunks[2]);
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

    let mut constraints: Vec<Constraint> = Vec::new();
    for (i, ws) in app.data.workspaces.iter().enumerate() {
        let is_expanded = app.expanded.contains(&i);
        let task_rows = if is_expanded {
            ws.active_tasks.len() + ws.recent_completed.len()
        } else {
            ws.active_tasks.len()
        };
        let height = 3 + task_rows.max(1) as u16;
        constraints.push(Constraint::Length(height));
    }
    constraints.push(Constraint::Min(0));

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
            format!("auto:ON  {}", ws.auto_run_agent.as_deref().unwrap_or("?"))
        } else {
            "auto:OFF".to_string()
        };

        let title = format!(" [{}] {}  {}  {} ", i + 1, ws.name, ws.path, auto_str);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title);

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
                Row::new(vec![
                    "  (idle)".to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                ])
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

fn render_dashboard_footer(f: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":退出  "),
        Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":刷新  "),
        Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":选择  "),
        Span::styled("enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":日志/展开"),
    ]))
    .style(Style::default().fg(Color::DarkGray));

    f.render_widget(footer, area);
}

// ── Log view ────────────────────────────────────────────────────────

fn render_log(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),  // header
        Constraint::Min(5),    // log content
        Constraint::Length(1), // footer / search bar
    ])
    .split(f.area());

    render_log_header(f, chunks[0], app);
    render_log_panel(f, chunks[1], app);
    render_log_footer(f, chunks[2], app);
}

fn render_log_header(f: &mut Frame, area: Rect, app: &App) {
    let Some(log) = &app.log_view else { return };

    let id_short = &log.task_id[..8.min(log.task_id.len())];
    let total = log.events.len();
    let visible = app.visible_log_events().len();

    let mut spans = vec![
        Span::raw(format!("Task: {} [{}]", log.title, id_short)),
        Span::raw(format!(" · {} events", total)),
    ];

    if let Some(filter) = &log.filter {
        spans.push(Span::styled(
            format!(" · filter:{}", filter),
            Style::default().fg(Color::Magenta),
        ));
        spans.push(Span::raw(format!(" ({})", visible)));
    }

    if let Some(query) = &log.search {
        spans.push(Span::styled(
            format!(" · search:\"{}\"", query),
            Style::default().fg(Color::Yellow),
        ));
    }

    if !log.auto_scroll {
        spans.push(Span::styled(
            " · PAUSED",
            Style::default().fg(Color::Red),
        ));
    }

    let header = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Agent Log "),
    );

    f.render_widget(header, area);
}

fn render_log_panel(f: &mut Frame, area: Rect, app: &App) {
    let Some(log) = &app.log_view else { return };

    let visible = app.visible_log_events();
    let content_height = area.height.saturating_sub(2) as usize; // minus borders

    // Calculate visible window with scroll
    let total = visible.len();
    let scroll = if log.auto_scroll {
        total.saturating_sub(content_height)
    } else {
        total
            .saturating_sub(content_height)
            .saturating_sub(log.scroll_offset as usize)
    };

    let window_end = (scroll + content_height).min(total);
    let window = &visible[scroll..window_end];

    let lines: Vec<Line> = window
        .iter()
        .map(|event| format_event_line(event))
        .collect();

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    f.render_widget(paragraph, area);
}

fn format_event_line(event: &crate::daemon::event_store::Event) -> Line<'static> {
    // Extract timestamp (just time portion if possible)
    let time = if event.created_at.len() >= 19 {
        &event.created_at[11..19]
    } else {
        &event.created_at
    };

    let (icon, color) = match event.event_type.as_str() {
        "tool_call" => ("🔧", Color::Cyan),
        "tool_result" => ("📋", Color::Blue),
        "agent_output" => ("💬", Color::White),
        "task_started" => ("▶", Color::Green),
        "task_completed" => ("✓", Color::Green),
        "task_failed" => ("✗", Color::Red),
        "state_change" => ("⚡", Color::Yellow),
        "error" => ("⚠", Color::Red),
        _ => ("·", Color::DarkGray),
    };

    // Extract a human-readable summary from payload
    let summary = match event.event_type.as_str() {
        "tool_call" => parse_payload_field(&event.payload, "tool")
            .unwrap_or_else(|| "(unknown tool)".to_string()),
        "agent_output" => {
            let text = parse_payload_field(&event.payload, "text")
                .unwrap_or_default();
            // Truncate to single line, max 120 chars
            let line = text.lines().next().unwrap_or("");
            if line.len() > 120 {
                format!("{}…", &line[..119])
            } else {
                line.to_string()
            }
        }
        "state_change" => {
            let from = parse_payload_field(&event.payload, "from").unwrap_or_default();
            let to = parse_payload_field(&event.payload, "to").unwrap_or_default();
            format!("{} → {}", from, to)
        }
        "task_started" => parse_payload_field(&event.payload, "title")
            .unwrap_or_else(|| "started".to_string()),
        "task_completed" | "task_failed" => parse_payload_field(&event.payload, "title")
            .unwrap_or_else(|| event.event_type.clone()),
        _ => {
            // Show truncated raw payload
            let p = &event.payload;
            if p.len() > 80 {
                format!("{}…", &p[..79])
            } else {
                p.to_string()
            }
        }
    };

    Line::from(vec![
        Span::styled(
            format!("{} ", time),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(format!("{} ", icon)),
        Span::styled(
            format!("{:14}", event.event_type),
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}", summary)),
    ])
}

fn render_log_footer(f: &mut Frame, area: Rect, app: &App) {
    let Some(log) = &app.log_view else { return };

    // If in search input mode, show search bar
    if let Some(input) = &log.search_input {
        let search_bar = Paragraph::new(Line::from(vec![
            Span::styled("/", Style::default().fg(Color::Yellow)),
            Span::raw(input.as_str()),
            Span::styled("█", Style::default().fg(Color::White)),
        ]));
        f.render_widget(search_bar, area);
        return;
    }

    let mut spans = vec![
        Span::styled("esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":返回  "),
        Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":滚动  "),
        Span::styled("G", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":底部  "),
        Span::styled("/", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":搜索  "),
        Span::styled("f", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":过滤  "),
        Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":导出"),
    ];

    // Show active filter
    if let Some(filter) = &log.filter {
        spans.push(Span::styled(
            format!("  [{}]", filter),
            Style::default().fg(Color::Magenta),
        ));
    }

    let footer = Paragraph::new(Line::from(spans))
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(footer, area);
}
