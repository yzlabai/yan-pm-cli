use std::collections::HashMap;
use std::sync::Arc;

use crate::api::client::ApiClient;
use crate::local::directory::AutoRunConfig;

use super::event_store::EventStore;

/// AutoRunner manages automatic task execution across workspaces in the daemon.
/// Currently disabled — Task cloud APIs have been removed in Phase 2.
pub struct AutoRunner {
    _client: Arc<ApiClient>,
    slots: HashMap<String, RunnerSlot>,
    _event_store: Option<Arc<EventStore>>,
}

struct RunnerSlot {
    config: AutoRunConfig,
}

impl AutoRunner {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            _client: client,
            slots: HashMap::new(),
            _event_store: None,
        }
    }

    pub fn set_event_store(&mut self, _store: Arc<EventStore>) {
        // Disabled
    }

    /// Register or update a workspace slot.
    pub fn set_workspace(&mut self, path: &str, _project_id: &str, config: AutoRunConfig) {
        if let Some(slot) = self.slots.get_mut(path) {
            slot.config = config;
        } else {
            self.slots.insert(path.to_string(), RunnerSlot { config });
        }
    }

    /// Remove a workspace slot.
    #[allow(dead_code)]
    pub fn remove_workspace(&mut self, path: &str) {
        self.slots.remove(path);
    }

    /// Check all slots — currently a no-op.
    pub async fn check_and_run(&mut self) {
        // Disabled: Task cloud APIs have been removed
    }

    /// Process completed tasks — currently a no-op.
    pub async fn collect_completed(&mut self) {
        // Disabled: Task cloud APIs have been removed
    }

    /// Check if any workspace has auto-run enabled.
    #[allow(dead_code)]
    pub fn has_any_enabled(&self) -> bool {
        self.slots.values().any(|s| s.config.enabled)
    }

    /// Gracefully shut down.
    pub async fn shutdown(&mut self) {
        self.slots.clear();
    }
}
