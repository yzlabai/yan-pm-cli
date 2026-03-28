# yan-pm-cli 架构升级 Implementation Plan

> **状态：✅ 已完成** — 2026-03-28，14 commits，+1621/-107 行。详见 `docs/devlogs/2026-03-28-architecture-upgrade.md`

**Goal:** 为 yan-pm-cli 添加 WAL 事件持久化、连接状态机、Agent 后端注册表三大能力

**Architecture:** P0 新增 `daemon/event_store.rs` 封装 SQLite 读写；P1 在 `agent/session.rs` 引入 `ConnectionState` 枚举和状态转换；P2 将 `agent/registry.rs` 重构为 trait + 多后端实现。三者通过事件流串联——状态机变更和 Agent 执行事件都写入 event_store。

**Tech Stack:** Rust, rusqlite (bundled), tokio, serde_json, chrono

**Spec:** `docs/plans/2026-03-28-yan-pm-cli-architecture-upgrade.md`

---

## File Structure

| 操作 | 文件 | 职责 |
|------|------|------|
| Create | `crates/yan-pm/src/daemon/event_store.rs` | SQLite WAL 事件存储（insert/query/mark_synced/compact） |
| Create | `crates/yan-pm/src/daemon/event_uploader.rs` | 异步批量上报 + 退出 flush |
| Create | `crates/yan-pm/src/agent/state.rs` | ConnectionState 枚举 + 转换逻辑 |
| Create | `crates/yan-pm/src/agent/backend.rs` | AgentBackend trait 定义 |
| Create | `crates/yan-pm/src/agent/backends/mod.rs` | 后端模块入口 |
| Create | `crates/yan-pm/src/agent/backends/claude.rs` | ClaudeBackend 实现 |
| Create | `crates/yan-pm/src/agent/backends/codex.rs` | CodexBackend 实现 |
| Create | `crates/yan-pm/src/agent/backends/gemini.rs` | GeminiBackend 实现 |
| Modify | `crates/yan-pm/Cargo.toml` | 添加 rusqlite 依赖 |
| Modify | `crates/yan-pm/src/daemon/mod.rs` | 导出 event_store, event_uploader |
| Modify | `crates/yan-pm/src/daemon/process.rs` | 集成 event_store 初始化、compact、event_uploader、退出 flush |
| Modify | `crates/yan-pm/src/agent/mod.rs` | 导出新模块 |
| Modify | `crates/yan-pm/src/agent/session.rs` | 接收 EventStore 写入事件；接收 `&dyn AgentBackend` |
| Modify | `crates/yan-pm/src/agent/registry.rs` | 加入 capabilities、protocol_version、priority 字段 |
| Modify | `crates/yan-pm/src/daemon/auto_runner.rs` | 集成状态机重试逻辑 + 事件写入 + 按能力选 Agent |
| Create | `tests/event_store_test.rs` | event_store 集成测试 |
| Create | `tests/connection_state_test.rs` | 状态机转换测试 |
| Create | `tests/agent_backend_test.rs` | AgentBackend trait 测试 |

---

## Task 1: 添加 rusqlite 依赖

**Files:**
- Modify: `crates/yan-pm/Cargo.toml`

- [x] **Step 1: 添加 rusqlite 到 Cargo.toml**

```toml
# 在 [dependencies] 的 # Config 段后添加：

# SQLite (WAL event store)
rusqlite = { version = "0.34", features = ["bundled"] }
```

- [x] **Step 2: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功，无错误

- [x] **Step 3: Commit**

```bash
git add crates/yan-pm/Cargo.toml Cargo.lock
git commit -m "deps: add rusqlite with bundled feature for WAL event store"
```

---

## Task 2: 实现 EventStore（SQLite WAL 读写）

**Files:**
- Create: `crates/yan-pm/src/daemon/event_store.rs`
- Modify: `crates/yan-pm/src/daemon/mod.rs`

- [x] **Step 1: 创建 event_store.rs 基础结构**

```rust
// crates/yan-pm/src/daemon/event_store.rs

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Event types stored in the WAL
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
            Self::TaskStarted => "task_started",
            Self::TaskCompleted => "task_completed",
            Self::TaskFailed => "task_failed",
            Self::ToolCall => "tool_call",
            Self::ToolResult => "tool_result",
            Self::StateChange => "state_change",
            Self::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "task_started" => Some(Self::TaskStarted),
            "task_completed" => Some(Self::TaskCompleted),
            "task_failed" => Some(Self::TaskFailed),
            "tool_call" => Some(Self::ToolCall),
            "tool_result" => Some(Self::ToolResult),
            "state_change" => Some(Self::StateChange),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// A single persisted event
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

/// SQLite-backed event store with WAL journal mode
pub struct EventStore {
    conn: Mutex<Connection>,
}

impl EventStore {
    /// Open or create the event database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create event store directory")?;
        }

        let conn = Connection::open(db_path)
            .context("Failed to open event store database")?;

        // WAL mode + busy timeout
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;

        // Create table and indexes
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                synced_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_events_task_seq
                ON events (task_id, id);

            CREATE INDEX IF NOT EXISTS idx_events_unsynced
                ON events (synced_at) WHERE synced_at IS NULL;

            CREATE INDEX IF NOT EXISTS idx_events_created
                ON events (created_at);",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Insert a new event. Returns the auto-incremented id (seq).
    pub fn insert(
        &self,
        task_id: &str,
        workspace_id: &str,
        event_type: &str,
        payload: &str,
    ) -> Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (task_id, workspace_id, event_type, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![task_id, workspace_id, event_type, payload, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Query events for a task, optionally after a given seq, with limit.
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
        let rows = stmt.query_map(params![task_id, after, limit], |row| {
            Ok(Event {
                id: row.get(0)?,
                task_id: row.get(1)?,
                workspace_id: row.get(2)?,
                event_type: row.get(3)?,
                payload: row.get(4)?,
                created_at: row.get(5)?,
                synced_at: row.get(6)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Fetch unsynced events (for batch upload). Returns up to `limit` events.
    pub fn fetch_unsynced(&self, limit: i64) -> Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, workspace_id, event_type, payload, created_at, synced_at
             FROM events
             WHERE synced_at IS NULL
             ORDER BY id ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(Event {
                id: row.get(0)?,
                task_id: row.get(1)?,
                workspace_id: row.get(2)?,
                event_type: row.get(3)?,
                payload: row.get(4)?,
                created_at: row.get(5)?,
                synced_at: row.get(6)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Mark events as synced by their IDs.
    pub fn mark_synced(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        // Use a transaction for batch update
        let tx = conn.unchecked_transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE events SET synced_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Delete synced events older than `days` and vacuum.
    pub fn compact(&self, days: i64) -> Result<usize> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM events WHERE synced_at IS NOT NULL AND created_at < ?1",
            params![cutoff],
        )?;
        if deleted > 0 {
            // VACUUM cannot run inside a transaction in WAL mode,
            // but PRAGMA incremental_vacuum works if auto_vacuum is enabled.
            // For simplicity, just let SQLite reuse pages naturally.
            tracing::info!("EventStore: compacted {deleted} old events");
        }
        Ok(deleted)
    }
}
```

