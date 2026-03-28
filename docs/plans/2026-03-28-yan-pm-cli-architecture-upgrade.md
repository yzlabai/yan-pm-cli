# yan-pm-cli 架构升级 Spec

> 日期：2026-03-28

## 范围与决策

| 决策项 | 结论 |
|--------|------|
| 范围 | P0 WAL 事件持久化 + P1 连接状态机 + P2 Agent 注册表 + 能力协商 |
| 权限审批 | 不做（保持完全自治） |
| WAL 存储 | rusqlite bundled 静态链接 |
| 事件粒度 | Tool call 级别 |
| 观测台对接 | 只定义 CLI 侧持久化 + 上报 API，前端消费方式另议 |
| 架构策略 | 渐进集成，不引入新 crate，在现有模块内扩展 |

## 不做的事

- 权限请求转发到 Web UI
- E2EE 加密通道、Gateway 中继、连接池
- CDN 注册表
- 前端观测台消费层设计

---

## P0: WAL 事件持久化

### 目标

所有 Agent 执行事件先写入本地 SQLite，daemon 崩溃可恢复，观测台可断线回放。

### 存储

`~/.config/yan-pm/events.db`，WAL journal mode。

**并发策略**：单连接 + `Mutex<Connection>` 串行写入，设置 `busy_timeout = 5000ms` 避免 `SQLITE_BUSY`。

### 事件表

| 字段 | 类型 | 说明 |
|------|------|------|
| id | INTEGER PK | 自增序列号（回放用 seq） |
| task_id | TEXT | 关联任务 UUID |
| workspace_id | TEXT | 关联工作区 |
| event_type | TEXT | 见下方枚举 |
| payload | TEXT (JSON) | 事件数据，结构按 event_type 不同 |
| created_at | TEXT (ISO8601) | 事件时间 |
| synced_at | TEXT (ISO8601) NULL | 上报服务端的时间，NULL 表示未同步 |

**event_type 枚举**：`task_started` / `task_completed` / `task_failed` / `tool_call` / `tool_result` / `state_change` / `error`

**索引**：

| 索引 | 用途 |
|------|------|
| `(task_id, id)` | 回放查询 `afterSeq` |
| `(synced_at)` WHERE `synced_at IS NULL` | 上报时查未同步事件（partial index） |
| `(created_at)` | compact 清理 |

### 核心行为

1. **写入**：`agent/session.rs` ACP handler 中，每收到 ToolCall 事件写一条记录；任务开始/结束/失败各写一条
2. **上报**：daemon 异步批量上报到服务端，上报成功后标记 `synced_at`
3. **回放**：支持 `afterSeq` 参数查询，供断线重连拉取缺失事件
4. **清理**：daemon 启动时 + 每 24h 执行一次 compact，`DELETE` 7 天前已同步的事件后执行 `VACUUM`
5. **退出 flush**：daemon 收到 shutdown 信号时，立即 flush 待上报事件队列，避免丢失最后一批

### 新增模块

`daemon/event_store.rs` — 封装 SQLite 读写，接口：`insert(event)`、`query(task_id, after_seq, limit)`、`mark_synced(ids)`、`compact()`

### 新增依赖

`rusqlite`（features: `bundled`）

---

## P1: 连接状态机

### 目标

为每个 Agent 进程定义明确的生命周期状态，结构化错误码，提升异常恢复能力。

### 五态状态机

```
idle → connecting → ready ⇄ error → stopped
                ↑          │
                └──────────┘ (reconnect)
```

| 状态 | 含义 | 进入条件 |
|------|------|----------|
| `idle` | 已注册，未启动 | 初始状态 |
| `connecting` | 进程已 spawn，ACP 握手中 | 调用 execute_agent |
| `ready` | ACP initialize 成功，可接受 prompt | 收到 initialize 响应 |
| `error` | 连接异常，可重试 | 超时 / 协议错误 / 进程异常退出；`ready` 态断连也进入此状态 |
| `stopped` | 终态，已清理 | 正常完成 / 重试耗尽 / 手动停止 |

### 结构化错误码

| 错误码 | 触发场景 |
|--------|----------|
| `AGENT_NOT_FOUND` | 命令不存在（未安装） |
| `AGENT_SPAWN_FAILED` | 进程启动失败 |
| `AGENT_TIMEOUT` | ACP 握手或执行超时 |
| `AGENT_CRASHED` | 进程非零退出 |
| `PROTOCOL_ERROR` | ACP 消息解析失败 |

