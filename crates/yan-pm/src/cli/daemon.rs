use std::io::{BufRead, BufReader};

use anyhow::Result;
use colored::Colorize;

use crate::config::config_dir;
use crate::daemon::{pid, process, service, state::DaemonState};

pub async fn start(url: Option<&str>, token: Option<&str>, foreground: bool) -> Result<()> {
    if foreground {
        // Run in foreground (blocking)
        process::run_foreground(url, token).await
    } else {
        // Check if already running
        if let Some(pid) = pid::check_running() {
            println!("Daemon 已在运行 (PID: {pid})");
            return Ok(());
        }
        // Fork to background
        process::fork_daemon()
    }
}

pub fn stop() -> Result<()> {
    process::stop_daemon()
}

pub fn restart(_url: Option<&str>, _token: Option<&str>) -> Result<()> {
    // Stop if running
    if pid::check_running().is_some() {
        process::stop_daemon()?;
        // Wait for process to actually exit (up to 5s)
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if pid::check_running().is_none() {
                break;
            }
        }
        if pid::check_running().is_some() {
            anyhow::bail!("Daemon 进程未能在 5s 内退出");
        }
    }
    // Start fresh (fork to background)
    process::fork_daemon()
}

pub fn status() -> Result<()> {
    match pid::check_running() {
        Some(pid) => {
            println!("{} Daemon 运行中 (PID: {pid})", "●".green());

            // Read state file for details
            if let Some(state) = DaemonState::load() {
                println!("  启动时间: {}", state.started_at);
                println!("  工作区数: {}", state.workspaces.len());
                for ws in &state.workspaces {
                    let sync_info = ws.last_sync.as_deref().unwrap_or("从未同步");
                    let auto_run = if ws.auto_run { "开启" } else { "关闭" };
                    println!(
                        "    {} (项目: {}, 上次同步: {}, 自动执行: {})",
                        ws.path, ws.project_id, sync_info, auto_run
                    );
                }
            }
        }
        None => {
            println!("{} Daemon 未运行", "●".red());
        }
    }
    Ok(())
}

pub fn logs(follow: bool) -> Result<()> {
    let log_file = config_dir().join("daemon.log");
    if !log_file.exists() {
        println!("暂无日志文件");
        return Ok(());
    }

    if follow {
        // Tail -f behavior
        let file = std::fs::File::open(&log_file)?;
        let reader = BufReader::new(file);

        // Print existing content
        for line in reader.lines().map_while(Result::ok) {
            println!("{line}");
        }

        // Then watch for new content
        println!("{}", "--- 等待新日志 (Ctrl+C 退出) ---".dimmed());
        let mut last_size = std::fs::metadata(&log_file)?.len();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let current_size = match std::fs::metadata(&log_file) {
                Ok(m) => m.len(),
                Err(_) => continue,
            };
            if current_size < last_size {
                // Log file was truncated/rotated — restart from beginning
                last_size = 0;
            }
            if current_size > last_size {
                let file = std::fs::File::open(&log_file)?;
                let mut reader = BufReader::new(file);
                use std::io::Seek;
                reader.seek(std::io::SeekFrom::Start(last_size))?;
                for line in reader.lines().map_while(Result::ok) {
                    println!("{line}");
                }
                last_size = current_size;
            }
        }
    } else {
        // Print last 50 lines
        let content = std::fs::read_to_string(&log_file)?;
        let lines: Vec<&str> = content.lines().collect();
        let start = if lines.len() > 50 {
            lines.len() - 50
        } else {
            0
        };
        for line in &lines[start..] {
            println!("{line}");
        }
        if start > 0 {
            println!(
                "\n{} 更多日志使用 `yan daemon logs -f` 跟踪",
                "...".dimmed()
            );
        }
    }

    Ok(())
}

pub fn install() -> Result<()> {
    service::install()
}

pub fn uninstall() -> Result<()> {
    service::uninstall()
}
