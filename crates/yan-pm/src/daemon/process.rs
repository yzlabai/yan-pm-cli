use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use colored::Colorize;
use tokio::sync::watch;

use crate::api::client::ApiClient;
use crate::config;
use crate::local::directory::LocalDirectory;

use super::auto_runner::AutoRunner;
use super::file_watcher::FileWatcher;
use super::heartbeat::HeartbeatManager;
use super::pid;
use super::state::{DaemonState, DaemonWorkspaceState};
use super::sync_manager::SyncManager;

const SYNC_INTERVAL: Duration = Duration::from_secs(30);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(120);
const AUTO_RUN_INTERVAL: Duration = Duration::from_secs(30);

/// Fork the current process and run daemon in background (Unix only).
#[cfg(unix)]
pub fn fork_daemon() -> Result<()> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().context("Failed to get executable path")?;

    // Child inherits env vars (YAN_PM_BASE_URL, YAN_PM_TOKEN) automatically via fork+exec.
    // Config file (~/.config/yan-pm/config.json) is also accessible. No need to pass --url/--token
    // as CLI args (which would expose token in `ps aux` output).
    let args = vec!["daemon".to_string(), "start".to_string(), "--foreground".to_string()];

    // Double-fork to detach from terminal
    match unsafe { libc::fork() } {
        -1 => anyhow::bail!("fork() failed"),
        0 => {
            // Child process
            unsafe { libc::setsid() };

            // Redirect stdio to /dev/null
            let devnull = std::fs::File::open("/dev/null").ok();

            let mut cmd = std::process::Command::new(exe);
            cmd.args(&args);

            if let Some(f) = devnull {
                use std::os::unix::io::AsRawFd;
                let fd = f.as_raw_fd();
                unsafe {
                    libc::dup2(fd, 0); // stdin
                    libc::dup2(fd, 1); // stdout
                    libc::dup2(fd, 2); // stderr
                }
            }

            // exec replaces this process
            let err = cmd.exec();
            eprintln!("exec failed: {err}");
            std::process::exit(1);
        }
        _child_pid => {
            // Parent process — daemon is starting
            // Wait briefly for PID file to appear
            std::thread::sleep(Duration::from_millis(500));
            if let Some(pid) = pid::read_pid() {
                println!("{} Daemon 已启动 (PID: {pid})", "✓".green());
            } else {
                println!("{} Daemon 正在启动...", "⟳".cyan());
            }
            Ok(())
        }
    }
}

#[cfg(not(unix))]
pub fn fork_daemon() -> Result<()> {
    let exe = std::env::current_exe().context("Failed to get executable path")?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["daemon", "start", "--foreground"]);
    // CREATE_NO_WINDOW flag on Windows would be set here
    cmd.spawn().context("Failed to start daemon process")?;
    std::thread::sleep(Duration::from_millis(500));
    if let Some(pid) = pid::read_pid() {
        println!("{} Daemon 已启动 (PID: {pid})", "✓".green());
    }
    Ok(())
}

