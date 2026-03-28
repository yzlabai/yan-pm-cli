use anyhow::Result;
use colored::Colorize;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};

use crate::agent::registry::{is_command_available, list_backends_by_priority};
use crate::daemon::event_store::EventStore;

pub async fn run(running_only: bool, json: bool) -> Result<()> {
    let backends = list_backends_by_priority().await;

    // Open event store for running task info
    let event_store = {
        let db_path = crate::config::config_dir().join("events.db");
        if db_path.exists() {
            EventStore::open(&db_path).ok()
        } else {
            None
        }
    };

    let active_tasks = event_store
        .as_ref()
        .and_then(|s| s.query_active_tasks().ok())
        .unwrap_or_default();

    if json {
        let mut entries = Vec::new();
        for backend in &backends {
            let available = is_command_available(backend.command()).await;
            let caps = backend.capabilities();
            entries.push(serde_json::json!({
                "name": backend.name(),
                "available": available,
                "command": backend.command(),
                "acp_args": backend.acp_args(),
                "capabilities": {
                    "supports_images": caps.supports_images,
                    "supports_mcp": caps.supports_mcp,
                    "supports_worktree": caps.supports_worktree,
                    "max_context_tokens": caps.max_context_tokens,
                },
                "description": backend.description(),
                "priority": backend.priority(),
            }));
        }
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    // Agents table
    if !running_only {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                "Agent", "状态", "Context", "MCP", "IMG", "Worktree", "命令",
            ]);

        for backend in &backends {
            let available = is_command_available(backend.command()).await;
            let status = if available {
                Cell::new("✓ 可用").fg(Color::Green)
            } else {
                Cell::new("✗ 未安装").fg(Color::Red)
            };
            let caps = backend.capabilities();
            let context = format!("{}K", caps.max_context_tokens / 1000);
            let check = |v: bool| {
                if v {
                    Cell::new("✓").fg(Color::Green)
                } else {
                    Cell::new("✗").fg(Color::DarkGrey)
                }
            };
            let cmd = format!("{} {}", backend.command(), backend.acp_args().join(" "));

            table.add_row(vec![
                Cell::new(backend.name()),
                status,
                Cell::new(&context),
                check(caps.supports_mcp),
                check(caps.supports_images),
                check(caps.supports_worktree),
                Cell::new(&cmd),
            ]);
        }

        println!("{table}");
    }

    // Running agents section
    if !active_tasks.is_empty() {
        println!();
        println!("{}", "正在执行:".bold());
        for event in &active_tasks {
            let agent = crate::cli::dashboard::parse_payload_field(&event.payload, "agent")
                .unwrap_or_else(|| "-".into());
            let title = crate::cli::dashboard::parse_payload_field(&event.payload, "title")
                .unwrap_or_default();
            let elapsed = event
                .created_at
                .parse::<chrono::DateTime<chrono::Utc>>()
                .ok()
                .map(|start| {
                    let secs = (chrono::Utc::now() - start).num_seconds();
                    if secs < 60 {
                        format!("{}s", secs)
                    } else {
                        format!("{}m {}s", secs / 60, secs % 60)
                    }
                })
                .unwrap_or_default();
            println!(
                "  {} → #{} {} · {}",
                agent.green(),
                &event.task_id[..8.min(event.task_id.len())],
                title,
                elapsed.dimmed()
            );
        }
    } else if running_only {
        println!("{}", "没有正在执行的 agent".yellow());
    }

    Ok(())
}
