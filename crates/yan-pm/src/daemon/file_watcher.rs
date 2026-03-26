use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEvent, Debouncer};
use notify::RecommendedWatcher;
use tokio::sync::mpsc as tokio_mpsc;

const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

/// Watches .yan-pm/tasks/ directories across multiple workspaces.
/// Emits (workspace_path, changed_file) events.
pub struct FileWatcher {
    /// Map workspace_path → watched tasks directory
    watched: HashMap<String, PathBuf>,
    /// The debouncer (must be kept alive)
    _debouncer: Option<Debouncer<RecommendedWatcher>>,
    /// Async receiver bridged from notify's sync channel
    async_rx: Option<tokio_mpsc::UnboundedReceiver<Vec<DebouncedEvent>>>,
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {
            watched: HashMap::new(),
            _debouncer: None,
            async_rx: None,
        }
    }

    /// Start watching a workspace's .yan-pm/tasks/ directory.
    pub fn watch_workspace(&mut self, workspace_path: &str) -> Result<()> {
        let tasks_dir = PathBuf::from(workspace_path).join(".yan-pm").join("tasks");
        if !tasks_dir.exists() {
            tracing::debug!("Tasks dir not found, skipping watch: {}", tasks_dir.display());
            return Ok(());
        }

        // If debouncer not yet created, create it now
        if self._debouncer.is_none() {
            let (async_tx, async_rx) = tokio_mpsc::unbounded_channel();
            // notify uses std::sync::mpsc — bridge to tokio channel
            let (sync_tx, sync_rx) = std::sync::mpsc::channel();
            let debouncer = new_debouncer(DEBOUNCE_DURATION, sync_tx)
                .context("Failed to create file watcher")?;
            self._debouncer = Some(debouncer);
            self.async_rx = Some(async_rx);
            // Spawn a bridge thread: reads from sync_rx, sends to async_tx
            std::thread::spawn(move || {
                while let Ok(result) = sync_rx.recv() {
                    match result {
                        Ok(events) => {
                            if async_tx.send(events).is_err() {
                                break; // Receiver dropped
                            }
                        }
                        Err(e) => {
                            tracing::error!("File watch error: {e}");
                        }
                    }
                }
            });
        }

        if let Some(debouncer) = &mut self._debouncer {
            debouncer
                .watcher()
                .watch(&tasks_dir, RecursiveMode::NonRecursive)
                .context(format!("Failed to watch {}", tasks_dir.display()))?;
        }

        self.watched
            .insert(workspace_path.to_string(), tasks_dir);

        tracing::info!("Watching: {workspace_path}/.yan-pm/tasks/");
        Ok(())
    }

    /// Get the next file change event. Returns (workspace_path, filename).
    /// Awaits efficiently without polling — wakes only when a file changes.
    pub async fn next_event(&mut self) -> Option<(String, String)> {
        let rx = self.async_rx.as_mut()?;
        loop {
            match rx.recv().await {
                Some(events) => {
                    for event in events {
                        let event_path = &event.path;
                        for (ws_path, tasks_dir) in &self.watched {
                            if event_path.starts_with(tasks_dir) {
                                let filename = event_path
                                    .file_name()
                                    .map(|f| f.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                if filename.ends_with(".md") {
                                    return Some((ws_path.clone(), filename));
                                }
                            }
                        }
                    }
                    // Events didn't match any .md file — continue waiting
                }
                None => {
                    tracing::error!("File watcher channel disconnected");
                    return None;
                }
            }
        }
    }

    /// Stop all watchers.
    pub fn stop(&mut self) {
        self._debouncer = None;
        self.async_rx = None;
        self.watched.clear();
    }
}