### 重试策略

`error` 状态下自动重试最多 2 次，指数退避 5s → 15s。重试时转入 `connecting` 状态。重试耗尽转 `stopped`，任务状态处理：

- **无副作用**（无 `tool_call` 事件）→ 回退为 `Todo`，可重新分配
- **有副作用**（已产生 `tool_call` 事件）→ 标记为 `Failed`，需人工介入

### 状态变更事件

每次状态转换写入 P0 event_store（event_type = `state_change`），payload 含 `from`、`to`、`error_code`（如有）。

### 改动点

- `agent/session.rs`：引入 `ConnectionState` 枚举和转换逻辑，替代当前退出码判断
- `daemon/auto_runner.rs`：根据状态决定重试还是放弃

---

## P2: Agent 后端注册表 + 能力协商

### 目标

将硬编码的 Agent 启动逻辑抽象为 trait，支持不同 AI 工具的协议差异和能力声明。

### `AgentBackend` trait

| 方法 | 说明 |
|------|------|
| `name()` | 标识名，如 `"claude"` |
| `command()` | 可执行文件路径/名称 |
| `acp_args()` | ACP 启动参数 |
| `env_vars()` | 额外环境变量 |
| `capabilities()` | 能力声明 |
| `is_available()` | 检测本机是否已安装 |
| `protocol_version()` | ACP 协议版本，握手时校验兼容性 |
| `build_prompt(task)` | 按 Agent 特性构造 prompt |

### 内置实现

`ClaudeBackend`、`CodexBackend`、`GeminiBackend`，从现有 `registry.rs` 的 `AgentDefinition` 迁移。

### 能力声明

| 能力 | 类型 | 说明 |
|------|------|------|
| `supports_images` | bool | 支持图片输入 |
| `supports_mcp` | bool | 支持 MCP 工具 |
| `supports_worktree` | bool | 支持 git worktree 隔离 |
| `max_context_tokens` | u32 | 最大上下文长度 |

### 发现与优先级

1. `~/.config/yan-pm/agents.toml` 声明可用后端及优先级
2. `yan-pm-cli setup` 自动检测已安装的 AI 工具，写入配置
3. 任务分配时可指定 `preferred_agent`，否则按优先级依次尝试（跳过未安装的）

**`agents.toml` 示例**：

```toml
default = "claude"

[[agents]]
name = "claude"
command = "claude"
priority = 1
enabled = true

[[agents]]
name = "codex"
command = "codex"
priority = 2
enabled = true

[[agents]]
name = "gemini"
command = "gemini"
priority = 3
enabled = false
```

### 能力匹配

任务有特殊需求标签（如 `needs_images`）时，优先选择满足能力的 Agent。无匹配时 fallback 到默认优先级。

### 改动点

- `agent/registry.rs` → 拆为 `agent/backend.rs`（trait）+ `agent/backends/`（各实现）
- `agent/session.rs` 接收 `&dyn AgentBackend` 而非 `AgentDefinition`
- `cli/agents.rs` 展示能力信息

---

## 事件上报 API

### 新增 REST 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/projects/{id}/tasks/{id}/events` | POST | 批量上报事件 |
| `/api/projects/{id}/tasks/{id}/events?afterSeq={n}&limit={n}` | GET | 按 seq 分页查询 |

### 认证

复用现有 API token（`Authorization: Bearer <token>`），与其他 REST 端点一致。

### 上报请求体

事件数组，每条含 `seq`、`event_type`、`payload`、`created_at`。服务端按 workspace_id + task_id + seq 去重。

### 整体数据流

```
Agent 进程 (ACP)
    ↓ tool_call / state_change 事件
agent/session.rs
    ↓ insert
daemon/event_store.rs (SQLite)
    ↓ 异步批量（每 10s 或攒满 50 条）
服务端 REST API
    ↓ 持久化到 PostgreSQL
观测台前端（消费方式后续 spec）
```

### 离线容错

网络不可达时事件留在本地 SQLite，daemon 恢复连接后按 `synced_at IS NULL` 自动补传。

---

## 实施顺序

```
P0: WAL 事件持久化（独立功能，观测台直接受益）
P1: 连接状态机（依赖 P0 记录状态变更事件）
P2: Agent 注册表 + 能力协商（独立重构，可与 P1 并行）
```
