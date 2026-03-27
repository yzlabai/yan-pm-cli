use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use colored::Colorize;

use crate::agent;
use crate::api::client::*;
use crate::config;
use crate::runner;

use super::make_client;

const WORKSPACE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(120);

pub async fn run(
    url: Option<&str>,
    token: Option<&str>,
    project_id: &str,
    task_id: Option<&str>,
    auto: bool,
    budget: Option<f64>,
    total_budget: Option<f64>,
    cwd_override: Option<&str>,
    agent_name: &str,
    model: Option<&str>,
    permission_mode: &str,
    tools: Option<&str>,
    mcp_config: Option<&str>,
    verbose: bool,
) -> Result<()> {
    let client = make_client(url, token)?;

    // Resolve agent
    let agent_def = agent::find_agent(agent_name);
    let agent_def = match agent_def {
        Some(a) => a,
        None => {
            let available = agent::load_agents();
            let names: Vec<&str> = available.iter().map(|a| a.name.as_str()).collect();
            bail!("未知 Agent: {agent_name}。可用: {}", names.join(", "));
        }
    };

    // Check availability
    if !agent::is_command_available(&agent_def.command).await {
        bail!(
            "{} CLI 未找到 (命令: {})。请确保已安装且在 PATH 中。",
            agent_def.name,
            agent_def.command
        );
    }

    let cwd = if let Some(dir) = cwd_override {
        dir.to_string()
    } else {
        std::env::current_dir()?.to_string_lossy().to_string()
    };

    // Start workspace heartbeat (register workspace + 2min interval)
    let workspace_id = start_workspace_heartbeat(&client).await;

    // Budget tracking depends on agent reporting cost (ACP currently doesn't expose cost_usd)
    if budget.is_some() || total_budget.is_some() {
        eprintln!(
            "{}",
            "⚠ 预算限制依赖 Agent 上报费用信息，当前 ACP 协议暂不支持，设置仅作参考".yellow()
        );
    }

    let runner_opts = runner::TaskRunnerOptions {
        cwd,
        workspace_id: workspace_id.clone().map(|(_pid, wid)| wid),
        max_budget_usd: budget,
        permission_mode: Some(permission_mode.to_string()),
        model: model.map(String::from),
        allowed_tools: tools.map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
        mcp_configs: mcp_config.map(|p| vec![p.to_string()]),
        verbose,
        agent: agent_def,
    };

    let start_opts = runner::StartOptions {
        project_id: project_id.to_string(),
        task_id: task_id.map(String::from),
        auto,
        total_budget_usd: total_budget.or(budget),
        runner: runner_opts,
    };

    // Spawn workspace heartbeat background task
    let ws_heartbeat_running = Arc::new(AtomicBool::new(true));
    let hb_handle = if let Some((pid, wid)) = &workspace_id {
        let flag = ws_heartbeat_running.clone();
        let hb_url = client.base_url().to_string();
        let hb_token = client.token().to_string();
        let hb_pid = pid.clone();
        let hb_wid = wid.clone();
        Some(tokio::spawn(async move {
            let hb_client = match ApiClient::new(&hb_url, &hb_token) {
                Ok(c) => c,
                Err(_) => return,
            };
            let mut interval = tokio::time::interval(WORKSPACE_HEARTBEAT_INTERVAL);
            interval.tick().await; // skip immediate first tick
            while flag.load(Ordering::Acquire) {
                interval.tick().await;
                if !flag.load(Ordering::Acquire) {
                    break;
                }
                let _ = hb_client.workspace_heartbeat(&hb_pid, &hb_wid, None).await;
            }
        }))
    } else {
        None
    };

    let result = runner::start(&client, start_opts).await;

    // Stop workspace heartbeat
    ws_heartbeat_running.store(false, Ordering::Release);
    if let Some(h) = hb_handle {
        let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
    }

    result
}

/// Register workspace on server and return (project_id, workspace_id)
async fn start_workspace_heartbeat(client: &ApiClient) -> Option<(String, String)> {
    let link = config::find_workspace_link(None)?;
    let machine_id = config::get_machine_id();
    let name = std::path::Path::new(&link.path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());

    let data = RegisterWorkspaceData {
        name,
        local_path: link.path.to_string(),
        machine_id,
        metadata: None,
    };

    let ws = client
        .register_workspace(&link.project_id, &data)
        .await
        .ok()?;

    eprintln!(
        "📡 工作区心跳已启动 (workspace={})",
        &ws.id[..8.min(ws.id.len())]
    );
    Some((link.project_id, ws.id))
}
