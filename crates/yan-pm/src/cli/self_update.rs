use std::env;
use std::fs;
use std::io::Read;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use flate2::read::GzDecoder;
use reqwest::Client;
use serde::Deserialize;
use tar::Archive;

const GITEE_OWNER: &str = "yzlab";
const GITEE_REPO: &str = "xiaoyan";

/// Gitee Release API response (partial).
#[derive(Deserialize)]
struct GiteeRelease {
    tag_name: String,
    assets: Vec<GiteeAsset>,
}

#[derive(Deserialize)]
struct GiteeAsset {
    name: String,
    browser_download_url: String,
}

/// Return Rust target triple for the current platform.
fn current_target() -> &'static str {
    env!("TARGET")
}

/// Parse version string, stripping leading 'v' and 'yan-pm-v' prefix.
fn parse_version(tag: &str) -> Option<&str> {
    tag.strip_prefix("yan-pm-v")
        .or_else(|| tag.strip_prefix('v'))
        .or(Some(tag))
}

/// Check for and apply updates from Gitee Releases.
pub async fn run() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("{}", format!("当前版本: v{current}").dimmed());
    println!("检查更新...");

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Fetch releases with yan-pm-v* tags
    let url = format!(
        "https://gitee.com/api/v5/repos/{GITEE_OWNER}/{GITEE_REPO}/releases?per_page=10"
    );
    let releases: Vec<GiteeRelease> = client
        .get(&url)
        .header("User-Agent", format!("yan-pm/{current}"))
        .send()
        .await
        .context("无法连接 Gitee API")?
        .json()
        .await
        .context("解析 Gitee Release 响应失败")?;

    // Find latest yan-pm release
    let release = releases
        .iter()
        .find(|r| r.tag_name.starts_with("yan-pm-v"))
        .context("未找到 yan-pm 发布版本")?;

    let latest = parse_version(&release.tag_name).unwrap_or("0.0.0");

    if latest <= current {
        println!("{} 已是最新版本 (v{current})", "✓".green());
        return Ok(());
    }

    println!("发现新版本: v{latest}");

    // Find asset matching current target
    let target = current_target();
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    let asset_name = format!("yan-pm-{target}.{ext}");

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .or_else(|| {
            // Also try versioned name
            let versioned = format!("yan-pm-{latest}-{target}.{ext}");
            release.assets.iter().find(|a| a.name == versioned)
        })
        .with_context(|| format!("未找到当前平台 ({target}) 的下载文件"))?;

    println!("下载 {}...", asset.name);

    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .context("下载失败")?
        .bytes()
        .await?;

    // Extract binary from archive
    let current_exe = env::current_exe().context("无法获取当前可执行文件路径")?;

    if cfg!(windows) {
        // ZIP extraction for Windows — write to temp then replace
        #[cfg(windows)]
        {
            let cursor = std::io::Cursor::new(&bytes);
            let mut archive = zip::ZipArchive::new(cursor).context("解压 ZIP 失败")?;
            let mut file = archive.by_name("yan-pm.exe").context("ZIP 中未找到 yan-pm.exe")?;
            let tmp_path = current_exe.with_extension("tmp");
            let mut out = fs::File::create(&tmp_path).context("创建临时文件失败")?;
            std::io::copy(&mut file, &mut out)?;
            drop(out);
            self_replace::self_replace(&tmp_path).context("替换可执行文件失败")?;
            let _ = fs::remove_file(&tmp_path);
        }
    } else {
        // tar.gz extraction for Unix
        let decoder = GzDecoder::new(&bytes[..]);
        let mut archive = Archive::new(decoder);
        let mut found = false;
        for entry in archive.entries().context("读取 tar 归档失败")? {
            let mut entry = entry?;
            let path = entry.path()?;
            if path.file_name().and_then(|n| n.to_str()) == Some("yan-pm") {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                // Atomic replace: write to temp file then rename
                let tmp_path = current_exe.with_extension("tmp");
                fs::write(&tmp_path, &buf).context("写入临时文件失败")?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))?;
                }
                fs::rename(&tmp_path, &current_exe).context("替换可执行文件失败")?;
                found = true;
                break;
            }
        }
        if !found {
            bail!("归档中未找到 yan-pm 二进制文件");
        }
    }

    println!("{} 已更新到 v{latest}", "✓".green());
    Ok(())
}
