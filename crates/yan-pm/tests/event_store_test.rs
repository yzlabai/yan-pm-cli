use tempfile::TempDir;
use yan_pm_cli::daemon::event_store::{EventStore, EventType};

fn open_store() -> (EventStore, TempDir) {
    let dir = TempDir::new().unwrap();
    let store = EventStore::open(&dir.path().join("events.db")).unwrap();
    (store, dir)
}

#[test]
fn test_insert_and_query() {
    let (store, _dir) = open_store();

    // Insert 2 events for task-1, 1 for task-2
    let id1 = store
        .insert("task-1", "ws-1", EventType::TaskStarted.as_str(), r#"{"step":1}"#)
        .unwrap();
    let id2 = store
        .insert("task-1", "ws-1", EventType::ToolCall.as_str(), r#"{"step":2}"#)
        .unwrap();
    store
        .insert("task-2", "ws-1", EventType::TaskStarted.as_str(), r#"{"step":3}"#)
        .unwrap();

    // task-1 should return 2 events
    let events = store.query("task-1", None, 100).unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.task_id == "task-1"));

    // task-2 should return 1 event
    let events2 = store.query("task-2", None, 100).unwrap();
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].task_id, "task-2");

    // after_seq filtering: after id1 should return only id2
    let after = store.query("task-1", Some(id1), 100).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, id2);

    // after last id → empty
    let empty = store.query("task-1", Some(id2), 100).unwrap();
    assert_eq!(empty.len(), 0);
}

#[test]
fn test_fetch_unsynced_and_mark_synced() {
    let (store, _dir) = open_store();

    let id1 = store
        .insert("task-1", "ws-1", EventType::TaskStarted.as_str(), "{}")
        .unwrap();
    let id2 = store
        .insert("task-1", "ws-1", EventType::ToolCall.as_str(), "{}")
        .unwrap();
    let id3 = store
        .insert("task-2", "ws-1", EventType::TaskCompleted.as_str(), "{}")
        .unwrap();

    // All 3 events should be unsynced initially
    let unsynced = store.fetch_unsynced(100).unwrap();
    assert_eq!(unsynced.len(), 3);

    // Mark 2 as synced
    store.mark_synced(&[id1, id2]).unwrap();

    // Only 1 unsynced should remain
    let unsynced = store.fetch_unsynced(100).unwrap();
    assert_eq!(unsynced.len(), 1);
    assert_eq!(unsynced[0].id, id3);

    // Synced events should have synced_at set
    let all = store.query("task-1", None, 100).unwrap();
    for e in &all {
        assert!(e.synced_at.is_some(), "event {} should be synced", e.id);
    }
}

#[test]
fn test_compact() {
    let (store, _dir) = open_store();

    let id1 = store
        .insert("task-1", "ws-1", EventType::TaskStarted.as_str(), "{}")
        .unwrap();
    let id2 = store
        .insert("task-1", "ws-1", EventType::TaskCompleted.as_str(), "{}")
        .unwrap();

    // Mark id1 as synced
    store.mark_synced(&[id1]).unwrap();

    // Compact with days=0 (delete all synced events older than now)
    let deleted = store.compact(0).unwrap();
    assert_eq!(deleted, 1, "should delete 1 synced event");

    // Only the unsynced event (id2) should remain
    let remaining = store.query("task-1", None, 100).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, id2);
    assert!(remaining[0].synced_at.is_none());
}

#[test]
fn test_empty_operations() {
    let (store, _dir) = open_store();

    // Query on empty store
    let events = store.query("task-none", None, 100).unwrap();
    assert_eq!(events.len(), 0);

    // fetch_unsynced on empty store
    let unsynced = store.fetch_unsynced(100).unwrap();
    assert_eq!(unsynced.len(), 0);

    // mark_synced with empty slice
    store.mark_synced(&[]).unwrap();

    // compact on empty store
    let deleted = store.compact(7).unwrap();
    assert_eq!(deleted, 0);
}
