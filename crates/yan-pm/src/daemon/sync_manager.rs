use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;

use crate::api::client::ApiClient;
use crate::sync::engine::SyncEngine;

/// Manages N SyncEngines, one per linked workspace.
pub struct SyncManager {
    client: Arc<ApiClient>,
    engines: HashMap<String, SyncEngine>,
    /// Track last sync time per workspace path
    last_sync_times: HashMap<String, String>,
}

impl SyncManager {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            engines: HashMap::new(),
            last_sync_times: HashMap::new(),
        }
    }

    /// Register a workspace for syncing.
    pub fn add_workspace(&mut self, path: &str, project_id: &str) -> Result<()> {
        let workspace_root = PathBuf::from(path);
        if !workspace_root.exists() {
            tracing::warn!("Workspace path does not exist: {path}");
            return Ok(());
        }

        let mut engine = SyncEngine::new(&workspace_root, project_id);
        engine.init_cache().context("Failed to init sync cache")?;
        self.engines.insert(path.to_string(), engine);
        Ok(())
    }

    /// Sync all registered workspaces.
    pub async fn sync_all(&mut self) -> Result<()> {
        let paths: Vec<String> = self.engines.keys().cloned().collect();
        let mut errors = Vec::new();

        for path in &paths {
            if let Err(e) = self.sync_workspace_inner(path).await {
                tracing::error!("Sync failed for {path}: {e}");
                errors.push(format!("{path}: {e}"));
            }
        }

        if !errors.is_empty() {
            anyhow::bail!("{} workspace(s) failed to sync", errors.len());
        }
        Ok(())
    }

    /// Sync a single workspace by path.
    pub async fn sync_workspace(&mut self, path: &str) -> Result<()> {
        self.sync_workspace_inner(path).await
    }

    async fn sync_workspace_inner(&mut self, path: &str) -> Result<()> {
        let engine = self
            .engines
            .get_mut(path)
            .context("Workspace not registered")?;

        let result = engine.full_sync(&self.client).await?;
        self.last_sync_times
            .insert(path.to_string(), Utc::now().to_rfc3339());

        tracing::info!(
            "Sync {}: ↓{} pulled ↑{} pushed 📦{} archived",
            path,
            result.pulled,
            result.pushed,
            result.archived
        );

        if !result.errors.is_empty() {
            for err in &result.errors {
                tracing::warn!("  {err}");
            }
        }

        Ok(())
    }

    /// Get last sync time for a workspace.
    pub fn get_last_sync(&self, path: &str) -> Option<String> {
        self.last_sync_times.get(path).cloned()
    }

    /// Check if a workspace path is managed.
    #[allow(dead_code)]
    pub fn has_workspace(&self, path: &str) -> bool {
        self.engines.contains_key(path)
    }
}
