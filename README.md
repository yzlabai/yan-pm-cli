# yan-pm (Rust)

YanChat 项目管理 CLI — 单二进制，零依赖安装。任务管理 + AI Agent 自动执行 + Daemon 守护 + MCP 桥接。

> Rust 重写版，替代 `packages/yan-pm`（TypeScript/Bun）。功能完全覆盖 + 新增 daemon、本地文件同步、auto-run。

## 特性

- **零依赖**：单二进制下载即用，无需 Bun/Node
- **跨平台**：macOS (arm64/x64) + Linux (arm64/x64) + Windows (x64)
- **低内存**：daemon 常驻 < 15 MB（TS 版 Bun ~80 MB）
- **25 CLI 命令** + **14 MCP Tools**
- **ACP Agent 管理**：统一调度 Claude Code / Codex / Gemini
- **Daemon 守护进程**：后台同步 + 文件监听 + 自动执行
- **本地任务文件**：`.yan-pm/tasks/*.md` Markdown + frontmatter，双向同步

## 安装

```bash
# 从源码构建（需要 Rust 工具链）
cd yan-pm
cargo build --release
sudo cp target/release/yan-pm /usr/local/bin/
```

## 快速开始

```bash
# 1. 登录（浏览器 Device Code Flow）
yan-pm login

# 2. 关联项目目录
cd /path/to/your/repo
yan-pm link <project-slug>

# 3. 查看任务
yan-pm tasks --status todo

# 4. 启动 daemon（后台同步 + 文件监听）
yan-pm daemon start

# 5. 开启自动执行
yan-pm auto-run on --agent claude
```

## 命令一览

### 项目与任务

```bash
yan-pm list                              # 列出项目
yan-pm tasks [--status S] [--priority P] # 列出任务（已关联目录读本地文件）
yan-pm create <pid> --title "..."        # 创建任务
yan-pm update <pid> <tid> --status done  # 更新任务
yan-pm comment <pid> <tid> "完成说明"     # 添加评论
yan-pm report <pid>                      # AI 项目报告
yan-pm status <pid>                      # 执行状态
yan-pm force-unlock <pid> <tid>          # 强制解锁
```

### 需求管理

```bash
yan-pm issues <pid> [--status S]         # 列出需求
yan-pm create-issue <pid> --title "..."  # 创建需求
yan-pm update-issue <pid> <iid> ...      # 更新需求
yan-pm decompose-issue <pid> <iid>       # AI 分解需求为任务
```

### 工作区

```bash
yan-pm link <slug> [--path DIR]          # 关联目录到项目
yan-pm unlink                            # 取消关联
yan-pm info                              # 当前项目信息
yan-pm workspaces <pid>                  # 列出工作区
yan-pm sync                              # 手动同步
```

### Daemon 守护进程

```bash
yan-pm daemon start                      # 启动（后台 fork）
yan-pm daemon stop                       # 停止
yan-pm daemon status                     # 查看状态
yan-pm daemon restart                    # 重启
yan-pm daemon logs [-f]                  # 查看日志
yan-pm daemon install                    # 注册系统服务（开机自启）
yan-pm daemon uninstall                  # 卸载系统服务
```

Daemon 功能：30s 轮询同步、文件变更即时推送（500ms debounce）、2min 心跳、AutoRunner 自动执行。

### Auto-Run 自动执行

```bash
yan-pm auto-run on [OPTIONS]             # 启用
yan-pm auto-run off                      # 禁用
yan-pm auto-run status                   # 查看状态
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `--budget <N>` | 预算上限（USD） | 无限制 |
| `--concurrency <N>` | 并发任务数 | 1 |
| `--filter-priority <P>` | 只执行指定优先级 | 全部 |
| `--agent <ID>` | 使用的 Agent | claude |

### AI Agent

```bash
yan-pm start <pid> --cwd /repo           # 执行最高优先级任务
yan-pm start <pid> --task <tid>          # 执行指定任务
yan-pm start <pid> --auto                # 批量串行执行
yan-pm agents                            # 列出可用 Agent
yan-pm mcp                               # 启动 MCP stdio Server
yan-pm self-update                       # 自更新
```

### 全局选项

| 选项 | 说明 |
|------|------|
| `--url <URL>` | 服务器地址（env: `YAN_PM_BASE_URL`） |
| `--token <TOKEN>` | 认证 Token（env: `YAN_PM_TOKEN`） |
| `--json` | JSON 格式输出 |

## 本地文件格式

关联项目后，任务以 Markdown 文件存储在 `.yan-pm/tasks/`：

```yaml
---
id: abc-123
number: 1
title: Fix login bug
type: bug
priority: urgent
status: todo
tags: [auth]
created: 2026-03-25T10:00:00Z
updated: 2026-03-25T10:00:00Z
---

Bug description here.
```

文件名格式：`{number:03d}-{title-slug}.md`（如 `001-fix-login-bug.md`）。
完成/取消的任务自动归档到 `.yan-pm/done/`。

## MCP Tools（14 个）

启动方式：`yan-pm mcp`（stdio JSON-RPC 2.0），适用于不支持远程 HTTP MCP 的工具。

| Tool | 用途 |
|------|------|
| `list_projects` / `get_project` | 项目管理 |
| `list_tasks` / `get_task` / `create_task` / `update_task` | 任务 CRUD |
| `add_comment` / `get_report` / `decompose_task` | 评论 / 报告 / 分解 |
| `list_issues` / `get_issue` / `create_issue` / `update_issue` / `decompose_issue` | 需求管理 |

MCP 配置示例（Claude Code `.mcp.json`）：
```json
{
  "mcpServers": {
    "yan-pm": {
      "command": "yan-pm",
      "args": ["mcp"]
    }
  }
}
```

## Agent 支持

通过 [ACP 协议](https://agentclientprotocol.com/) 统一管理 AI Agent：

| Agent | 命令 | 状态 |
|-------|------|------|
| Claude Code | `claude --acp` | ✅ |
| Codex | `codex --acp` | ✅ |
| Gemini CLI | `gemini --experimental-acp` | ✅ |

自定义 Agent 可通过 `~/.config/yan-pm/agents.toml` 配置。

## 开发

```bash
cd yan-pm
cargo build              # 开发构建
cargo test               # 运行测试
cargo build --release     # 发布构建（strip + LTO，~6-10 MB）
```

## 架构

~6,400 行 Rust，单 binary crate：

| 模块 | 职责 |
|------|------|
| `cli/` | 25 命令处理 |
| `api/` | HTTP 客户端（22 API 方法） |
| `agent/` | ACP Agent 注册表 + 会话管理 |
| `runner/` | 任务编排（single/batch/specific） |
| `mcp/` | MCP stdio Server（14 tools） |
| `local/` | 本地文件系统 + frontmatter 解析 |
| `sync/` | 双向同步引擎（LWW 冲突解决） |
| `daemon/` | 守护进程 + AutoRunner + 文件监听 |

设计文档：`docs/plans/2026-03-25-yan-pm-rust-rewrite-guide.md`
