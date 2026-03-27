use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use crate::config::config_dir;

/// PID file path: ~/.config/yan-pm-cli/daemon.pid
fn pid_file() -> PathBuf {
    config_dir().join("daemon.pid")
}

/// Write the current process PID to the PID file.
pub fn write_pid() -> Result<()> {
    let path = pid_file();
    let pid = std::process::id();
    let tmp = path.with_extension("pid.tmp");
    fs::write(&tmp, pid.to_string())?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Remove the PID file (on clean shutdown).
pub fn remove_pid() {
    let _ = fs::remove_file(pid_file());
}

/// Read PID from file, if it exists.
pub fn read_pid() -> Option<u32> {
    let path = pid_file();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

/// Check if a process with the given PID is alive.
#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    // kill(pid, 0) checks if process exists without sending a signal
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: u32) -> bool {
    // Fallback: assume not alive on non-Unix
    false
}

/// Check if daemon is already running. Returns the PID if running.
pub fn check_running() -> Option<u32> {
    if let Some(pid) = read_pid() {
        if is_process_alive(pid) {
            return Some(pid);
        }
        // Stale PID file — clean up
        remove_pid();
    }
    None
}

/// Acquire the PID lock. Fails if daemon is already running.
pub fn acquire_lock() -> Result<()> {
    if let Some(pid) = check_running() {
        bail!("Daemon 已在运行 (PID: {pid})。使用 `yan-pm daemon stop` 停止后重试。");
    }
    write_pid().context("无法写入 PID 文件")?;
    Ok(())
}
