use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;

const GITHUB_REPO: &str = "yzlabai/yan-pm-cli";
const BINARY_NAME: &str = "yan-pm-cli";

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn current_target() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
}

fn parse_version(tag: &str) -> Option<&str> {
    tag.strip_prefix('v').or(Some(tag))
}

fn version_newer(remote: &str, local: &str) -> bool {
    let parse =
        |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse::<u32>().ok()).collect() };
    let r = parse(remote);
    let l = parse(local);
    r > l
}

pub async fn run() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("{}", format!("当前版本: v{current}").dimmed());
    println!("{}", "检查最新版本...".dimmed());

    // 1. Fetch latest release from GitHub API
    let client = reqwest::Client::builder()
        .user_agent(format!("{BINARY_NAME}/{current}"))
        .build()?;

    let release: GithubRelease = client
        .get(format!(
            "https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
        ))
        .send()
        .await
        .context("无法连接 GitHub API")?
        .error_for_status()
        .context("GitHub API 请求失败（可能还没有发布版本）")?
        .json()
        .await
        .context("解析 release 信息失败")?;

    let remote_version = parse_version(&release.tag_name).unwrap_or(&release.tag_name);

    if !version_newer(remote_version, current) {
        println!("{} 已是最新版本 (v{current})", "✓".green().bold());
        return Ok(());
    }

    println!(
        "发现新版本: {} → {}",
        format!("v{current}").red(),
        format!("v{remote_version}").green().bold()
    );

    // 2. Find matching asset for current platform
    let target = current_target();
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.contains(target) && a.name.ends_with(".tar.gz"))
        .with_context(|| format!("未找到当前平台 ({target}) 的构建产物"))?;

    println!("{}", format!("下载 {}...", asset.name).dimmed());

    // 3. Download
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .context("下载失败")?
        .error_for_status()
        .context("下载失败")?
        .bytes()
        .await?;

    println!(
        "{}",
        format!("已下载 ({:.1} MB)", bytes.len() as f64 / 1_048_576.0).dimmed()
    );

    // 4. Extract binary from tar.gz
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(&bytes));
    let mut archive = tar::Archive::new(decoder);

    let mut new_binary: Option<Vec<u8>> = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        if path.file_name().is_some_and(|n| n == BINARY_NAME) {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf)?;
            new_binary = Some(buf);
            break;
        }
    }

    let new_binary = new_binary.context("tar.gz 中未找到 yan-pm-cli 二进制")?;

    // 5. Replace current binary
    let current_exe = std::env::current_exe().context("无法获取当前二进制路径")?;
    let current_exe = current_exe.canonicalize().unwrap_or(current_exe);

    replace_binary(&current_exe, &new_binary)?;

    println!("{} 已更新到 v{remote_version}", "✓".green().bold());
    Ok(())
}

fn replace_binary(exe_path: &PathBuf, new_binary: &[u8]) -> Result<()> {
    let dir = exe_path.parent().context("无法获取二进制所在目录")?;

    // Write new binary to temp file in same directory (ensures same filesystem for rename)
    let tmp_path = dir.join(format!(".{BINARY_NAME}.update.tmp"));
    let backup_path = dir.join(format!(".{BINARY_NAME}.backup"));

    // Write new binary
    {
        let mut file =
            std::fs::File::create(&tmp_path).context("无法创建临时文件（权限不足？试试 sudo）")?;
        file.write_all(new_binary)?;
    }

    // Set executable permission
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Atomic replace: backup old → rename new → remove backup
    if exe_path.exists() {
        std::fs::rename(exe_path, &backup_path).context("无法备份旧版本（权限不足？试试 sudo）")?;
    }

    if let Err(e) = std::fs::rename(&tmp_path, exe_path) {
        // Rollback: restore backup
        let _ = std::fs::rename(&backup_path, exe_path);
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e).context("无法替换二进制（权限不足？试试 sudo）");
    }

    // Clean up backup
    let _ = std::fs::remove_file(&backup_path);

    Ok(())
}