- [x] **Step 2: 注册模块到 daemon/mod.rs**

在 `crates/yan-pm/src/daemon/mod.rs` 添加：

```rust
pub mod event_store;
```

- [x] **Step 3: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 4: Commit**

```bash
git add crates/yan-pm/src/daemon/event_store.rs crates/yan-pm/src/daemon/mod.rs
git commit -m "feat(P0): add EventStore with SQLite WAL for event persistence"
```

---

## Task 3: EventStore 测试

**Files:**
- Create: `crates/yan-pm/tests/event_store_test.rs`

- [x] **Step 1: 写测试**

```rust
// crates/yan-pm/tests/event_store_test.rs

use tempfile::TempDir;

// We test via the public API of EventStore
// Since EventStore is in yan_pm_cli::daemon::event_store, import it
use yan_pm_cli::daemon::event_store::EventStore;

#[test]
fn test_insert_and_query() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test_events.db");
    let store = EventStore::open(&db_path).unwrap();

    let id1 = store
        .insert("task-1", "ws-1", "task_started", r#"{"msg":"hello"}"#)
        .unwrap();
    let id2 = store
        .insert("task-1", "ws-1", "tool_call", r#"{"tool":"read"}"#)
        .unwrap();
    let _id3 = store
        .insert("task-2", "ws-1", "task_started", r#"{}"#)
        .unwrap();

    assert!(id2 > id1);

    // Query all events for task-1
    let events = store.query("task-1", None, 100).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, "task_started");
    assert_eq!(events[1].event_type, "tool_call");

    // Query after seq
    let events = store.query("task-1", Some(id1), 100).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, id2);

    // Query task-2
    let events = store.query("task-2", None, 100).unwrap();
    assert_eq!(events.len(), 1);
}

#[test]
fn test_fetch_unsynced_and_mark_synced() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test_events.db");
    let store = EventStore::open(&db_path).unwrap();

    let id1 = store.insert("t1", "ws-1", "task_started", "{}").unwrap();
    let id2 = store.insert("t1", "ws-1", "tool_call", "{}").unwrap();
    let id3 = store.insert("t1", "ws-1", "task_completed", "{}").unwrap();

    // All unsynced
    let unsynced = store.fetch_unsynced(100).unwrap();
    assert_eq!(unsynced.len(), 3);

    // Mark first two as synced
    store.mark_synced(&[id1, id2]).unwrap();

    // Only one unsynced left
    let unsynced = store.fetch_unsynced(100).unwrap();
    assert_eq!(unsynced.len(), 1);
    assert_eq!(unsynced[0].id, id3);
}

#[test]
fn test_compact() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test_events.db");
    let store = EventStore::open(&db_path).unwrap();

    let id1 = store.insert("t1", "ws-1", "task_started", "{}").unwrap();
    store.insert("t1", "ws-1", "tool_call", "{}").unwrap();

    // Mark first as synced
    store.mark_synced(&[id1]).unwrap();

    // Compact with 0 days (delete all synced events)
    let deleted = store.compact(0).unwrap();
    assert_eq!(deleted, 1);

    // Remaining: 1 unsynced event
    let all = store.query("t1", None, 100).unwrap();
    assert_eq!(all.len(), 1);
}

#[test]
fn test_empty_operations() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test_events.db");
    let store = EventStore::open(&db_path).unwrap();

    // Query on empty store
    let events = store.query("nonexistent", None, 100).unwrap();
    assert!(events.is_empty());

    // Mark synced with empty slice
    store.mark_synced(&[]).unwrap();

    // Compact empty store
    let deleted = store.compact(7).unwrap();
    assert_eq!(deleted, 0);
}
```

- [x] **Step 2: 确认 daemon 模块的可见性**

检查 `crates/yan-pm/src/daemon/mod.rs` 和 `crates/yan-pm/src/main.rs`，确保 `daemon` 模块和 `event_store` 是 `pub` 的，以便集成测试可以访问。如果 `main.rs` 中 daemon 模块不是 pub，需要将其改为 `pub mod daemon;`。

- [x] **Step 3: 运行测试**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo test --test event_store_test`
Expected: 4 tests passed

- [x] **Step 4: Commit**

```bash
git add crates/yan-pm/tests/event_store_test.rs
git commit -m "test(P0): add EventStore integration tests"
```

---

## Task 4: 实现 EventUploader（异步批量上报）

**Files:**
- Create: `crates/yan-pm/src/daemon/event_uploader.rs`
- Modify: `crates/yan-pm/src/daemon/mod.rs`

- [x] **Step 1: 创建 event_uploader.rs**

```rust
// crates/yan-pm/src/daemon/event_uploader.rs

use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::api::client::ApiClient;

use super::event_store::EventStore;

