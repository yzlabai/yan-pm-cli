use std::path::Path;

use anyhow::Result;
use colored::Colorize;

use crate::config;
use crate::local::directory::{AutoRunConfig, LocalDirectory};

/// Enable auto-run for the current workspace.
pub fn enable(
    budget: Option<f64>,
    concurrency: Option<u32>,
    filter_priority: Option<&str>,
    agent: Option<&str>,
) -> Result<()> {
    let ws = config::find_workspace_link(None);
    let ws =
        ws.ok_or_else(|| anyhow::anyhow!("当前目录未关联项目。请先运行 yan-pm link <project>"))?;

    let local_dir = LocalDirectory::new(Path::new(&ws.path));
    let mut config = local_dir
        .load_config()
        .ok_or_else(|| anyhow::anyhow!("本地配置不存在。请先运行 yan-pm link"))?;

    config.auto_run = AutoRunConfig {
        enabled: true,
        budget,
        concurrency: concurrency.unwrap_or(1),
        filter_priority: filter_priority
            .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_default(),
        agent: agent.unwrap_or("claude").to_string(),
    };

    local_dir.save_config(&config)?;

    println!("{} Auto-run 已启用", "✓".green());
    print_config(&config.auto_run);
    println!(
        "\n{}",
        "提示: 启动 daemon 后 auto-run 将自动生效 (yan-pm daemon start)".dimmed()
    );
    Ok(())
}

/// Disable auto-run for the current workspace.
pub fn disable() -> Result<()> {
    let ws = config::find_workspace_link(None);
    let ws = ws.ok_or_else(|| anyhow::anyhow!("当前目录未关联项目"))?;

    let local_dir = LocalDirectory::new(Path::new(&ws.path));
    let mut config = local_dir
        .load_config()
        .ok_or_else(|| anyhow::anyhow!("本地配置不存在"))?;

    config.auto_run.enabled = false;
    local_dir.save_config(&config)?;

    println!("{} Auto-run 已禁用", "✓".green());
    Ok(())
}

/// Show auto-run status for the current workspace.
pub fn status() -> Result<()> {
    let ws = config::find_workspace_link(None);
    let ws = ws.ok_or_else(|| anyhow::anyhow!("当前目录未关联项目"))?;

    let local_dir = LocalDirectory::new(Path::new(&ws.path));
    let config = local_dir
        .load_config()
        .ok_or_else(|| anyhow::anyhow!("本地配置不存在"))?;

    if config.auto_run.enabled {
        println!("{} Auto-run: {}", "●".green(), "已启用".green());
    } else {
        println!("{} Auto-run: {}", "●".red(), "已禁用".dimmed());
    }
    print_config(&config.auto_run);

    // Check daemon status
    if crate::daemon::pid::check_running().is_some() {
        println!("\n{} Daemon 运行中", "●".green());
    } else {
        println!(
            "\n{} Daemon 未运行 — auto-run 需要 daemon (yan-pm daemon start)",
            "●".red()
        );
    }

    Ok(())
}

fn print_config(config: &AutoRunConfig) {
    println!("  Agent: {}", config.agent.cyan());
    println!("  并发: {}", config.concurrency);
    if let Some(budget) = config.budget {
        println!("  预算: ${budget:.2}");
    } else {
        println!("  预算: {}", "无限制".dimmed());
    }
    if !config.filter_priority.is_empty() {
        println!("  优先级过滤: {}", config.filter_priority.join(", "));
    }
}
