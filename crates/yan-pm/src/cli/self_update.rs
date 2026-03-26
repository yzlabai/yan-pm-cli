use anyhow::Result;
use colored::Colorize;

/// Check for and apply updates from GitHub Releases.
pub async fn run() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!(
        "{}",
        format!("当前版本: v{current}").dimmed()
    );
    println!("检查更新...");

    let target = self_update::get_target();
    let status = self_update::backends::github::Update::configure()
        .repo_owner("nicepkg")
        .repo_name("yan-pm")
        .bin_name("yan-pm")
        .target(&target)
        .current_version(current)
        .build()?
        .update()?;

    if status.updated() {
        println!(
            "{} 已更新到 v{}",
            "✓".green(),
            status.version()
        );
    } else {
        println!("{} 已是最新版本 (v{current})", "✓".green());
    }

    Ok(())
}
