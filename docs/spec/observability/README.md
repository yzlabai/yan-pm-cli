# Observability 模块

CLI 侧可观测性：Dashboard、TUI 实时面板、Agent 日志。

## 代码位置

- `crates/yan-pm/src/cli/dashboard.rs` — Dashboard 命令（静态输出）
- `crates/yan-pm/src/cli/agents.rs` — Agents 命令（capabilities 展示）
- `crates/yan-pm/src/tui/mod.rs` — TUI 模块入口 + Terminal 管理
- `crates/yan-pm/src/tui/app.rs` — App state + 事件循环
- `crates/yan-pm/src/tui/ui.rs` — 纯渲染函数
- `crates/yan-pm/src/output/format.rs` — 输出格式化

## Dashboard 命令

```bash
yan dashboard              # 全量（详细表格）
yan dashboard --compact    # 紧凑模式（单行 per workspace）
yan dashboard --json       # JSON 输出
yan dashboard --live       # TUI 实时刷新
```

### 数据来源

| 数据 | 来源 | 离线可用 |
|------|------|----------|
| Workspace 列表 | `~/.config/yan-pm-cli/workspaces.json` | 是 |
| 项目名称/状态 | `.yan-pm/config.json` 或 API | 降级 |
| Daemon 状态 | PID 文件 | 是 |
| Auto-run 配置 | `.yan-pm/config.json` | 是 |
| 执行中任务 | EventStore | 是 |
| 执行历史 | EventStore | 是 |

## Agents 命令

```bash
yan agents                 # 列出所有 agent 及 capabilities
yan agents --running       # 仅正在执行的
yan agents --json          # JSON 输出
```

展示: Agent 名称、可用状态、Context 大小、MCP/IMG/Worktree 支持、命令。

## TUI 实时 Dashboard

### 架构

```
cli/dashboard.rs  → --live flag → tui/mod.rs (Terminal 初始化)
                                  tui/app.rs (state + event loop)
                                  tui/ui.rs  (纯渲染函数)
```

### 布局

```
┌─ Header ────────────────────────────────────────────┐
│ Daemon status · workspace count · active agents      │
├─ Workspace List ────────────────────────────────────┤
│ 每个 workspace: 名称 + 路径 + auto-run + task 表    │
├─ Footer ────────────────────────────────────────────┤
│ 快捷键: ↑↓ 选择 · Enter 展开 · r 刷新 · q 退出     │
└─────────────────────────────────────────────────────┘
```

### 快捷键

| 键 | 功能 |
|-----|------|
| `↑↓` | 选择 workspace |
| `Enter` | 展开/折叠 |
| `r` | 手动刷新 |
| `a` | 切换 auto-run |
| `q` | 退出 |

### 依赖

- `ratatui` 0.29+ — TUI 框架
- `crossterm` 0.28+ — 终端后端

### 终端安全

panic hook + Drop trait 双重保障恢复终端状态。

## Agent 日志流（规划中）

分三阶段：

1. **展示已有事件** — TUI 中选中任务后展示 `tool_call` / `tool_result` 事件序列
2. **流式写入** — session 按 chunk 写入 `agent_output` 事件到 EventStore
3. **日志增强** — 搜索（`/`）、过滤（`f`）、导出（`e`）

## 不做的事

- Web 观测台前端（属于 xiaoyandev）
- WebSocket 实时推送
- 多机聚合
- Agent 编排
