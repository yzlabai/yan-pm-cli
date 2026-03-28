# Dashboard & Agents 增强实施记录

> 日期：2026-03-28 | Spec: `docs/plans/2026-03-28-iteration-dashboard.md`

## 概览

实现 S1（全局 Dashboard）+ S2（Agents 增强），共 6 文件变更、+~400 行。

| 编号 | 内容 | 状态 |
|------|------|------|
| S1 | `yan-pm dashboard` 全局 Dashboard 命令 | ✅ 已实现 |
| S2 | `yan-pm agents` 增强（capabilities + 运行状态） | ✅ 已实现 |
| S3 | Session Backend trait 迁移 | 🔲 后续 |
| S4 | Auto-runner 能力选择 | 🔲 后续 |

## 新增文件

| 文件 | 行数 | 职责 |
|------|------|------|
| `cli/dashboard.rs` | ~170 | Dashboard 数据收集编排（workspace 列表 + daemon 状态 + EventStore 查询 + auto-run 配置） |

## 修改文件

| 文件 | 主要变更 |
|------|----------|
| `daemon/event_store.rs` | +`query_active_tasks()` 跨 workspace 查询活跃任务 + `query_recent_completed(limit)` 最近完成的任务 + 2 测试 |
| `cli/agents.rs` | 重写：展示 Context/MCP/IMG/Worktree capabilities 列，`--running` 过滤，`--json` 输出，显示正在执行的 agent |
| `output/format.rs` | +`print_dashboard()` 默认模式 + `print_dashboard_compact()` 紧凑模式 + `format_elapsed()` 时间格式化 |
| `cli/mod.rs` | 注册 `dashboard` 模块 |
| `main.rs` | 新增 `Dashboard { --compact }` 子命令，`Agents` 增加 `--running` flag |

## 关键设计决策

### EventStore 查询策略

- `query_active_tasks()`：用 `NOT EXISTS` 子查询判断 `task_started` 后是否有对应的 `task_completed/task_failed`，确保只返回真正活跃的任务
- `query_recent_completed(limit)`：按 `created_at DESC` 排序返回最近完成/失败的事件

### Dashboard 数据编排

- **离线优先**：所有核心数据（workspace 列表、本地配置、EventStore、daemon PID）均从本地获取，无需 API
- **workspace 名称**：从路径最后一段推导，避免额外 API 调用
- **三种输出**：默认（详细表格）、`--compact`（单行 per workspace）、`--json`（机器可读）

### Agents 增强

- 合并 `AgentBackend` trait 的 capabilities 数据到表格展示
- 复用 EventStore `query_active_tasks()` 显示正在执行的 agent
- `--json` 输出完整的 capabilities 结构

## 测试

112/112 通过（含 2 个新增 EventStore 测试），集成测试全量通过。

## 待办（后续迭代）

- [ ] S3: `session.rs` 改为接收 `&dyn AgentBackend` 替代 `AgentDefinition`
- [ ] S4: `auto_runner.rs` 任务分配时使用 `find_capable_backend()` 按能力选 Agent
- [ ] S5: 服务端实现 `/api/projects/{id}/tasks/{id}/events` POST/GET 端点
- [ ] TUI 实时刷新（ratatui/crossterm）
