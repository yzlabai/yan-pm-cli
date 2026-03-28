# yan-pm-cli

yan.chat 项目管理终端 CLI — 单二进制，零依赖安装。任务管理 + AI Agent 自动执行 + Daemon 守护 + MCP 桥接。

> Rust 实现，单二进制零依赖。26 CLI 命令 + 14 MCP Tools + AI Agent 自动执行 + Daemon 守护。

## 概念说明

| 概念 | 含义 |
|------|------|
| **yan.chat** | 品牌/域名，整个产品 |
| **yan-server** | 服务端（[xiaoyandev](https://gitee.com/yzlab/xiaoyan) `apps/server`） |
| **yan-pm** | 项目管理功能模块（服务端 + 前端） |
| **yan-pm-cli** | 本工具 — Rust 终端 CLI，供开发者电脑使用 |

## 特性

- **零依赖**：单二进制下载即用，无需 Bun/Node
- **跨平台**：macOS (arm64/x64) + Linux (arm64/x64)
- **低内存**：daemon 常驻 < 15 MB
- **26 CLI 命令** + **14 MCP Tools**
- **一键安装**：`setup` 命令自动配置 Claude Code / VS Code / Cursor
- **自动更新**：`self-update` 从 GitHub Releases 下载最新版本
- **ACP Agent 管理**：统一调度 Claude Code / Codex / Gemini
- **Daemon 守护进程**：后台同步 + 文件监听 + 自动执行
- **本地任务文件**：`.yan-pm/tasks/*.md` Markdown + frontmatter，双向同步

## 安装

```bash
# 从 GitHub Releases 下载（推荐）
# https://github.com/yzlabai/yan-pm-cli/releases

# 或从源码构建（需要 Rust 工具链）
cargo build --release
sudo cp target/release/yan-pm-cli /usr/local/bin/yan-pm-cli
```

## 快速开始

```bash
# 1. 登录（浏览器 Device Code Flow）
yan-pm-cli login

# 2. 一键安装到 AI 工具（Claude Code / VS Code / Cursor）
yan-pm-cli setup

# 3. 关联项目目录
cd /path/to/your/repo
yan-pm-cli link <project-slug>

# 4. 查看任务
yan-pm-cli tasks --status todo

# 5. 启动 daemon（后台同步 + 文件监听）
yan-pm-cli daemon start

# 6. 开启自动执行
yan-pm-cli auto-run on --agent claude
```

## 命令一览

### 项目与任务

```bash
yan-pm-cli list                              # 列出项目
yan-pm-cli tasks [--status S] [--priority P] # 列出任务（已关联目录读本地文件）
yan-pm-cli create <pid> --title "..."        # 创建任务
yan-pm-cli update <pid> <tid> --status done  # 更新任务
yan-pm-cli comment <pid> <tid> "完成说明"     # 添加评论
yan-pm-cli report <pid>                      # AI 项目报告
yan-pm-cli status <pid>                      # 执行状态
yan-pm-cli force-unlock <pid> <tid>          # 强制解锁
```

### 需求管理

```bash
yan-pm-cli issues <pid> [--status S]         # 列出需求
yan-pm-cli create-issue <pid> --title "..."  # 创建需求
yan-pm-cli update-issue <pid> <iid> ...      # 更新需求
yan-pm-cli decompose-issue <pid> <iid>       # AI 分解需求为任务
```

### 工作区

```bash
yan-pm-cli link <slug> [--path DIR]          # 关联目录到项目
yan-pm-cli unlink                            # 取消关联
yan-pm-cli info                              # 当前项目信息
yan-pm-cli workspaces <pid>                  # 列出工作区
yan-pm-cli sync                              # 手动同步
```

### Daemon 守护进程

```bash
yan-pm-cli daemon start                      # 启动（后台 fork）
yan-pm-cli daemon stop                       # 停止
yan-pm-cli daemon status                     # 查看状态
yan-pm-cli daemon restart                    # 重启
yan-pm-cli daemon logs [-f]                  # 查看日志
yan-pm-cli daemon install                    # 注册系统服务（开机自启）
yan-pm-cli daemon uninstall                  # 卸载系统服务
```

Daemon 功能：30s 轮询同步、文件变更即时推送（500ms debounce）、2min 心跳、AutoRunner 自动执行、WAL 事件持久化（10s 批量上报 + 离线补传）。

### Auto-Run 自动执行

```bash
yan-pm-cli auto-run on [OPTIONS]             # 启用
yan-pm-cli auto-run off                      # 禁用
yan-pm-cli auto-run status                   # 查看状态
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `--budget <N>` | 预算上限（USD） | 无限制 |
| `--concurrency <N>` | 并发任务数 | 1 |
| `--filter-priority <P>` | 只执行指定优先级 | 全部 |
| `--agent <ID>` | 使用的 Agent | claude |

### Setup（AI 工具集成）

```bash
yan-pm-cli setup                             # 自动检测并安装到所有 AI 工具
yan-pm-cli setup --target claude             # 仅安装到 Claude Code
yan-pm-cli setup --target vscode             # 仅安装到 VS Code
yan-pm-cli setup --target cursor             # 仅安装到 Cursor
yan-pm-cli setup --status                    # 查看安装状态
yan-pm-cli setup --uninstall                 # 卸载所有配置
```

安装内容：
- **MCP Server**：注册 yan-pm stdio MCP server，AI agent 可调用 14 个工具
- **Skill 文档**（仅 Claude Code）：安装工作流指导到 `~/.claude/skills/yan-pm/`

### AI Agent

```bash
yan-pm-cli start <pid> --cwd /repo           # 执行最高优先级任务
yan-pm-cli start <pid> --task <tid>          # 执行指定任务
yan-pm-cli start <pid> --auto                # 批量串行执行
yan-pm-cli agents                            # 列出可用 Agent
yan-pm-cli mcp                               # 启动 MCP stdio Server
```

### 更新

```bash
yan-pm-cli self-update                       # 从 GitHub Releases 自动更新
```

自动检测当前平台，下载最新版本并原子替换二进制。

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

启动方式：`yan-pm-cli mcp`（stdio JSON-RPC 2.0）。推荐用 `yan-pm-cli setup` 自动配置。

| Tool | 用途 |
|------|------|
| `list_projects` / `get_project` | 项目管理 |
| `list_tasks` / `get_task` / `create_task` / `update_task` | 任务 CRUD |
| `add_comment` / `get_report` / `decompose_task` | 评论 / 报告 / 分解 |
| `list_issues` / `get_issue` / `create_issue` / `update_issue` / `decompose_issue` | 需求管理 |

手动配置（如不用 `setup`）：
```json
{
  "mcpServers": {
    "yan-pm": {
      "command": "yan-pm-cli",
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

单 binary crate：

| 模块 | 职责 |
|------|------|
| `cli/` | 26 命令处理（含 setup） |
| `api/` | HTTP 客户端（22 API 方法） |
| `agent/` | ACP Agent 注册表 + 会话管理 + 状态机 + Backend trait |
| `agent/backends/` | Claude / Codex / Gemini 后端实现（能力声明 + 优先级） |
| `runner/` | 任务编排（single/batch/specific） |
| `mcp/` | MCP stdio Server（14 tools） |
| `local/` | 本地文件系统 + frontmatter 解析 |
| `sync/` | 双向同步引擎（LWW 冲突解决） |
| `daemon/` | 守护进程 + AutoRunner + 文件监听 + WAL 事件持久化 |
