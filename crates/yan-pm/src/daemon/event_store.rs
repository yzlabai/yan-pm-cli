use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use tracing::{debug, info};

/// Event type enum for agent execution events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    TaskStarted,
    TaskCompleted,
    TaskFailed,
    ToolCall,
    ToolResult,
    StateChange,
    Error,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::TaskStarted => "task_started",
            EventType::TaskCompleted => "task_completed",
            EventType::TaskFailed => "task_failed",
            EventType::ToolCall => "tool_call",
            EventType::ToolResult => "tool_result",
            EventType::StateChange => "state_change",
            EventType::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "task_started" => Some(EventType::TaskStarted),
            "task_completed" => Some(EventType::TaskCompleted),
            "task_failed" => Some(EventType::TaskFailed),
            "tool_call" => Some(EventType::ToolCall),
            "tool_result" => Some(EventType::ToolResult),
            "state_change" => Some(EventType::StateChange),
            "error" => Some(EventType::Error),
            _ => None,
        }
    }
}

/// A persisted event record from the SQLite store.
#[derive(Debug, Clone)]
pub struct Event {
    pub id: i64,
    pub task_id: String,
    pub workspace_id: String,
    pub event_type: String,
    pub payload: String,
    pub created_at: String,
    pub synced_at: Option<String>,
}

/// SQLite-backed event store with WAL mode for concurrent reads.
pub struct EventStore {
    conn: Mutex<Connection>,
}

impl EventStore {
    /// Open (or create) the event store at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create db dir {}", parent.display()))?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("open sqlite db at {}", db_path.display()))?;

        // Enable WAL mode for better concurrent read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;

        // Create table and indexes.
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id      TEXT    NOT NULL,
                workspace_id TEXT    NOT NULL,
                event_type   TEXT    NOT NULL,
                payload      TEXT    NOT NULL,
                created_at   TEXT    NOT NULL,
                synced_at    TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_events_task_id_id
                ON events (task_id, id);

            CREATE INDEX IF NOT EXISTS idx_events_unsynced
                ON events (synced_at) WHERE synced_at IS NULL;

            CREATE INDEX IF NOT EXISTS idx_events_created_at
                ON events (created_at);
            "#,
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Insert a new event and return its row id.
    pub fn insert(
        &self,
        task_id: &str,
        workspace_id: &str,
        event_type: &str,
        payload: &str,
    ) -> Result<i64> {
        let created_at = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (task_id, workspace_id, event_type, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![task_id, workspace_id, event_type, payload, created_at],
        )?;
        let row_id = conn.last_insert_rowid();
        debug!(task_id, event_type, row_id, "event inserted");
        Ok(row_id)
    }

    /// Query events for a task, optionally after a sequence id, ordered by id ASC.
    pub fn query(
        &self,
        task_id: &str,
        after_seq: Option<i64>,
        limit: i64,
    ) -> Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let after = after_seq.unwrap_or(0);
        let mut stmt = conn.prepare(
            "SELECT id, task_id, workspace_id, event_type, payload, created_at, synced_at
             FROM events
             WHERE task_id = ?1 AND id > ?2
             ORDER BY id ASC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![task_id, after, limit], row_to_event)?;
        rows.map(|r| r.map_err(Into::into)).collect()
    }

    /// Fetch events that have not yet been synced to the server.
    pub fn fetch_unsynced(&self, limit: i64) -> Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, workspace_id, event_type, payload, created_at, synced_at
             FROM events
             WHERE synced_at IS NULL
             ORDER BY id ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], row_to_event)?;
        rows.map(|r| r.map_err(Into::into)).collect()
    }

    /// Mark a batch of events as synced.
    pub fn mark_synced(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let synced_at = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        // Use a transaction for batch efficiency.
        let tx = conn.unchecked_transaction()?;
        for &id in ids {
            tx.execute(
                "UPDATE events SET synced_at = ?1 WHERE id = ?2",
                params![synced_at, id],
            )?;
        }
        tx.commit()?;
        debug!(count = ids.len(), "events marked synced");
        Ok(())
    }

    /// Delete synced events older than `days` days. Returns number of deleted rows.
    pub fn compact(&self, days: i64) -> Result<usize> {
        let cutoff = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM events WHERE synced_at IS NOT NULL AND created_at < ?1",
            params![cutoff],
        )?;
        info!(deleted, days, "compacted old synced events");
        Ok(deleted)
    }
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        id: row.get(0)?,
        task_id: row.get(1)?,
        workspace_id: row.get(2)?,
        event_type: row.get(3)?,
        payload: row.get(4)?,
        created_at: row.get(5)?,
        synced_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_store() -> (EventStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = EventStore::open(&dir.path().join("events.db")).unwrap();
        (store, dir)
    }

    #[test]
    fn test_insert_and_query() {
        let (store, _dir) = open_store();
        let id = store
            .insert("task-1", "ws-1", EventType::TaskStarted.as_str(), "{}")
            .unwrap();
        assert!(id > 0);

        let events = store.query("task-1", None, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].task_id, "task-1");
        assert_eq!(events[0].event_type, "task_started");
    }

    #[test]
    fn test_unsynced_and_mark_synced() {
        let (store, _dir) = open_store();
        let id1 = store
            .insert("task-2", "ws-1", EventType::ToolCall.as_str(), "{}")
            .unwrap();
        let id2 = store
            .insert("task-2", "ws-1", EventType::ToolResult.as_str(), "{}")
            .unwrap();

        let unsynced = store.fetch_unsynced(10).unwrap();
        assert_eq!(unsynced.len(), 2);

        store.mark_synced(&[id1]).unwrap();
        let unsynced = store.fetch_unsynced(10).unwrap();
        assert_eq!(unsynced.len(), 1);
        assert_eq!(unsynced[0].id, id2);
    }

    #[test]
    fn test_compact() {
        let (store, _dir) = open_store();
        store
            .insert("task-3", "ws-1", EventType::TaskCompleted.as_str(), "{}")
            .unwrap();
        // Nothing to compact (not synced yet).
        let deleted = store.compact(0).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_event_type_roundtrip() {
        for et in [
            EventType::TaskStarted,
            EventType::TaskCompleted,
            EventType::TaskFailed,
            EventType::ToolCall,
            EventType::ToolResult,
            EventType::StateChange,
            EventType::Error,
        ] {
            let s = et.as_str();
            let parsed = EventType::parse(s).expect("round-trip");
            assert_eq!(parsed.as_str(), s);
        }
    }
}
