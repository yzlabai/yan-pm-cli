use std::sync::Arc;

use crate::api::client::ApiClient;

/// Manages periodic heartbeat for all workspaces.
pub struct HeartbeatManager {
    client: Arc<ApiClient>,
}

impl HeartbeatManager {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self { client }
    }

    /// Send heartbeat for a workspace.
    pub async fn send_heartbeat(&self, project_id: &str, workspace_id: &str) {
        match self
            .client
            .workspace_heartbeat(project_id, workspace_id, None)
            .await
        {
            Ok(_) => {
                tracing::debug!("Heartbeat OK: project={project_id} workspace={workspace_id}");
            }
            Err(e) => {
                tracing::warn!(
                    "Heartbeat failed: project={project_id} workspace={workspace_id}: {e}"
                );
            }
        }
    }
}
