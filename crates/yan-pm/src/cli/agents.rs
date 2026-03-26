use anyhow::Result;
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

use crate::agent::registry::{is_command_available, load_agents};

pub async fn run() -> Result<()> {
    let agents = load_agents();
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(vec!["Agent", "状态", "命令"]);

    for agent in &agents {
        let available = is_command_available(&agent.command).await;
        let status = if available {
            "✓ 可用".green().to_string()
        } else {
            "✗ 未安装".red().to_string()
        };
        let cmd = format!(
            "{} {}",
            agent.command,
            agent.acp_args.join(" ")
        );
        table.add_row(vec![&agent.name, &status, &cmd]);
    }

    println!("{table}");
    Ok(())
}
