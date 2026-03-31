# Local SDD 模块

本地 Specification-Driven Development 流程：Issue 同步 → Spec 生成 → Task 管理。

## 代码位置

- `crates/yan-pm/src/local/directory.rs` — 本地目录管理（issues/specs/tasks）
- `crates/yan-pm/src/local/issuefile.rs` — Issue 本地文件格式
- `crates/yan-pm/src/local/specfile.rs` — Spec 文件格式
- `crates/yan-pm/src/local/taskfile.rs` — Task 文件格式
- `crates/yan-pm/src/local/task_parser.rs` — Task 解析
- `crates/yan-pm/src/cli/pull.rs` — `yan pull` 命令
- `crates/yan-pm/src/cli/spec.rs` — `yan spec` 命令
- `crates/yan-pm/src/cli/task.rs` — `yan tasks` 命令
- `crates/yan-pm/src/cli/verify.rs` — `yan verify` 命令
- `crates/yan-pm/src/cli/issue.rs` — Issue 管理命令

## SDD 工作流

```
yan pull          # 从云端拉取 Issue 到 .yan-pm/issues/
    ↓
yan spec <N>      # 为 Issue #N 生成技术规格模板
    ↓
(手动编辑 Spec)
    ↓
yan tasks <N>     # 查看/管理 Issue #N 的本地 Task
    ↓
yan verify <N>    # 验证 Issue 实现结果
```

## Issue 生命周期

```
open → accepted → delivered → closed
                            → cancelled
```

### Issue 文件格式

路径: `.yan-pm/issues/{number:03d}-{slug}.md`

```yaml
---
id: "uuid"
number: 1
title: "OAuth SSO 集成"
type: feature
priority: high
status: open
labels: [auth]
acceptance_criteria:
  - "支持 Google OAuth"
  - "Token 自动刷新"
assignee: "user-id"
created: "2026-03-28T10:00:00Z"
updated: "2026-03-28T10:00:00Z"
---

Issue 描述正文...
```

## Spec 文件格式

路径: `.yan-pm/specs/{issue_number:03d}-{slug}.md`

```yaml
---
issue: 1
title: "OAuth SSO 集成"
status: draft    # draft → ready → in_progress → done
created: "2026-03-28T10:00:00Z"
---

## 背景
(来自 Issue 描述)

## 技术方案

## 验收标准
- [ ] 支持 Google OAuth
- [ ] Token 自动刷新

## 任务拆分
```

## Task 文件格式

路径: `.yan-pm/tasks/{issue_number}-{seq}-{slug}.md`

```yaml
---
number: 1
title: "实现 OAuth 回调处理"
type: feature
priority: high
status: todo     # todo → in_progress → done
tags: [auth]
depends_on: []
issue: 1         # 关联 Issue 编号
requires: [mcp]  # Agent 能力需求
created: "2026-03-28T10:00:00Z"
updated: "2026-03-28T10:00:00Z"
---

Task 描述正文...
```

## CLI 命令

### Issue 子命令

| 命令 | 说明 |
|------|------|
| `yan issue list` | 列出需求 |
| `yan issue show <N>` | 查看需求详情 |
| `yan issue create` | 创建需求 |
| `yan issue accept <N>` | 认领需求（open → accepted） |
| `yan issue deliver <N>` | 标记已交付（accepted → delivered） |

### SDD 命令

| 命令 | 说明 |
|------|------|
| `yan pull` | 从云端拉取 Issue 到本地 |
| `yan spec <N>` | 为 Issue 生成/查看 Spec |
| `yan tasks [N]` | 查看本地 Task（可按 Issue 筛选） |
| `yan verify <N>` | 验证 Issue 实现 |

## API 变更

Phase 2 删除了所有云端 Task API 方法，Task 完全本地化。保留的云端 API：

- Project: list/get
- Issue: list/get/create/update + **accept/deliver**（新增）
- Workspace: 全部保留

## 后续（Phase 3）

- `yan spec` AI 自动生成 Spec 内容
- `yan run` 调度 AI Agent 执行本地 Task
- 多 Agent 并行调度
- `yan verify` 对照 acceptance criteria 自动验证
