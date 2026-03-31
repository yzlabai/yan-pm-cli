# Setup 模块

一键安装 yan-pm 到 AI 工具（Claude Code / VS Code / Cursor）。

## 代码位置

- `crates/yan-pm/src/cli/setup.rs` — setup 命令实现
- `crates/yan-pm/src/cli/detect.rs` — AI 工具检测
- `SKILL.md` — 编译时嵌入的 Skill 文档

## 命令接口

```bash
yan setup                       # 交互式安装（自动检测）
yan setup --target claude       # 指定目标
yan setup --uninstall           # 卸载
yan setup --status              # 查看安装状态
yan setup --binary-path <PATH>  # 手动指定二进制路径
yan setup --scope user|project  # MCP 注册范围（仅 Claude Code）
yan setup --yes                 # 跳过确认
```

## 安装内容

| 步骤 | 作用 | 方式 |
|------|------|------|
| 注册 MCP Server | AI agent 可调用 14 个 MCP tools | 写入目标工具的 MCP 配置 |
| 安装 Skill 文档 | AI agent 知道工作流 | 写入 `~/.claude/skills/yan-pm/SKILL.md`（仅 Claude Code） |

## 支持的目标

| Target | MCP 配置方式 | Skill |
|--------|-------------|-------|
| `claude` | `claude mcp add` CLI（回退: 直接写 `~/.claude.json`） | 是 |
| `vscode` | 写 `~/.vscode/mcp.json`（merge 已有配置） | 否 |
| `cursor` | 写 `~/.cursor/mcp.json`（merge 已有配置） | 否 |

## 二进制路径解析

优先级: 用户 `--binary-path` → `current_exe()` → `which yan`

检测到 `target/debug` 或 `target/release` 路径时警告用户。

## 模块内部结构

```
setup.rs
├── install(target, binary_path, scope, yes)
├── uninstall(target)
├── status()
├── detect_tools() → Vec<DetectedTool>
├── resolve_binary_path(override) → String
├── setup_claude / setup_vscode / setup_cursor
├── install_skill()
├── remove_claude / remove_vscode / remove_cursor
├── check_claude_status / check_vscode_status / check_cursor_status
└── SKILL_CONTENT = include_str!("../../../SKILL.md")
```

## 边界情况

- 已安装时再次 setup → 提示 "已安装，是否更新?"
- `claude` 命令不在 PATH → 回退写 `~/.claude.json`
- 配置文件格式错误 → 备份原文件 `.bak`
- 当前仅支持 macOS/Linux
