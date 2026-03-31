pub mod agents;
pub mod auto_run;
pub mod daemon;
pub mod dashboard;
pub mod detect;
pub mod issue;
pub mod login;
pub mod project;
pub mod pull;
pub mod self_update;
pub mod setup;
pub mod spec;
pub mod start;
pub mod sync;
pub mod task;
pub mod verify;
pub mod workspace;

use anyhow::Result;

use crate::api::client::ApiClient;
use crate::config;

/// Build ApiClient from CLI args or config
pub fn make_client(url: Option<&str>, token: Option<&str>) -> Result<ApiClient> {
    let resolved = config::resolve_config(url, token);
    if resolved.base_url.is_empty() || resolved.token.is_empty() {
        anyhow::bail!(
            "未配置。请先运行 `yan login` 或设置环境变量:\n  export YAN_PM_BASE_URL=https://your-domain.com\n  export YAN_PM_TOKEN=your_token"
        );
    }
    ApiClient::new(&resolved.base_url, &resolved.token).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Fire-and-forget activity reporting. Logs warning on failure, never blocks.
pub async fn report_activity_quiet(
    api: &crate::api::client::ApiClient,
    project_id: &str,
    issue_id: &str,
    action: &str,
    detail: Option<serde_json::Value>,
    actor_name: &str,
) {
    if let Err(e) = api
        .report_activity(project_id, issue_id, action, detail, Some(actor_name))
        .await
    {
        tracing::warn!("Activity report failed: {}", e);
    }
}

/// Get workspace name from link path
pub fn workspace_name_from_link(link: &crate::config::workspace::WorkspaceEntry) -> String {
    std::path::Path::new(&link.path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}
