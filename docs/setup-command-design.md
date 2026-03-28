# yan-pm-cli setup 命令设计方案

## 背景

yan-pm-cli 已有 14 个 MCP tools（`yan-pm-cli mcp` stdio 服务）和 25 个 CLI 命令，但用户需要手动编辑配置文件才能让 AI agent（Claude Code、VS Code Copilot、Cursor 等）发现和使用这些工具。需要一个 `setup` 命令实现一键安装。

## 目标

```
yan-pm-cli setup              # 交互式安装（自动检测已安装的 AI 工具）
yan-pm-cli setup --target claude   # 指定目标
yan-pm-cli setup --uninstall       # 卸载
yan-pm-cli setup --status          # 查看当前安装状态
```

安装后 AI agent 获得两个能力：
1. **MCP Tools** — 14 个可直接调用的函数（list_tasks、update_task 等）
2. **Skill 文档** — 工作流指导（什么时候用哪个工具、流程怎么走）

## 核心设计

### 安装两件事

| 步骤 | 作用 | 方式 |
|------|------|------|
| 注册 MCP Server | AI agent 能调用 14 个 MCP tools | 写入目标工具的 MCP 配置 |
| 安装 Skill 文档 | AI agent 知道工作流和最佳实践 | 写入 `~/.claude/skills/yan-pm/SKILL.md`（仅 Claude Code） |

### 支持的目标工具

| Target | MCP 配置位置 | MCP 类型 | Skill 支持 |
|--------|-------------|----------|-----------|
| `claude` | `claude mcp add` CLI 命令 | stdio | 是（`~/.claude/skills/yan-pm/`） |
| `vscode` | `~/.vscode/mcp.json` 或 `.vscode/mcp.json` | stdio | 否 |
| `cursor` | `~/.cursor/mcp.json` | stdio | 否 |

### MCP 注册策略

**Claude Code（推荐路径）**：shell out 到 `claude mcp add`

```bash
claude mcp add --transport stdio --scope user yan-pm -- /absolute/path/to/yan-pm-cli mcp
```

优点：
- 官方 API，格式变了也不受影响
- 自动处理去重、scope
- 不需要知道内部配置文件格式

回退：如果 `claude` 命令不存在，直接写 `~/.claude.json`

**VS Code / Cursor**：直接写 JSON 配置文件

```json
{
  "servers": {
    "yan-pm": {
      "type": "stdio",
      "command": "/absolute/path/to/yan-pm-cli",
      "args": ["mcp"]
    }
  }
}
```

注意：需要 merge 已有配置，不能覆盖。

### Skill 文档策略

SKILL.md 内容通过 `include_str!("../../SKILL.md")` 编译时嵌入二进制。

安装时写到 `~/.claude/skills/yan-pm/SKILL.md`。

SKILL.md 内容基于现有 `packages/yan-pm-skill/SKILL.md`，包含：
- 触发词（"my tasks"、"查看任务" 等）
- 13 个 MCP tool 说明表
- 5 个工作流程
- 任务状态机、类型、优先级
- 最佳实践

### 二进制路径解析

优先级：
1. 用户手动指定 `--binary-path /path/to/yan-pm-cli`（最高优先）
2. `std::env::current_exe()` — 当前运行的二进制绝对路径（`canonicalize` 解析符号链接）
3. `which yan-pm-cli` — PATH 中查找（兜底）

注意：`current_exe()` 在某些情况下可能返回临时路径（如 `cargo run`），需要验证路径稳定性。如果路径包含 `target/debug` 或 `target/release`，提示用户使用 `--binary-path` 指定安装后的路径。

### 前置条件检查

`setup` 执行前验证：
1. ~~yan-pm-cli 已登录（有 token）~~ **不要求登录** — MCP server 启动时才需要 token，setup 只做配置注册。用户可以先 setup 再 login。
2. 目标 AI 工具已安装 — 否则跳过并提示
3. 二进制路径可访问

### 卸载流程

```
yan-pm-cli setup --uninstall
yan-pm-cli setup --uninstall --target claude
```

- Claude Code：`claude mcp remove yan-pm` + `rm -rf ~/.claude/skills/yan-pm/`
- VS Code：从 mcp.json 中移除 `yan-pm` 条目
- Cursor：从 mcp.json 中移除 `yan-pm` 条目

### 状态查看

```
yan-pm-cli setup --status
```

输出示例：
```
yan-pm-cli setup 状态:

  Claude Code:
    MCP Server: ✓ 已注册 (scope: user)
    Skill:      ✓ 已安装 (~/.claude/skills/yan-pm/SKILL.md)

  VS Code:
    MCP Server: ✗ 未配置

  Cursor:
    MCP Server: ✗ 未检测到 Cursor
```

## 交互流程

### 默认（无参数）

```
$ yan-pm-cli setup

检测到以下 AI 工具:
  [1] Claude Code (/usr/local/bin/claude)
  [2] VS Code (~/.vscode/)
  [3] Cursor (~/.cursor/)

将为以上工具配置 yan-pm MCP Server。确认? [Y/n]

✓ Claude Code: MCP Server 已注册
✓ Claude Code: Skill 已安装
✓ VS Code: MCP Server 已配置
✓ Cursor: MCP Server 已配置

安装完成! 重启 AI 工具后即可使用。
试试在 Claude Code 中说: "查看我的待办任务"
```

### 指定目标

```
$ yan-pm-cli setup --target claude

✓ MCP Server 已注册 (scope: user)
✓ Skill 已安装 (~/.claude/skills/yan-pm/SKILL.md)
```

## CLI 接口

```
yan-pm-cli setup [OPTIONS]

Options:
  --target <TARGET>     指定目标工具 [可选值: claude, vscode, cursor]
  --uninstall           卸载配置
  --status              查看安装状态
  --binary-path <PATH>  手动指定 yan-pm-cli 二进制路径
  --scope <SCOPE>       MCP 注册范围 (仅 Claude Code) [默认: user] [可选值: user, project]
  --yes                 跳过确认提示
```

## 与现有代码的关系

- `src/cli/mod.rs` — 新增 `Setup` 子命令到 `Commands` 枚举
- `src/cli/setup.rs` — 新模块，实现 setup 逻辑
- `SKILL.md` — 项目根目录新增，编译时嵌入二进制
- `src/mcp/mod.rs` — 不变，setup 只是配置注册，不改 MCP 服务本身

## 边界情况

1. **已安装时再次 setup**：检测到已有配置时，提示 "已安装，是否更新?"，更新时覆盖旧配置
2. **claude 命令不在 PATH**：回退到直接写 `~/.claude.json`
3. **配置文件格式错误**：读取失败时备份原文件（`.bak`），写入新配置
4. **权限不足**：提示用户检查文件权限
5. **多版本共存**：用绝对路径注册，不会冲突
6. **VS Code 全局 vs 项目级**：默认写全局 `~/.vscode/mcp.json`，`--scope project` 时写当前目录 `.vscode/mcp.json`
7. **Windows 支持**：当前仅支持 macOS/Linux（`~` 展开、路径分隔符）。Windows 后续可加

## Review 结论

方案整体可行，核心收益：
- **零手动配置**：用户装完 CLI 跑一条命令就能用
- **多工具支持**：一次 setup 同时配好 Claude Code / VS Code / Cursor
- **可维护**：SKILL.md 内嵌二进制，版本跟着 CLI 走，`setup` 更新即可同步

风险点：
- `claude mcp add` 命令的参数格式可能变化 — 需要 pin 住当前行为，变了再适配
- VS Code / Cursor 的 mcp.json 格式是各自的约定，非标准化 — 需要关注变化
