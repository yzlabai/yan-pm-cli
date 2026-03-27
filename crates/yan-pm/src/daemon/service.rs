use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use colored::Colorize;

/// Install daemon as a system service (launchd on macOS, systemd on Linux).
pub fn install() -> Result<()> {
    let exe = std::env::current_exe().context("Failed to get executable path")?;
    let exe_path = exe.to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    return install_launchd(&exe_path);

    #[cfg(target_os = "linux")]
    return install_systemd(&exe_path);

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    bail!("系统服务安装暂不支持当前平台。请使用 `yan-pm daemon start` 手动启动。");
}

/// Uninstall the daemon system service.
pub fn uninstall() -> Result<()> {
    #[cfg(target_os = "macos")]
    return uninstall_launchd();

    #[cfg(target_os = "linux")]
    return uninstall_systemd();

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    bail!("系统服务卸载暂不支持当前平台");
}

// ---- macOS launchd ----

#[cfg(target_os = "macos")]
const PLIST_LABEL: &str = "chat.yan.yan-pm-cli";

#[cfg(target_os = "macos")]
fn plist_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{PLIST_LABEL}.plist"))
}

/// Escape a string for safe inclusion in XML text content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn install_launchd(exe_path: &str) -> Result<()> {
    let plist = plist_path();
    let plist_dir = plist.parent().unwrap();
    fs::create_dir_all(plist_dir)?;

    let safe_exe = xml_escape(exe_path);
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{PLIST_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{safe_exe}</string>
        <string>daemon</string>
        <string>start</string>
        <string>--foreground</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    <string>/tmp/yan-pm-daemon.stdout.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/yan-pm-daemon.stderr.log</string>
</dict>
</plist>
"#
    );

    fs::write(&plist, &content).context("Failed to write plist")?;

    let uid = unsafe { libc::getuid() };
    let status = std::process::Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{uid}"), &plist.to_string_lossy()])
        .status()
        .context("Failed to run launchctl bootstrap")?;

    if status.success() {
        println!("{} 系统服务已安装 (launchd)", "✓".green());
        println!("  Plist: {}", plist.display());
        println!("  服务将在登录后自动启动，异常退出自动重启");
    } else {
        bail!(
            "launchctl bootstrap 失败 (exit code: {})",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_launchd() -> Result<()> {
    let plist = plist_path();
    let uid = unsafe { libc::getuid() };

    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}"), &plist.to_string_lossy()])
        .status();

    if plist.exists() {
        fs::remove_file(&plist).context("Failed to remove plist")?;
    }

    println!("{} 系统服务已卸载 (launchd)", "✓".green());
    Ok(())
}

// ---- Linux systemd ----

#[cfg(target_os = "linux")]
fn service_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user")
        .join("yan-pm-cli.service")
}

#[cfg(target_os = "linux")]
fn install_systemd(exe_path: &str) -> Result<()> {
    let service = service_path();
    let service_dir = service.parent().unwrap();
    fs::create_dir_all(service_dir)?;

    // systemd ExecStart requires quoting paths with special characters
    let exec_path = if exe_path.contains(|c: char| c.is_whitespace() || c == '"') {
        format!("\"{}\"", exe_path.replace('"', "\\\""))
    } else {
        exe_path.to_string()
    };
    let content = format!(
        r#"[Unit]
Description=yan-pm daemon
After=network-online.target

[Service]
Type=simple
ExecStart={exec_path} daemon start --foreground
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#
    );

    fs::write(&service, &content).context("Failed to write systemd unit")?;

    let reload = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .context("Failed to run systemctl daemon-reload")?;

    if !reload.success() {
        bail!("systemctl daemon-reload 失败");
    }

    let enable = std::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", "yan-pm"])
        .status()
        .context("Failed to run systemctl enable")?;

    if enable.success() {
        println!("{} 系统服务已安装 (systemd --user)", "✓".green());
        println!("  Unit: {}", service.display());
        println!("  服务将在登录后自动启动，异常退出自动重启");
    } else {
        bail!("systemctl enable 失败");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_systemd() -> Result<()> {
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", "--now", "yan-pm"])
        .status();

    let service = service_path();
    if service.exists() {
        fs::remove_file(&service).context("Failed to remove systemd unit")?;
    }

    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    println!("{} 系统服务已卸载 (systemd --user)", "✓".green());
    Ok(())
}
