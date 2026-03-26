use anyhow::Result;

use crate::output;
use super::make_client;

pub async fn list(url: Option<&str>, token: Option<&str>, json: bool) -> Result<()> {
    let client = make_client(url, token)?;
    let projects = client.list_projects().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&projects)?);
    } else {
        output::print_projects(&projects);
    }
    Ok(())
}

pub async fn report(url: Option<&str>, token: Option<&str>, json: bool, project_id: &str) -> Result<()> {
    let client = make_client(url, token)?;
    let report = client.generate_report(project_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.report);
    }
    Ok(())
}