/// Run daemon in foreground (called by `daemon start --foreground`).
pub async fn run_foreground(url: Option<&str>, token: Option<&str>) -> Result<()> {
    // Acquire PID lock
    pid::acquire_lock()?;

    let client = Arc::new(make_daemon_client(url, token)?);
    let pid = std::process::id();

    // Setup logging to file
    let log_file = config::config_dir().join("daemon.log");
    setup_daemon_logging(&log_file)?;

    tracing::info!("Daemon starting (PID: {pid})");

    // Load all linked workspaces
    let workspace_entries = config::list_all_workspace_links();
    if workspace_entries.is_empty() {
        tracing::warn!("No linked workspaces found. Daemon will idle.");
    }

    // Initialize daemon state
    let mut daemon_state = DaemonState::new(pid);
    for ws in &workspace_entries {
        let local_dir = LocalDirectory::new(std::path::Path::new(&ws.path));
        let local_config = local_dir.load_config();
        let auto_run_enabled = local_config.as_ref().map(|c| c.auto_run.enabled).unwrap_or(false);
        daemon_state.workspaces.push(DaemonWorkspaceState {
            path: ws.path.clone(),
            project_id: ws.project_id.clone(),
            last_sync: None,
            auto_run: auto_run_enabled,
        });
    }
    daemon_state.save()?;

    // Create shutdown signal
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Initialize components
    let mut sync_manager = SyncManager::new(client.clone());
    for ws in &workspace_entries {
        sync_manager.add_workspace(&ws.path, &ws.project_id)?;
    }

    let mut file_watcher = FileWatcher::new();
    for ws in &workspace_entries {
        file_watcher.watch_workspace(&ws.path)?;
    }

    let heartbeat_manager = HeartbeatManager::new(client.clone());
    let hb_workspaces: Vec<(String, String, Option<String>)> = workspace_entries
        .iter()
        .map(|ws| {
            (
                ws.project_id.clone(),
                ws.path.clone(),
                ws.workspace_id.clone(),
            )
        })
        .collect();

    // Initialize AutoRunner
    let mut auto_runner = AutoRunner::new(client.clone());
    for ws in &workspace_entries {
        let local_dir = LocalDirectory::new(std::path::Path::new(&ws.path));
        if let Some(local_config) = local_dir.load_config() {
            auto_runner.set_workspace(&ws.path, &ws.project_id, local_config.auto_run);
        }
    }

    tracing::info!(
        "Daemon ready: {} workspace(s) monitored",
        workspace_entries.len()
    );

    // Main event loop
    let mut sync_interval = tokio::time::interval(SYNC_INTERVAL);
    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    let mut auto_run_interval = tokio::time::interval(AUTO_RUN_INTERVAL);
    // Skip the immediate first tick
    sync_interval.tick().await;
    heartbeat_interval.tick().await;
    auto_run_interval.tick().await;

    let mut shutdown_rx_clone = shutdown_rx.clone();

    // Register signal handlers
    let shutdown_tx_signal = shutdown_tx.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Received shutdown signal");
        let _ = shutdown_tx_signal.send(true);
    });

    #[cfg(unix)]
    {
        let shutdown_tx_term = shutdown_tx.clone();
        tokio::spawn(async move {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("Failed to register SIGTERM handler");
            sigterm.recv().await;
            tracing::info!("Received SIGTERM");
            let _ = shutdown_tx_term.send(true);
        });
    }

    // Do an initial full sync
    tracing::info!("Running initial sync...");
    if let Err(e) = sync_manager.sync_all().await {
        tracing::error!("Initial sync failed: {e}");
    }
    update_state_sync_times(&mut daemon_state, &sync_manager);
    let _ = daemon_state.save();

    loop {
        tokio::select! {
            _ = sync_interval.tick() => {
                tracing::debug!("Periodic sync tick");
                if let Err(e) = sync_manager.sync_all().await {
                    tracing::error!("Sync error: {e}");
                }
                update_state_sync_times(&mut daemon_state, &sync_manager);
                // Reload auto-run configs (user may have changed them)
                reload_auto_run_configs(&mut auto_runner, &workspace_entries);
                update_state_auto_run(&mut daemon_state, &workspace_entries);
                let _ = daemon_state.save();
            }

            _ = heartbeat_interval.tick() => {
                tracing::debug!("Heartbeat tick");
                for (project_id, _path, workspace_id) in &hb_workspaces {
                    if let Some(wid) = workspace_id {
                        heartbeat_manager.send_heartbeat(project_id, wid).await;
                    }
                }
            }

            _ = auto_run_interval.tick() => {
                // Collect completed tasks first
                auto_runner.collect_completed().await;
                // Then check for new tasks to run
                auto_runner.check_and_run().await;
            }

            event = file_watcher.next_event() => {
                if let Some((workspace_path, changed_file)) = event {
                    tracing::info!("File changed: {} in {}", changed_file, workspace_path);
                    if let Err(e) = sync_manager.sync_workspace(&workspace_path).await {
                        tracing::error!("Sync after file change error: {e}");
                    }
                    update_state_sync_times(&mut daemon_state, &sync_manager);
                    let _ = daemon_state.save();
                }
            }

            _ = shutdown_rx_clone.changed() => {
                if *shutdown_rx_clone.borrow() {
                    tracing::info!("Shutting down daemon...");
                    break;
                }
            }
        }
    }

    // Cleanup
    auto_runner.shutdown().await;
    file_watcher.stop();
    pid::remove_pid();
    DaemonState::remove();
    tracing::info!("Daemon stopped cleanly");
    Ok(())
}

/// Stop a running daemon by sending SIGTERM.
pub fn stop_daemon() -> Result<()> {
    let pid = pid::check_running();
    match pid {
        Some(pid) => {
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            #[cfg(not(unix))]
            {
                // On non-Unix, try taskkill
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output();
            }
            // Wait for process to exit
            for _ in 0..20 {
                std::thread::sleep(Duration::from_millis(250));
                if !pid::is_process_alive(pid) {
                    pid::remove_pid();
                    DaemonState::remove();
                    println!("{} Daemon 已停止 (PID: {pid})", "✓".green());
                    return Ok(());
                }
            }
            // Force kill
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
            pid::remove_pid();
            DaemonState::remove();
            println!("{} Daemon 已强制停止 (PID: {pid})", "✓".yellow());
            Ok(())
        }
        None => {
            println!("Daemon 未在运行");
            Ok(())
        }
    }
}

fn make_daemon_client(url: Option<&str>, token: Option<&str>) -> Result<ApiClient> {
    let resolved = config::resolve_config(url, token);
    if resolved.base_url.is_empty() || resolved.token.is_empty() {
        anyhow::bail!("未配置 API。请先运行 `yan-pm login`");
    }
    Ok(ApiClient::new(&resolved.base_url, &resolved.token)?)
}

fn setup_daemon_logging(log_file: &std::path::Path) -> Result<()> {
    use tracing_subscriber::prelude::*;

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .context("Failed to open daemon log file")?;

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .with_target(false);

    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set tracing subscriber")?;

    Ok(())
}

fn update_state_sync_times(state: &mut DaemonState, sync_manager: &SyncManager) {
    for ws_state in &mut state.workspaces {
        if let Some(last_sync) = sync_manager.get_last_sync(&ws_state.path) {
            ws_state.last_sync = Some(last_sync);
        }
    }
}

fn reload_auto_run_configs(
    auto_runner: &mut AutoRunner,
    workspace_entries: &[config::WorkspaceEntry],
) {
    for ws in workspace_entries {
        let local_dir = LocalDirectory::new(std::path::Path::new(&ws.path));
        if let Some(local_config) = local_dir.load_config() {
            auto_runner.set_workspace(&ws.path, &ws.project_id, local_config.auto_run);
        }
    }
}

fn update_state_auto_run(
    state: &mut DaemonState,
    _workspace_entries: &[config::WorkspaceEntry],
) {
    for ws_state in &mut state.workspaces {
        let local_dir = LocalDirectory::new(std::path::Path::new(&ws_state.path));
        if let Some(local_config) = local_dir.load_config() {
            ws_state.auto_run = local_config.auto_run.enabled;
        }
    }
}
