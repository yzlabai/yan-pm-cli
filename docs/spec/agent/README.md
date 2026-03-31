# Agent 模块

多 AI Agent 后端抽象与调度。

## 代码位置

- `crates/yan-pm/src/agent/backend.rs` — `AgentBackend` trait 定义
- `crates/yan-pm/src/agent/backends/` — 内置实现（Claude / Codex / Gemini）
- `crates/yan-pm/src/agent/registry.rs` — Agent 发现与优先级管理
- `crates/yan-pm/src/agent/session.rs` — ACP 会话管理
- `crates/yan-pm/src/agent/state.rs` — 连接状态机

## AgentBackend Trait

| 方法 | 说明 |
|------|------|
| `name()` | 标识名，如 `"claude"` |
| `command()` | 可执行文件路径 |
| `acp_args()` | ACP 启动参数 |
| `env_vars()` | 额外环境变量 |
| `capabilities()` | 能力声明 |
| `is_available()` | 本机是否已安装 |
| `protocol_version()` | ACP 协议版本 |
| `build_prompt(task)` | 构造 prompt |

## 能力声明

| 能力 | 类型 | 说明 |
|------|------|------|
| `supports_images` | bool | 支持图片输入 |
| `supports_mcp` | bool | 支持 MCP 工具 |
| `supports_worktree` | bool | 支持 git worktree 隔离 |
| `max_context_tokens` | u32 | 最大上下文长度 |

## 连接状态机

```
idle → connecting → ready ⇄ error → stopped
                ↑          │
                └──────────┘ (reconnect)
```

| 状态 | 含义 |
|------|------|
| `idle` | 已注册，未启动 |
| `connecting` | 进程已 spawn，ACP 握手中 |
| `ready` | ACP initialize 成功 |
| `error` | 连接异常，可重试（最多 2 次，指数退避 5s → 15s） |
| `stopped` | 终态，已清理 |

### 结构化错误码

`AGENT_NOT_FOUND` / `AGENT_SPAWN_FAILED` / `AGENT_TIMEOUT` / `AGENT_CRASHED` / `PROTOCOL_ERROR`

## Agent 发现与配置

1. `~/.config/yan-pm/agents.toml` 声明可用后端及优先级
2. `yan setup` 自动检测已安装的 AI 工具
3. 任务分配时可指定 `preferred_agent`，否则按优先级依次尝试

```toml
default = "claude"

[[agents]]
name = "claude"
command = "claude"
priority = 1
enabled = true
```

## 能力匹配

任务有特殊需求标签（如 `needs_images`）时，优先选择满足能力的 Agent。匹配失败时 fallback 到默认优先级。

`find_capable_backend()` 位于 `agent/registry.rs`，由 `auto_runner` 调用。

## 实现状态

- [x] P0: WAL 事件持久化
- [x] P1: 连接状态机
- [x] P2: Backend 注册表 + 能力协商
- [ ] Session Backend trait 迁移（`execute_agent` 接收 `&dyn AgentBackend`）
- [ ] Auto-runner 能力选择
