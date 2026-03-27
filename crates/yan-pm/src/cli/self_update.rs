use anyhow::Result;
use colored::Colorize;

/// Self-update placeholder — not yet implemented.
pub async fn run() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("{}", format!("当前版本: v{current}").dimmed());
    println!("{} 自动更新暂未开放，请从源码重新构建：", "!".yellow());
    println!("  cd yan-pm && cargo build --release");
    Ok(())
}
