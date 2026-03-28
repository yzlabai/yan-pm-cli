pub mod agents;
pub mod auto_run;
pub mod daemon;
pub mod detect;
pub mod issue;
pub mod login;
pub mod project;
pub mod self_update;
pub mod setup;
pub mod start;
pub mod sync;
pub mod task;
pub mod workspace;

use anyhow::Result;

use crate::api::client::ApiClient;
use crate::config;

/// Build ApiClient from CLI args or config
pub fn make_client(url: Option<&str>, token: Option<&str>) -> Result<ApiClient> {
    let resolved = config::resolve_config(url, token);
    if resolved.base_url.is_empty() || resolved.token.is_empty() {
        anyhow::bail!(
            "未配置。请先运行 `yan-pm login` 或设置环境变量:\n  export YAN_PM_BASE_URL=https://your-domain.com\n  export YAN_PM_TOKEN=your_token"
        );
    }
    ApiClient::new(&resolved.base_url, &resolved.token).map_err(|e| anyhow::anyhow!("{e}"))
}
