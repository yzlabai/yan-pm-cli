use std::path::Path;

use anyhow::{bail, Result};
use colored::Colorize;

use crate::config;
use crate::sync::engine::{print_sync_result, SyncEngine};

pub async fn run(url: Option<&str>, token: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let link = config::find_workspace_link(Some(Path::new(&cwd.to_string_lossy().as_ref())));

    let entry = match link {
        Some(e) => e,
        None => bail!("当前目录未关联到项目。请先运行 `yan-pm link <project>`"),
    };

    let client = super::make_client(url, token)?;
    let mut engine = SyncEngine::new(&cwd, &entry.project_id);

    // Initialize cache from existing local files
    engine.init_cache()?;

    println!("{}", "⟳ 正在同步...".cyan());
    let result = engine.full_sync(&client).await?;
    print_sync_result(&result);

    Ok(())
}
