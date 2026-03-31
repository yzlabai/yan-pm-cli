# yan

yan.chat CLI — 单二进制，零依赖安装。需求管理 + AI Agent 执行 + MCP 桥接。

> Rust 实现，单二进制零依赖。28 CLI 命令 + 14 MCP Tools + AI Agent 自动执行 + Daemon 守护。

## 概念说明

| 概念 | 含义 |
|------|------|
| **yan.chat** | 品牌/域名，整个产品 |
| **yan-server** | 服务端（[xiaoyandev](https://gitee.com/yzlab/xiaoyan) `apps/server`） |
| **yan-pm** | 项目管理功能模块（服务端 + 前端） |
| **yan** (CLI) | 本工具 — Rust 终端 CLI，供开发者电脑使用 |

## 特性

- **零依赖**：单二进制下载即用，无需 Bun/Node
- **跨平台**：macOS (arm64/x64) + Linux (arm64/x64)
- **低内存**：daemon 常驻 < 15 MB
- **28 CLI 命令** + **14 MCP Tools**
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
cargo install --path crates/yan-pm
```

## 登录与配置

```bash
# 首次登录（需指定服务器地址）
yan --url https://yan.chat login

# 或手动提供 token（跳过浏览器流程）
yan --url https://yan.chat login --token <your-token>
```

登录后配置保存在 `~/.config/yan-pm/config.json`，后续命令自动读取，无需重复指定 `--url`。

也可通过环境变量配置：

```bash
export YAN_PM_BASE_URL=https://yan.chat
export YAN_PM_TOKEN=your_token
```

优先级：CLI 参数 > 环境变量 > 配置文件。

## 快速开始

```bash
# 1. 登录（浏览器 Device Code Flow，自动打开浏览器授权）
yan --url https://yan.chat login

# 2. 一键安装到 AI 工具（Claude Code / VS Code / Cursor）
yan setup

# 3. 关联项目目录
cd /path/to/your/repo
yan link <project-slug>

# 4. 查看任务
yan tasks

# 5. 同步项目信息到云端（repoUrl / techStack / CLAUDE.md）
yan project sync-info
```

## 典型使用流程

### 流程一：首次配置

```
登录 → setup 安装到 AI 工具 → 进入项目目录 → link 关联项目 → sync-info 同步信息
```

```bash
yan --url https://yan.chat login           # 浏览器授权，token 保存到本地
yan setup                                  # MCP + Skill 安装到 Claude Code
cd ~/projects/my-repo
yan link my-project                        # 关联目录，拉取任务文件
yan project sync-info                      # 同步 repoUrl、techStack、CLAUDE.md 到云端
```

### 流程二：日常开发（手动模式）

```
查看任务 → 选择任务 → AI Agent 执行 → 完成上报
```

```bash
yan tasks                                  # 查看待办任务
yan start my-project --task <tid>          # AI Agent 执行指定任务
yan start my-project                       # 或自动选择最高优先级任务
```

### 流程三：自动执行（Daemon 模式）

```
启动 daemon → 开启 auto-run → daemon 自动领取并执行任务
```

```bash
yan daemon start                           # 后台启动（同步 + 文件监听 + 心跳）
yan auto-run on --agent claude             # 开启自动执行
yan dashboard                              # 实时查看所有 workspace 状态
```

### 流程四：在 Claude Code 中使用（MCP / Skill）

`setup` 后重启 Claude Code 会话，即可在对话中使用：

```
用户: 查看我的待办任务
Claude: [调用 list_tasks MCP tool] 你有 3 个待办任务...

用户: 开始处理第一个
Claude: [调用 update_task 标记为 in_progress] → 阅读代码 → 实现 → [调用 update_task 标记为 done]
```

> MCP server 支持惰性认证：如果启动时未登录，可以在会话中执行 `yan login` 后直接使用，无需重启。

## 命令一览

### 项目与任务

```bash
yan list                                     # 列出项目
yan tasks [--status S] [--priority P]        # 列出任务（已关联目录读本地文件）
yan create <pid> --title "..."               # 创建任务
yan update <pid> <tid> --status done         # 更新任务
yan comment <pid> <tid> "完成说明"            # 添加评论
yan report <pid>                             # AI 项目报告
yan status <pid>                             # 执行状态
yan force-unlock <pid> <tid>                 # 强制解锁
```

### 需求管理

```bash
yan issues <pid> [--status S]                # 列出需求
yan create-issue <pid> --title "..."         # 创建需求
yan update-issue <pid> <iid> ...             # 更新需求
yan decompose-issue <pid> <iid>              # AI 分解需求为任务
```

### 工作区

```bash
yan link <slug> [--path DIR]                 # 关联目录到项目
yan unlink                                   # 取消关联
yan info                                     # 当前项目信息
yan workspaces <pid>                         # 列出工作区
yan sync                                     # 手动同步
yan project sync-info                        # 同步项目信息到云端
```

> **换目录**：如果项目目录迁移了（重新 clone、换路径等），在新目录重新 `link` 即可。旧工作区会在 web 端显示为离线，可手动删除或等待 7 天自动清理。

### Daemon 守护进程

```bash
yan daemon start                             # 启动（后台 fork）
yan daemon stop                              # 停止
yan daemon status                            # 查看状态
yan daemon restart                           # 重启
yan daemon logs [-f]                         # 查看日志
yan daemon install                           # 注册系统服务（开机自启）
yan daemon uninstall                         # 卸载系统服务
```

Daemon 功能：30s 轮询同步、文件变更即时推送（500ms debounce）、2min 心跳、AutoRunner 自动执行、WAL 事件持久化（10s 批量上报 + 离线补传）。

### Auto-Run 自动执行

```bash
yan auto-run on [OPTIONS]                    # 启用
yan auto-run off                             # 禁用
yan auto-run status                          # 查看状态
```

| 选项 | 说明 | 默认值 |
|------|------|--------|
| `--budget <N>` | 预算上限（USD） | 无限制 |
| `--concurrency <N>` | 并发任务数 | 1 |
| `--filter-priority <P>` | 只执行指定优先级 | 全部 |
| `--agent <ID>` | 使用的 Agent | claude |

### Setup（AI 工具集成）

```bash
yan setup                                    # 自动检测并安装到所有 AI 工具
yan setup --target claude                    # 仅安装到 Claude Code
yan setup --target vscode                    # 仅安装到 VS Code
yan setup --target cursor                    # 仅安装到 Cursor
yan setup --status                           # 查看安装状态
yan setup --uninstall                        # 卸载所有配置
```

安装内容：
- **MCP Server**：注册 yan-pm stdio MCP server，AI agent 可调用 14 个工具
- **Skill 文档**（仅 Claude Code）：安装工作流指导到 `~/.claude/skills/yan-pm/`

### Dashboard

```bash
yan dashboard                                # 全局 Dashboard：所有 workspace 状态概览
yan dashboard --compact                      # 紧凑模式（单行 per workspace）
yan dashboard --json                         # JSON 输出（供脚本消费）
```

Dashboard 展示：workspace 列表、项目名称/状态、daemon 是否在线、auto-run 配置、正在执行的 agent 任务及历史。

### AI Agent

```bash
yan start <pid> --cwd /repo                  # 执行最高优先级任务
yan start <pid> --task <tid>                 # 执行指定任务
yan start <pid> --auto                       # 批量串行执行
yan agents                                   # 列出可用 Agent（含 capabilities）
yan agents --running                         # 仅显示正在执行的 agent
yan agents --json                            # JSON 输出
yan mcp                                      # 启动 MCP stdio Server
```

### 更新

```bash
yan self-update                              # 从 GitHub Releases 自动更新
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

启动方式：`yan mcp`（stdio JSON-RPC 2.0）。推荐用 `yan setup` 自动配置。

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
      "command": "yan",
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
| `cli/` | 28 命令处理（含 setup + dashboard） |
| `api/` | HTTP 客户端（22 API 方法） |
| `agent/` | ACP Agent 注册表 + 会话管理 + 状态机 + Backend trait |
| `agent/backends/` | Claude / Codex / Gemini 后端实现（能力声明 + 优先级） |
| `runner/` | 任务编排（single/batch/specific） |
| `mcp/` | MCP stdio Server（14 tools） |
| `local/` | 本地文件系统 + frontmatter 解析 |
| `sync/` | 双向同步引擎（LWW 冲突解决） |
| `daemon/` | 守护进程 + AutoRunner + 文件监听 + WAL 事件持久化 |