/// Batch size for each upload round
const UPLOAD_BATCH_SIZE: i64 = 50;

/// Asynchronously uploads unsynced events to the server.
pub struct EventUploader {
    store: Arc<EventStore>,
    client: Arc<ApiClient>,
}

impl EventUploader {
    pub fn new(store: Arc<EventStore>, client: Arc<ApiClient>) -> Self {
        Self { store, client }
    }

    /// Upload one batch of unsynced events. Returns the number of events uploaded.
    pub async fn upload_batch(&self) -> Result<usize> {
        let events = self.store.fetch_unsynced(UPLOAD_BATCH_SIZE)?;
        if events.is_empty() {
            return Ok(0);
        }

        // Group events by (project_id derived from workspace, task_id)
        // For now, we upload per task_id since the API is per-task
        let mut by_task: std::collections::HashMap<String, Vec<&super::event_store::Event>> =
            std::collections::HashMap::new();
        for event in &events {
            by_task.entry(event.task_id.clone()).or_default().push(event);
        }

        let mut synced_ids = Vec::new();

        for (task_id, task_events) in &by_task {
            let payload: Vec<serde_json::Value> = task_events
                .iter()
                .map(|e| {
                    json!({
                        "seq": e.id,
                        "event_type": e.event_type,
                        "payload": serde_json::from_str::<serde_json::Value>(&e.payload)
                            .unwrap_or(json!({})),
                        "created_at": e.created_at,
                    })
                })
                .collect();

            // Derive project_id from workspace — the uploader needs project context.
            // For the initial implementation, we include project_id in the event payload.
            // The caller should ensure the payload contains "project_id" when inserting.
            let project_id = task_events
                .first()
                .and_then(|e| {
                    serde_json::from_str::<serde_json::Value>(&e.payload)
                        .ok()
                        .and_then(|v| v.get("project_id").and_then(|p| p.as_str().map(String::from)))
                })
                .unwrap_or_default();

            if project_id.is_empty() {
                // Can't upload without project_id, but still mark as synced
                // to avoid infinite retry of malformed events
                tracing::warn!("EventUploader: skipping events for task {task_id} (no project_id in payload)");
                for e in task_events {
                    synced_ids.push(e.id);
                }
                continue;
            }

            let body = json!({ "events": payload });
            let path = format!(
                "/projects/{}/tasks/{}/events",
                urlencoded(&project_id),
                urlencoded(task_id)
            );

            match self.client.post_raw(&path, &body).await {
                Ok(_) => {
                    for e in task_events {
                        synced_ids.push(e.id);
                    }
                }
                Err(e) => {
                    tracing::warn!("EventUploader: failed to upload events for task {task_id}: {e}");
                    // Leave unsynced for next retry
                }
            }
        }

        if !synced_ids.is_empty() {
            self.store.mark_synced(&synced_ids)?;
        }

        Ok(synced_ids.len())
    }

    /// Flush all remaining unsynced events (called on daemon shutdown).
    pub async fn flush(&self) {
        loop {
            match self.upload_batch().await {
                Ok(0) => break,
                Ok(n) => tracing::info!("EventUploader: flushed {n} events"),
                Err(e) => {
                    tracing::warn!("EventUploader: flush error: {e}");
                    break;
                }
            }
        }
    }
}

