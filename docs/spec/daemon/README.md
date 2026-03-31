# Daemon 模块

后台守护进程，负责事件持久化、同步上报、自动任务执行。

## 代码位置

- `crates/yan-pm/src/daemon/service.rs` — daemon 主服务
- `crates/yan-pm/src/daemon/event_store.rs` — SQLite WAL 事件存储
- `crates/yan-pm/src/daemon/event_uploader.rs` — 事件批量上报
- `crates/yan-pm/src/daemon/auto_runner.rs` — 自动任务执行
- `crates/yan-pm/src/daemon/state.rs` — daemon 状态管理
- `crates/yan-pm/src/daemon/sync_manager.rs` — 同步管理
- `crates/yan-pm/src/daemon/file_watcher.rs` — 文件监控
- `crates/yan-pm/src/daemon/heartbeat.rs` — 心跳
- `crates/yan-pm/src/daemon/pid.rs` — PID 文件管理
- `crates/yan-pm/src/daemon/process.rs` — 进程管理

## EventStore

SQLite WAL 模式存储，路径 `~/.config/yan-pm/events.db`。

**并发策略**: 单连接 + `Mutex<Connection>` 串行写入，`busy_timeout = 5000ms`。

### 事件表

| 字段 | 类型 | 说明 |
|------|------|------|
| id | INTEGER PK | 自增序列号 |
| task_id | TEXT | 关联任务 UUID |
| workspace_id | TEXT | 关联工作区 |
| event_type | TEXT | 枚举（见下） |
| payload | TEXT (JSON) | 事件数据 |
| created_at | TEXT (ISO8601) | 事件时间 |
| synced_at | TEXT NULL | 上报时间，NULL = 未同步 |

**event_type**: `task_started` / `task_completed` / `task_failed` / `tool_call` / `tool_result` / `state_change` / `error`

### 核心接口

| 方法 | 说明 |
|------|------|
| `insert(event)` | 写入事件 |
| `query(task_id, after_seq, limit)` | 回放查询 |
| `mark_synced(ids)` | 标记已同步 |
| `compact()` | 清理 7 天前已同步事件 |
| `query_active_tasks()` | 查询活跃任务 |
| `query_recent_completed(limit)` | 最近完成的任务 |

### 行为

1. **写入**: ACP handler 中每收到事件写一条记录
2. **上报**: 异步批量上报（每 10s 或 50 条），成功后标记 `synced_at`
3. **回放**: 支持 `afterSeq` 参数，断线重连拉取缺失事件
4. **清理**: daemon 启动时 + 每 24h，`DELETE` 7 天前已同步事件后 `VACUUM`
5. **退出 flush**: shutdown 信号时立即 flush 待上报队列

## 事件上报 API

```
POST /api/projects/{id}/tasks/{id}/events   — 批量上报
GET  /api/projects/{id}/tasks/{id}/events    — 按 seq 分页查询
```

幂等性: `local_id` 字段（复合格式 `{daemon_session_id}:{sqlite_rowid}`）确保重复上传不产生重复记录。

### 数据流

```
Agent (ACP) → session.rs → EventStore (SQLite)
    → event_uploader (异步, 10s/50条) → 服务端 REST API → PostgreSQL
```

## Auto-runner

配置位于 `.yan-pm/config.json`:

```json
{
  "autoRun": {
    "agent": "auto",
    "concurrency": 2
  }
}
```

`agent: "auto"` 时调用 `find_capable_backend()` 按任务需求自动选择 Agent。

## 依赖

- `rusqlite` (bundled) — SQLite WAL 存储
