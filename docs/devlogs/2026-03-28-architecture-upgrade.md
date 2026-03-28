# 架构升级实施记录

> 日期：2026-03-28 | Spec: `docs/plans/2026-03-28-yan-pm-cli-architecture-upgrade.md`

## 概览

一次性完成 P0/P1/P2 三个阶段的架构升级，共 14 commits、21 files changed、+1621/-107 行。

| Phase | 内容 | 状态 |
|-------|------|------|
| P0 | WAL 事件持久化 | ✅ 已实现 |
| P1 | 连接状态机 | ✅ 已实现 |
| P2 | Agent 后端注册表 + 能力协商 | ✅ 已实现 |

## 新增文件

| 文件 | 行数 | 职责 |
|------|------|------|
| `daemon/event_store.rs` | 280 | SQLite WAL 事件存储（insert/query/mark_synced/compact） |
| `daemon/event_uploader.rs` | 143 | 异步批量上报 + 退出 flush |
| `agent/state.rs` | 83 | ConnectionState 五态状态机 + AgentErrorCode |
| `agent/backend.rs` | 74 | AgentBackend trait + AgentCapabilities |
| `agent/backends/claude.rs` | 34 | ClaudeBackend 实现 |
| `agent/backends/codex.rs` | 34 | CodexBackend 实现 |
| `agent/backends/gemini.rs` | 34 | GeminiBackend 实现 |
| `agent/backends/mod.rs` | 17 | 后端模块入口 + builtin_backends() |
| `lib.rs` | 12 | 库入口（供集成测试访问内部模块） |
| `tests/event_store_test.rs` | 121 | EventStore 集成测试（4 tests） |
| `tests/connection_state_test.rs` | 118 | 状态机转换测试（4 tests） |
| `tests/agent_backend_test.rs` | 63 | Backend trait 测试（6 tests） |

## 修改文件

| 文件 | 主要变更 |
|------|----------|
| `Cargo.toml` | +rusqlite bundled, +[lib] section |
| `agent/session.rs` | 新增 ExecutionContext、状态跟踪、state_change/tool_call 事件写入 |
| `agent/registry.rs` | 新增 find_backend/list_backends_by_priority/find_capable_backend |
| `agent/mod.rs` | 导出 state, backend, backends 模块 |
| `daemon/auto_runner.rs` | 事件写入 + 重试逻辑（指数退避 + 副作用检测） |
| `daemon/process.rs` | EventStore 初始化 + 上报/compact 定时器 + shutdown flush |
| `daemon/mod.rs` | 导出 event_store, event_uploader |
| `api/client.rs` | +post_raw 方法 |
| `runner/mod.rs` | 适配 execute_agent 新签名 |

## 关键设计决策

### P0: EventStore

- **并发策略**：`Mutex<Connection>` 串行写入 + `busy_timeout=5000ms`，简单可靠
- **上报节奏**：每 10s 或攒满 50 条，daemon 退出时 flush
- **Compact**：启动时 + 每 24h，删除 7 天前已同步事件
- **离线容错**：`synced_at IS NULL` 查询未同步事件，恢复连接后自动补传

### P1: ConnectionState

- **五态**：`idle → connecting → ready ⇄ error → stopped`，error 可回到 connecting 重连
- **重试**：最多 2 次，指数退避 5s → 15s
- **副作用检测**：查询 event_store 中是否有 `tool_call` 事件，有则标记 Failed 需人工介入，无则回退 Todo 可重新分配

### P2: AgentBackend

- **trait 抽象**：`AgentBackend` trait 定义统一接口，`AgentDefinition` 保留作兼容层
- **能力声明**：`supports_images`, `supports_mcp`, `supports_worktree`, `max_context_tokens`
- **优先级发现**：`find_capable_backend()` 按优先级 + 能力匹配自动选择

## 测试

112/112 通过（含 14 个新增测试），clippy clean（仅 dead_code warnings — P2 API 尚未被 binary 消费）。

## 待办（后续迭代）

- [ ] 服务端实现 `/api/projects/{id}/tasks/{id}/events` POST/GET 端点
- [x] `cli/agents.rs` 展示 capabilities 信息 → 见 `devlogs/2026-03-28-dashboard-agents.md`
- [ ] `session.rs` 改为接收 `&dyn AgentBackend` 替代 `AgentDefinition`
- [ ] 前端观测台消费事件流
- [ ] `auto_runner.rs` 任务分配时使用 `find_capable_backend()` 按能力选 Agent