fn urlencoded(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}
```

- [x] **Step 2: 在 ApiClient 添加 post_raw 方法**

在 `crates/yan-pm/src/api/client.rs` 中添加一个公开方法（靠近其他 post 方法处）：

```rust
/// POST request that returns the raw response value (for event upload)
pub async fn post_raw(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value, ApiError> {
    self.post(path, body).await
}
```

- [x] **Step 3: 注册 event_uploader 到 daemon/mod.rs**

```rust
pub mod event_uploader;
```

- [x] **Step 4: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 5: Commit**

```bash
git add crates/yan-pm/src/daemon/event_uploader.rs crates/yan-pm/src/daemon/mod.rs crates/yan-pm/src/api/client.rs
git commit -m "feat(P0): add EventUploader for async batch event upload with flush"
```

---

## Task 5: 集成 EventStore 到 Daemon 主循环

**Files:**
- Modify: `crates/yan-pm/src/daemon/process.rs`

- [x] **Step 1: 在 run_foreground 中初始化 EventStore + EventUploader**

在 `run_foreground()` 函数中，在 `let client = Arc::new(...)` 之后添加：

```rust
use super::event_store::EventStore;
use super::event_uploader::EventUploader;

// Initialize event store
let events_db_path = config::config_dir().join("events.db");
let event_store = Arc::new(
    EventStore::open(&events_db_path)
        .context("Failed to open event store")?
);

// Run startup compact (delete synced events older than 7 days)
if let Err(e) = event_store.compact(7) {
    tracing::warn!("Event store compact on startup failed: {e}");
}

let event_uploader = EventUploader::new(event_store.clone(), client.clone());
```

- [x] **Step 2: 添加事件上报定时器到主循环**

在 main event loop 的常量定义区域添加：

```rust
const EVENT_UPLOAD_INTERVAL: Duration = Duration::from_secs(10);
const COMPACT_INTERVAL: Duration = Duration::from_secs(86400); // 24h
```

在主循环内添加两个新的 interval ticker 和对应的 select arm：

```rust
let mut event_upload_interval = tokio::time::interval(EVENT_UPLOAD_INTERVAL);
let mut compact_interval = tokio::time::interval(COMPACT_INTERVAL);
event_upload_interval.tick().await;
compact_interval.tick().await;
```

在 `tokio::select!` 中添加：

```rust
_ = event_upload_interval.tick() => {
    if let Err(e) = event_uploader.upload_batch().await {
        tracing::warn!("Event upload error: {e}");
    }
}

_ = compact_interval.tick() => {
    if let Err(e) = event_store.compact(7) {
        tracing::warn!("Event compact error: {e}");
    }
}
```

- [x] **Step 3: 在 shutdown 路径添加 flush**

在 `// Cleanup` 注释处，`auto_runner.shutdown()` 之后添加：

```rust
// Flush pending events before exit
event_uploader.flush().await;
```

- [x] **Step 4: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 5: Commit**

```bash
git add crates/yan-pm/src/daemon/process.rs
git commit -m "feat(P0): integrate EventStore + EventUploader into daemon main loop"
```

---

## Task 6: 在 AutoRunner 中写入执行事件

**Files:**
- Modify: `crates/yan-pm/src/daemon/auto_runner.rs`

- [x] **Step 1: 给 AutoRunner 添加 EventStore 字段**

修改 `AutoRunner` struct 和 `new()`:

```rust
use super::event_store::EventStore;

pub struct AutoRunner {
    client: Arc<ApiClient>,
    slots: HashMap<String, RunnerSlot>,
    event_store: Option<Arc<EventStore>>,
}

impl AutoRunner {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            slots: HashMap::new(),
            event_store: None,
        }
    }

    /// Set the event store for event persistence.
    pub fn set_event_store(&mut self, store: Arc<EventStore>) {
        self.event_store = Some(store);
    }
```

- [x] **Step 2: 在任务启动时写入 task_started 事件**

在 `check_slot()` 方法中，`slot.running.push(RunningTask { ... })` 之前添加：

```rust
// Record task_started event
if let Some(store) = &self.event_store {
    let payload = serde_json::json!({
        "project_id": project_id,
        "agent": agent_name,
        "title": title,
    });
    if let Err(e) = store.insert(
        &task_id,
        ws_id.as_deref().unwrap_or(""),
        "task_started",
        &payload.to_string(),
    ) {
        tracing::warn!("Failed to record task_started event: {e}");
    }
}
```

- [x] **Step 3: 在任务完成时写入 task_completed / task_failed 事件**

在 `collect_slot_completed()` 方法中，`let new_status = ...` 判断之后、更新 task status 之前添加：

```rust
// Record completion event
if let Some(store) = &self.event_store {
    let event_type = if result.success { "task_completed" } else { "task_failed" };
    let payload = serde_json::json!({
        "project_id": task.project_id,
        "exit_code": result.exit_code,
        "cost_usd": result.cost_usd,
        "has_tool_calls": false, // Will be enriched in P1
    });
    if let Err(e) = store.insert(
        &task.task_id,
        task.workspace_id.as_deref().unwrap_or(""),
        event_type,
        &payload.to_string(),
    ) {
        tracing::warn!("Failed to record {event_type} event: {e}");
    }
}
```

- [x] **Step 4: 在 daemon/process.rs 的 run_foreground 中设置 event_store**

在 `auto_runner.set_workspace(...)` 循环之后添加：

```rust
auto_runner.set_event_store(event_store.clone());
```

- [x] **Step 5: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 6: Commit**

```bash
git add crates/yan-pm/src/daemon/auto_runner.rs crates/yan-pm/src/daemon/process.rs
git commit -m "feat(P0): write task_started/completed/failed events from AutoRunner"
```

---

## Task 7: 实现 ConnectionState 状态机

**Files:**
- Create: `crates/yan-pm/src/agent/state.rs`
- Modify: `crates/yan-pm/src/agent/mod.rs`

- [x] **Step 1: 创建 state.rs**

```rust
// crates/yan-pm/src/agent/state.rs

use serde::{Deserialize, Serialize};

/// Connection lifecycle states for an agent process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Registered but not started
    Idle,
    /// Process spawned, ACP handshake in progress
    Connecting,
    /// ACP initialized, can accept prompts
    Ready,
    /// Connection error, may retry
    Error,
    /// Terminal state, cleaned up
    Stopped,
}

/// Structured error codes for agent failures
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentErrorCode {
    AgentNotFound,
    AgentSpawnFailed,
    AgentTimeout,
    AgentCrashed,
    ProtocolError,
}

impl AgentErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentNotFound => "AGENT_NOT_FOUND",
            Self::AgentSpawnFailed => "AGENT_SPAWN_FAILED",
            Self::AgentTimeout => "AGENT_TIMEOUT",
            Self::AgentCrashed => "AGENT_CRASHED",
            Self::ProtocolError => "PROTOCOL_ERROR",
        }
    }
}

/// Result of a state transition
#[derive(Debug)]
pub struct StateTransition {
    pub from: ConnectionState,
    pub to: ConnectionState,
    pub error_code: Option<AgentErrorCode>,
}

impl ConnectionState {
    /// Attempt to transition to a new state. Returns None if the transition is invalid.
    pub fn transition(
        &self,
        to: ConnectionState,
        error_code: Option<AgentErrorCode>,
    ) -> Option<StateTransition> {
        let valid = match (self, &to) {
            // Normal flow
            (Self::Idle, Self::Connecting) => true,
            (Self::Connecting, Self::Ready) => true,
            (Self::Ready, Self::Stopped) => true,

            // Error transitions
            (Self::Connecting, Self::Error) => true,
            (Self::Ready, Self::Error) => true,

            // Recovery
            (Self::Error, Self::Connecting) => true, // retry/reconnect
            (Self::Error, Self::Stopped) => true,    // give up

            _ => false,
        };

        if valid {
            Some(StateTransition {
                from: *self,
                to,
                error_code,
            })
        } else {
            None
        }
    }
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Connecting => write!(f, "connecting"),
            Self::Ready => write!(f, "ready"),
            Self::Error => write!(f, "error"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}
```

- [x] **Step 2: 注册模块**

在 `crates/yan-pm/src/agent/mod.rs` 添加：

```rust
pub mod state;
pub use state::{AgentErrorCode, ConnectionState};
```

- [x] **Step 3: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 4: Commit**

```bash
git add crates/yan-pm/src/agent/state.rs crates/yan-pm/src/agent/mod.rs
git commit -m "feat(P1): add ConnectionState state machine with error codes"
```

---

## Task 8: ConnectionState 测试

**Files:**
- Create: `crates/yan-pm/tests/connection_state_test.rs`

- [x] **Step 1: 写状态转换测试**

```rust
// crates/yan-pm/tests/connection_state_test.rs

use yan_pm_cli::agent::state::{AgentErrorCode, ConnectionState};

#[test]
fn test_normal_lifecycle() {
    let state = ConnectionState::Idle;

    // idle -> connecting
    let t = state.transition(ConnectionState::Connecting, None).unwrap();
    assert_eq!(t.from, ConnectionState::Idle);
    assert_eq!(t.to, ConnectionState::Connecting);
    assert!(t.error_code.is_none());

    // connecting -> ready
    let t = ConnectionState::Connecting
        .transition(ConnectionState::Ready, None)
        .unwrap();
    assert_eq!(t.to, ConnectionState::Ready);

    // ready -> stopped (normal completion)
    let t = ConnectionState::Ready
        .transition(ConnectionState::Stopped, None)
        .unwrap();
    assert_eq!(t.to, ConnectionState::Stopped);
}

#[test]
fn test_error_and_retry() {
    // connecting -> error (timeout)
    let t = ConnectionState::Connecting
        .transition(
            ConnectionState::Error,
            Some(AgentErrorCode::AgentTimeout),
        )
        .unwrap();
    assert_eq!(t.to, ConnectionState::Error);
    assert_eq!(t.error_code, Some(AgentErrorCode::AgentTimeout));

    // error -> connecting (retry)
    let t = ConnectionState::Error
        .transition(ConnectionState::Connecting, None)
        .unwrap();
    assert_eq!(t.to, ConnectionState::Connecting);

    // error -> stopped (give up)
    let t = ConnectionState::Error
        .transition(ConnectionState::Stopped, None)
        .unwrap();
    assert_eq!(t.to, ConnectionState::Stopped);
}

#[test]
fn test_ready_to_error() {
    // ready -> error (connection lost)
    let t = ConnectionState::Ready
        .transition(
            ConnectionState::Error,
            Some(AgentErrorCode::AgentCrashed),
        )
        .unwrap();
    assert_eq!(t.to, ConnectionState::Error);
    assert_eq!(t.error_code, Some(AgentErrorCode::AgentCrashed));
}

#[test]
fn test_invalid_transitions() {
    // idle -> ready (skip connecting)
    assert!(ConnectionState::Idle
        .transition(ConnectionState::Ready, None)
        .is_none());

    // idle -> error
    assert!(ConnectionState::Idle
        .transition(ConnectionState::Error, None)
        .is_none());

    // stopped -> anything
    assert!(ConnectionState::Stopped
        .transition(ConnectionState::Idle, None)
        .is_none());
    assert!(ConnectionState::Stopped
        .transition(ConnectionState::Connecting, None)
        .is_none());

    // ready -> idle
    assert!(ConnectionState::Ready
        .transition(ConnectionState::Idle, None)
        .is_none());
}
```

- [x] **Step 2: 运行测试**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo test --test connection_state_test`
Expected: 4 tests passed

- [x] **Step 3: Commit**

```bash
git add crates/yan-pm/tests/connection_state_test.rs
git commit -m "test(P1): add ConnectionState transition tests"
```

---

## Task 9: 在 session.rs 中使用 ConnectionState 并写入状态变更事件

**Files:**
- Modify: `crates/yan-pm/src/agent/session.rs`

- [x] **Step 1: 给 execute_agent 添加 EventStore 参数和状态跟踪**

修改 `execute_agent` 签名，添加可选的 event_store 和 task 元信息：

```rust
use super::state::{AgentErrorCode, ConnectionState};
use crate::daemon::event_store::EventStore;
use std::sync::Arc;

/// Context for event recording during agent execution
pub struct ExecutionContext {
    pub task_id: String,
    pub workspace_id: String,
    pub project_id: String,
    pub event_store: Arc<EventStore>,
}

pub async fn execute_agent(
    agent: &AgentDefinition,
    options: AgentOptions,
    ctx: Option<&ExecutionContext>,
) -> Result<AgentResult> {
```

- [x] **Step 2: 添加状态转换 helper 和事件写入**

在 `execute_agent` 函数体开头添加：

```rust
let mut conn_state = ConnectionState::Idle;

// Helper to record state changes
let record_state_change = |from: ConnectionState, to: ConnectionState, error_code: Option<&AgentErrorCode>, ctx: Option<&ExecutionContext>| {
    if let Some(ctx) = ctx {
        let payload = serde_json::json!({
            "project_id": ctx.project_id,
            "from": from.to_string(),
            "to": to.to_string(),
            "error_code": error_code.map(|e| e.as_str()),
        });
        let _ = ctx.event_store.insert(
            &ctx.task_id,
            &ctx.workspace_id,
            "state_change",
            &payload.to_string(),
        );
    }
};
```

- [x] **Step 3: 在关键位置插入状态转换**

1. 在 `is_command_available` 检查失败处：

```rust
if !is_command_available(&agent.command).await {
    record_state_change(conn_state, ConnectionState::Stopped, Some(&AgentErrorCode::AgentNotFound), ctx);
    return Ok(AgentResult { ... });
}
```

2. 在 `child.spawn()` 之前：

```rust
// Transition: idle -> connecting
conn_state = ConnectionState::Connecting;
record_state_change(ConnectionState::Idle, conn_state, None, ctx);
```

3. 在 spawn 失败处（将 `?` 改为 match）：

```rust
let mut child = match tokio::process::Command::new(&agent.command)
    ...
    .spawn()
{
    Ok(c) => c,
    Err(e) => {
        record_state_change(conn_state, ConnectionState::Stopped, Some(&AgentErrorCode::AgentSpawnFailed), ctx);
        return Err(e.into());
    }
};
```

4. 在 ACP initialize 成功后：

```rust
// Transition: connecting -> ready
conn_state = ConnectionState::Ready;
record_state_change(ConnectionState::Connecting, ConnectionState::Ready, None, ctx);
```

（此处需将 `ctx` 传入 ACP future 闭包——因为闭包 move 了值，改为传入 `ctx` 中的 `Arc<EventStore>` + task_id + workspace_id + project_id 的 clone）

5. 在超时处：

```rust
Err(_) => {
    record_state_change(conn_state, ConnectionState::Stopped, Some(&AgentErrorCode::AgentTimeout), ctx);
    ...
}
```

6. 在正常完成处：

```rust
record_state_change(conn_state, ConnectionState::Stopped, None, ctx);
```

7. 在 ACP 错误处：

```rust
Err(e) => {
    record_state_change(conn_state, ConnectionState::Error, Some(&AgentErrorCode::ProtocolError), ctx);
    ...
}
```

- [x] **Step 4: 记录 tool_call 事件**

在 `YanPmAcpClient::session_notification` 的 `ToolCall` arm 中，添加事件记录。由于 `YanPmAcpClient` 需要访问 event_store，给它加一个字段：

```rust
struct YanPmAcpClient {
    policy: PermissionPolicy,
    output: Arc<Mutex<String>>,
    verbose: bool,
    cancelled: Arc<AtomicBool>,
    // Event recording context (optional)
    event_ctx: Option<EventRecordCtx>,
}

struct EventRecordCtx {
    task_id: String,
    workspace_id: String,
    project_id: String,
    event_store: Arc<EventStore>,
}
```

在 `ToolCall` arm 中添加：

```rust
acp::SessionUpdate::ToolCall(tc) => {
    if let Some(ectx) = &self.event_ctx {
        let payload = serde_json::json!({
            "project_id": ectx.project_id,
            "tool": tc.title,
        });
        let _ = ectx.event_store.insert(
            &ectx.task_id,
            &ectx.workspace_id,
            "tool_call",
            &payload.to_string(),
        );
    }
    // ... existing verbose logging
}
```

- [x] **Step 5: 更新 auto_runner.rs 中的 execute_agent 调用**

在 `auto_runner.rs` 的 agent spawn thread 中，构造 `ExecutionContext` 并传入：

```rust
let event_store_clone = self.event_store.clone();
// ... inside std::thread::spawn:
let exec_ctx = event_store_clone.map(|store| agent::session::ExecutionContext {
    task_id: task_id_clone.clone(),
    workspace_id: ws_id_clone.unwrap_or_default(),
    project_id: project_id_clone.clone(),
    event_store: store,
});
match agent::execute_agent(
    &agent_clone,
    AgentOptions { ... },
    exec_ctx.as_ref(),
).await
```

- [x] **Step 6: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 7: Commit**

```bash
git add crates/yan-pm/src/agent/session.rs crates/yan-pm/src/daemon/auto_runner.rs
git commit -m "feat(P1): integrate ConnectionState + event recording into agent session"
```

---

## Task 10: AutoRunner 重试逻辑（基于状态机）

**Files:**
- Modify: `crates/yan-pm/src/daemon/auto_runner.rs`

- [x] **Step 1: 给 RunningTask 添加重试计数和 tool_call 追踪**

```rust
struct RunningTask {
    task_id: String,
    project_id: String,
    workspace_id: Option<String>,
    thread_handle: Option<std::thread::JoinHandle<AgentResult>>,
    heartbeat_running: Arc<AtomicBool>,
    heartbeat_handle: JoinHandle<()>,
    started_at: chrono::DateTime<chrono::Utc>,
    retry_count: u32,
}
```

在 `RunnerSlot` 中新增：

```rust
/// Tasks pending retry with their retry count
pending_retry: Vec<(String, u32, chrono::DateTime<chrono::Utc>)>, // (task_id, retry_count, retry_after)
```

初始化时设 `pending_retry: Vec::new()`。

- [x] **Step 2: 修改任务完成逻辑**

在 `collect_slot_completed` 中，替换当前的 `new_status` 判断逻辑：

```rust
const MAX_RETRIES: u32 = 2;

if result.success {
    // Success path — mark Done
    let _ = self.client.update_task(..., TaskStatus::Done, ...).await;
    // ... existing archive logic
} else {
    // Check if task had side effects (tool_call events)
    let has_side_effects = self.event_store.as_ref().map_or(false, |store| {
        store.query(&task.task_id, None, 1)
            .unwrap_or_default()
            .iter()
            .any(|e| e.event_type == "tool_call")
    });

    if has_side_effects || task.retry_count >= MAX_RETRIES {
        // Has side effects or retries exhausted → Failed, need manual intervention
        slot.failed_task_ids.insert(task.task_id.clone());
        // Keep as Todo but add failure comment (existing behavior)
        // If has side effects, the comment should indicate manual review needed
        let status_note = if has_side_effects {
            "失败（已有副作用，需人工检查）"
        } else {
            "失败（重试次数耗尽）"
        };
        // ... update status + comment with status_note
    } else {
        // No side effects, can retry
        let retry_count = task.retry_count + 1;
        let delay_secs = if retry_count == 1 { 5 } else { 15 }; // exponential: 5s, 15s
        let retry_after = chrono::Utc::now() + chrono::Duration::seconds(delay_secs);
        slot.pending_retry.push((task.task_id.clone(), retry_count, retry_after));
        tracing::info!(
            "AutoRunner: task {} will retry ({}/{}), after {}s",
            task.task_id, retry_count, MAX_RETRIES, delay_secs
        );
    }
}
```

- [x] **Step 3: 在 check_slot 中处理 pending_retry**

在 `check_slot()` 的任务选择逻辑之前，检查是否有到期的 retry 任务：

```rust
// Check for pending retries first
let now = chrono::Utc::now();
let ready_retries: Vec<_> = slot.pending_retry
    .iter()
    .filter(|(_, _, retry_after)| now >= *retry_after)
    .cloned()
    .collect();

if let Some((retry_task_id, retry_count, _)) = ready_retries.first() {
    // Remove from pending
    slot.pending_retry.retain(|(id, _, _)| id != retry_task_id);
    // Re-run this task with incremented retry_count
    // (reuse the existing task launch logic, passing retry_count)
    // ...
    return Ok(());
}
```

- [x] **Step 4: 在 RunningTask 初始化时设置 retry_count**

```rust
slot.running.push(RunningTask {
    // ... existing fields
    retry_count: 0, // or from retry context
});
```

- [x] **Step 5: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 6: Commit**

```bash
git add crates/yan-pm/src/daemon/auto_runner.rs
git commit -m "feat(P1): add retry logic with side-effect detection and exponential backoff"
```

---

## Task 11: AgentBackend trait 定义

**Files:**
- Create: `crates/yan-pm/src/agent/backend.rs`
- Modify: `crates/yan-pm/src/agent/mod.rs`

- [x] **Step 1: 创建 backend.rs**

```rust
// crates/yan-pm/src/agent/backend.rs

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Capability declarations for an agent backend
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub supports_images: bool,
    pub supports_mcp: bool,
    pub supports_worktree: bool,
    pub max_context_tokens: u32,
}

/// The AgentBackend trait abstracts different AI coding tools
pub trait AgentBackend: Send + Sync {
    /// Identifier name, e.g. "claude"
    fn name(&self) -> &str;

    /// Executable command path/name
    fn command(&self) -> &str;

    /// ACP startup arguments
    fn acp_args(&self) -> Vec<String>;

    /// Extra environment variables
    fn env_vars(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    /// Capability declarations
    fn capabilities(&self) -> AgentCapabilities;

    /// ACP protocol version supported
    fn protocol_version(&self) -> &str {
        "v1"
    }

    /// Check if the agent CLI is installed on this machine
    fn is_available_cmd(&self) -> &str {
        self.command()
    }

    /// Build the prompt for a given task
    fn build_prompt(&self, title: &str, description: &str) -> String {
        format!(
            "# 任务: {title}\n\n## 描述\n\n{description}\n\n## 要求\n\n\
             1. 在当前代码库中实现所需的变更\n\
             2. 确保代码通过类型检查\n\
             3. 不要修改与任务无关的代码\n\
             4. 完成后简要总结你做了什么"
        )
    }

    /// Optional human-readable description
    fn description(&self) -> Option<&str> {
        None
    }

    /// Priority for auto-selection (lower = higher priority)
    fn priority(&self) -> u32 {
        100
    }
}

/// Convert a trait object back to AgentDefinition for compatibility
impl dyn AgentBackend {
    pub fn to_definition(&self) -> super::registry::AgentDefinition {
        super::registry::AgentDefinition {
            name: self.name().to_string(),
            command: self.command().to_string(),
            acp_args: self.acp_args(),
            env: self.env_vars(),
            description: self.description().map(String::from),
        }
    }
}
```

- [x] **Step 2: 注册模块**

在 `crates/yan-pm/src/agent/mod.rs` 添加：

```rust
pub mod backend;
pub use backend::{AgentBackend, AgentCapabilities};
```

- [x] **Step 3: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 4: Commit**

```bash
git add crates/yan-pm/src/agent/backend.rs crates/yan-pm/src/agent/mod.rs
git commit -m "feat(P2): define AgentBackend trait with capabilities and protocol version"
```

---

## Task 12: 内置 Backend 实现

**Files:**
- Create: `crates/yan-pm/src/agent/backends/mod.rs`
- Create: `crates/yan-pm/src/agent/backends/claude.rs`
- Create: `crates/yan-pm/src/agent/backends/codex.rs`
- Create: `crates/yan-pm/src/agent/backends/gemini.rs`
- Modify: `crates/yan-pm/src/agent/mod.rs`

- [x] **Step 1: 创建 backends/claude.rs**

```rust
// crates/yan-pm/src/agent/backends/claude.rs

use std::collections::HashMap;

use super::super::backend::{AgentBackend, AgentCapabilities};

pub struct ClaudeBackend;

impl AgentBackend for ClaudeBackend {
    fn name(&self) -> &str {
        "claude"
    }

    fn command(&self) -> &str {
        "claude"
    }

    fn acp_args(&self) -> Vec<String> {
        vec!["--acp".into()]
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_images: true,
            supports_mcp: true,
            supports_worktree: true,
            max_context_tokens: 200_000,
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Anthropic Claude Code")
    }

    fn priority(&self) -> u32 {
        1
    }
}
```

- [x] **Step 2: 创建 backends/codex.rs**

```rust
// crates/yan-pm/src/agent/backends/codex.rs

use std::collections::HashMap;

use super::super::backend::{AgentBackend, AgentCapabilities};

pub struct CodexBackend;

impl AgentBackend for CodexBackend {
    fn name(&self) -> &str {
        "codex"
    }

    fn command(&self) -> &str {
        "codex"
    }

    fn acp_args(&self) -> Vec<String> {
        vec!["--acp".into()]
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_images: false,
            supports_mcp: false,
            supports_worktree: true,
            max_context_tokens: 200_000,
        }
    }

    fn description(&self) -> Option<&str> {
        Some("OpenAI Codex CLI")
    }

    fn priority(&self) -> u32 {
        2
    }
}
```

- [x] **Step 3: 创建 backends/gemini.rs**

```rust
// crates/yan-pm/src/agent/backends/gemini.rs

use std::collections::HashMap;

use super::super::backend::{AgentBackend, AgentCapabilities};

pub struct GeminiBackend;

impl AgentBackend for GeminiBackend {
    fn name(&self) -> &str {
        "gemini"
    }

    fn command(&self) -> &str {
        "gemini"
    }

    fn acp_args(&self) -> Vec<String> {
        vec!["--experimental-acp".into()]
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_images: true,
            supports_mcp: true,
            supports_worktree: false,
            max_context_tokens: 1_000_000,
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Google Gemini CLI")
    }

    fn priority(&self) -> u32 {
        3
    }
}
```

- [x] **Step 4: 创建 backends/mod.rs**

```rust
// crates/yan-pm/src/agent/backends/mod.rs

pub mod claude;
pub mod codex;
pub mod gemini;

pub use claude::ClaudeBackend;
pub use codex::CodexBackend;
pub use gemini::GeminiBackend;

use super::backend::AgentBackend;

/// Return all built-in backends
pub fn builtin_backends() -> Vec<Box<dyn AgentBackend>> {
    vec![
        Box::new(ClaudeBackend),
        Box::new(CodexBackend),
        Box::new(GeminiBackend),
    ]
}
```

- [x] **Step 5: 注册 backends 模块**

在 `crates/yan-pm/src/agent/mod.rs` 添加：

```rust
pub mod backends;
```

- [x] **Step 6: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 7: Commit**

```bash
git add crates/yan-pm/src/agent/backends/
git commit -m "feat(P2): implement Claude/Codex/Gemini backends with capability declarations"
```

---

## Task 13: 更新 registry.rs 支持 AgentBackend 发现

**Files:**
- Modify: `crates/yan-pm/src/agent/registry.rs`

- [x] **Step 1: 添加基于 AgentBackend 的发现函数**

在 `registry.rs` 末尾添加：

```rust
use super::backend::AgentBackend;
use super::backends::builtin_backends;

/// Find the best available backend by name.
/// Checks built-in backends, returns trait object.
pub fn find_backend(name: &str) -> Option<Box<dyn AgentBackend>> {
    builtin_backends().into_iter().find(|b| b.name() == name)
}

/// List all backends sorted by priority, optionally filtered by availability.
pub async fn list_backends_by_priority() -> Vec<Box<dyn AgentBackend>> {
    let mut backends = builtin_backends();
    // Sort by priority (lower = higher)
    backends.sort_by_key(|b| b.priority());
    backends
}

/// Find the best available backend that satisfies the given capability requirements.
pub async fn find_capable_backend(
    needs_images: bool,
    needs_mcp: bool,
    needs_worktree: bool,
) -> Option<Box<dyn AgentBackend>> {
    let mut backends = builtin_backends();
    backends.sort_by_key(|b| b.priority());

    for backend in backends {
        let caps = backend.capabilities();
        if needs_images && !caps.supports_images {
            continue;
        }
        if needs_mcp && !caps.supports_mcp {
            continue;
        }
        if needs_worktree && !caps.supports_worktree {
            continue;
        }
        if is_command_available(backend.command()).await {
            return Some(backend);
        }
    }
    None
}
```

- [x] **Step 2: 验证编译**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check`
Expected: 编译成功

- [x] **Step 3: Commit**

```bash
git add crates/yan-pm/src/agent/registry.rs
git commit -m "feat(P2): add backend discovery with capability matching and priority"
```

---

## Task 14: AgentBackend 测试

**Files:**
- Create: `crates/yan-pm/tests/agent_backend_test.rs`

- [x] **Step 1: 写 backend trait 测试**

```rust
// crates/yan-pm/tests/agent_backend_test.rs

use yan_pm_cli::agent::backend::{AgentBackend, AgentCapabilities};
use yan_pm_cli::agent::backends::{ClaudeBackend, CodexBackend, GeminiBackend, builtin_backends};

#[test]
fn test_claude_backend() {
    let b = ClaudeBackend;
    assert_eq!(b.name(), "claude");
    assert_eq!(b.command(), "claude");
    assert_eq!(b.acp_args(), vec!["--acp"]);
    assert!(b.capabilities().supports_images);
    assert!(b.capabilities().supports_mcp);
    assert!(b.capabilities().supports_worktree);
    assert_eq!(b.priority(), 1);
    assert_eq!(b.protocol_version(), "v1");
}

#[test]
fn test_codex_backend() {
    let b = CodexBackend;
    assert_eq!(b.name(), "codex");
    assert!(!b.capabilities().supports_images);
    assert!(!b.capabilities().supports_mcp);
    assert_eq!(b.priority(), 2);
}

#[test]
fn test_gemini_backend() {
    let b = GeminiBackend;
    assert_eq!(b.name(), "gemini");
    assert_eq!(b.acp_args(), vec!["--experimental-acp"]);
    assert!(b.capabilities().supports_images);
    assert_eq!(b.capabilities().max_context_tokens, 1_000_000);
    assert_eq!(b.priority(), 3);
}

#[test]
fn test_builtin_backends_sorted_by_priority() {
    let mut backends = builtin_backends();
    backends.sort_by_key(|b| b.priority());
    assert_eq!(backends[0].name(), "claude");
    assert_eq!(backends[1].name(), "codex");
    assert_eq!(backends[2].name(), "gemini");
}

#[test]
fn test_build_prompt() {
    let b = ClaudeBackend;
    let prompt = b.build_prompt("Fix bug", "The login page crashes");
    assert!(prompt.contains("Fix bug"));
    assert!(prompt.contains("The login page crashes"));
    assert!(prompt.contains("任务"));
}

#[test]
fn test_to_definition() {
    let b: Box<dyn AgentBackend> = Box::new(ClaudeBackend);
    let def = b.to_definition();
    assert_eq!(def.name, "claude");
    assert_eq!(def.command, "claude");
    assert_eq!(def.acp_args, vec!["--acp"]);
}
```

- [x] **Step 2: 运行测试**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo test --test agent_backend_test`
Expected: 6 tests passed

- [x] **Step 3: Commit**

```bash
git add crates/yan-pm/tests/agent_backend_test.rs
git commit -m "test(P2): add AgentBackend trait and backends tests"
```

---

## Task 15: 全量测试验证

**Files:** 无新文件

- [x] **Step 1: 运行全部测试**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo test`
Expected: 所有测试通过

- [x] **Step 2: 运行 clippy 检查**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo clippy -- -D warnings`
Expected: 无 warning

- [x] **Step 3: 确认构建**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo build`
Expected: 构建成功

- [x] **Step 4: Commit（如有 clippy 修复）**

```bash
git add -A
git commit -m "chore: fix clippy warnings from architecture upgrade"
```
