use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tracing::{info, warn};

use super::event_store::EventStore;
use crate::api::client::ApiClient;

const UPLOAD_BATCH_SIZE: i64 = 50;

pub struct EventUploader {
    store: Arc<EventStore>,
    client: Arc<ApiClient>,
    /// Unique session ID for this daemon instance, used in composite local_id
    session_id: String,
}

impl EventUploader {
    pub fn new(store: Arc<EventStore>, client: Arc<ApiClient>) -> Self {
        // Generate a short unique session ID for composite local_id
        let session_id = format!(
            "{:x}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        Self {
            store,
            client,
            session_id,
        }
    }

    /// Build composite local_id: "{session_id}:{sqlite_rowid}"
    fn local_id(&self, rowid: i64) -> String {
        format!("{}:{}", self.session_id, rowid)
    }

    /// Fetch one batch of unsynced events, upload them grouped by task_id, and
    /// mark them synced. Returns the number of events successfully synced.
    pub async fn upload_batch(&self) -> Result<usize> {
        let events = self.store.fetch_unsynced(UPLOAD_BATCH_SIZE)?;
        if events.is_empty() {
            return Ok(0);
        }

        // Group events by task_id.
        let mut by_task: HashMap<String, Vec<&super::event_store::Event>> = HashMap::new();
        for event in &events {
            by_task.entry(event.task_id.clone()).or_default().push(event);
        }

        let mut synced_ids: Vec<i64> = Vec::new();

        for (task_id, task_events) in &by_task {
            // Extract project_id from first event payload.
            let project_id = task_events
                .iter()
                .find_map(|e| {
                    serde_json::from_str::<Value>(&e.payload)
                        .ok()
                        .and_then(|v| v.get("project_id").and_then(|p| p.as_str()).map(String::from))
                });

            let Some(project_id) = project_id else {
                warn!(task_id = %task_id, "project_id missing in event payload, marking synced to avoid retry loop");
                let ids: Vec<i64> = task_events.iter().map(|e| e.id).collect();
                synced_ids.extend_from_slice(&ids);
                continue;
            };

            // Build complete event records for POST (not just payload).
            let events_json: Vec<Value> = task_events
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "event_type": &e.event_type,
                        "payload": &e.payload,
                        "created_at": &e.created_at,
                        "local_id": self.local_id(e.id),
                    })
                })
                .collect();

            let body = serde_json::json!({ "events": events_json });
            let path = format!(
                "/projects/{}/tasks/{}/events",
                urlencoded(&project_id),
                urlencoded(task_id)
            );

            match self.client.post_raw(&path, &body).await {
                Ok(_) => {
                    let ids: Vec<i64> = task_events.iter().map(|e| e.id).collect();
                    info!(
                        task_id = %task_id,
                        project_id = %project_id,
                        count = ids.len(),
                        "uploaded events"
                    );
                    synced_ids.extend_from_slice(&ids);
                }
                Err(e) => {
                    warn!(
                        task_id = %task_id,
                        project_id = %project_id,
                        error = %e,
                        "failed to upload events, will retry later"
                    );
                }
            }
        }

        let count = synced_ids.len();
        if !synced_ids.is_empty() {
            self.store.mark_synced(&synced_ids)?;
        }
        Ok(count)
    }

    /// Upload all pending events, looping until nothing remains or an error occurs.
    pub async fn flush(&self) {
        loop {
            match self.upload_batch().await {
                Ok(0) => break,
                Ok(n) => {
                    info!(synced = n, "flush: uploaded event batch");
                }
                Err(e) => {
                    warn!(error = %e, "flush: upload_batch error, stopping");
                    break;
                }
            }
        }
    }
}

/// RFC 3986 percent-encode a string (unreserved chars pass through).
fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencoded_basic() {
        assert_eq!(urlencoded("hello-world"), "hello-world");
        assert_eq!(urlencoded("a/b"), "a%2Fb");
        assert_eq!(urlencoded("id with space"), "id%20with%20space");
    }
}
