# Phase 2: CLI 本地化 — SDD 流程实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 yan-pm-cli 从云端 Task 的"遥控器"升级为本地自主运行的 Agent PM，实现 Issue 同步 + Spec 生成 + 本地 Task 管理的 SDD 流程。

**Architecture:** 分两个子阶段：(2a) 修复 CLI 与简化后云端的对接（IssueStatus 枚举更新、删除已废弃的 Task 云端 API、新增 accept/deliver）；(2b) 实现本地 SDD 流程（Issue pull → Spec 生成 → Task 拆分 → 本地管理）。

**Tech Stack:** Rust (clap + tokio + reqwest + serde + serde_yaml)

**CLI 仓库:** `/Users/yzlabmac/works/yanchat/yan-pm-cli/`，独立 Git 仓库

---

## 文件结构概览

### 要修改的文件

| 文件 | 改动 |
|------|------|
| `crates/yan-pm/src/api/types.rs` | 更新 IssueStatus 枚举，新增 Issue 字段，删除 Task 云端类型 |
| `crates/yan-pm/src/api/client.rs` | 删除 Task 云端方法，新增 accept/deliver，删除 decompose/report |
| `crates/yan-pm/src/main.rs` | 更新命令定义：删除旧命令、新增 SDD 命令 |
| `crates/yan-pm/src/cli/task.rs` | 改为读本地 tasks，不再调云端 |
| `crates/yan-pm/src/cli/issue.rs` | 新增 accept/deliver handler |
| `crates/yan-pm/src/local/directory.rs` | 新增 specs/ 和 issues/ 目录管理 |
| `crates/yan-pm/src/local/taskfile.rs` | 新增 issue 字段（spec 来源引用） |
| `crates/yan-pm/src/sync/engine.rs` | 改为只同步 Issue，不同步 Task |
| `crates/yan-pm/src/mcp/mod.rs` | 更新 MCP tools 对接新 API |

### 要新建的文件

| 文件 | 说明 |
|------|------|
| `crates/yan-pm/src/local/specfile.rs` | Spec 文件格式（frontmatter + markdown 解析） |
| `crates/yan-pm/src/local/issuefile.rs` | Issue 本地缓存文件格式 |
| `crates/yan-pm/src/cli/spec.rs` | `yan-pm spec` 命令 handler |
| `crates/yan-pm/src/cli/pull.rs` | `yan-pm pull` 命令 handler |

---

## Task 1: 更新 API 类型 — IssueStatus + Issue 新字段

**Files:**
- Modify: `crates/yan-pm/src/api/types.rs`

- [ ] **Step 1: 更新 IssueStatus 枚举**

```rust
// 替换现有 IssueStatus（约 L87-107）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    Open,
    Accepted,
    Delivered,
    Closed,
    Cancelled,
}

impl std::fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Accepted => write!(f, "accepted"),
            Self::Delivered => write!(f, "delivered"),
            Self::Closed => write!(f, "closed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}
```

- [ ] **Step 2: 更新 Issue struct 新增字段**

```rust
// 在 Issue struct 中新增字段（约 L211-229）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub project_id: String,
    pub number: i32,
    pub title: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub issue_type: IssueType,
    pub priority: TaskPriority,
    pub status: IssueStatus,
    pub labels: Vec<String>,
    // 新增字段
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub context: Option<String>,
    pub delivery_summary: Option<String>,
    pub accepted_at: Option<String>,
    pub delivered_at: Option<String>,
    pub accepted_by: Option<String>,
    // 保留字段
    pub closed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub creator_id: Option<String>,
    pub assignee_id: Option<String>,
}
```

- [ ] **Step 3: 删除已废弃的云端类型**

删除以下类型（云端已不存在）：
- `ExecutionStatus` struct
- `DecomposeResult` struct
- `ReportResult` struct
- `ExecutionReport` struct
- `HeartbeatResult` struct（task heartbeat 已删，workspace heartbeat 用 Value）

保留 `TaskListParams`、`CreateTaskData`、`UpdateTaskData` — 后续 Task 改为纯本地后再清理。

- [ ] **Step 4: 编译检查**

Run: `cd /Users/yzlabmac/works/yanchat/yan-pm-cli && cargo check 2>&1 | head -30`

预期：编译错误（引用了被删除的类型），后续 Task 修复。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "refactor: update IssueStatus enum and Issue fields for Phase 1 cloud changes"
```

---

## Task 2: 清理 API Client — 删除已废弃的 Task 云端方法

**Files:**
- Modify: `crates/yan-pm/src/api/client.rs`

- [ ] **Step 1: 删除 Task 云端方法**

从 `impl ApiClient` 中删除：
- `list_tasks` (L154-182)
- `get_task` (L184-189)
- `resolve_task_id` (L193-231)
- `create_task` (L233-242)
- `update_task` (L244-255)
- `add_comment` (L257-271)
- `lock_task` (L273-293)
- `unlock_task` (L295-299)
- `report_execution` (L302-316)
- `heartbeat`（task heartbeat，L318-327）
- `get_execution_status` (L329-336)
- `decompose_task` (L338-347)
- `force_unlock` (L349-356)
- `generate_report` (L358-362)
- `decompose_issue` (L432-443)

保留：
- 所有 Project 方法
- `list_issues`, `get_issue`, `create_issue`, `update_issue`
- 所有 Workspace 方法
- Device Code 方法

- [ ] **Step 2: 新增 accept_issue 方法**

```rust
pub async fn accept_issue(
    &self,
    project_id: &str,
    issue_id: &str,
) -> Result<Issue, ApiError> {
    validate_project_ref(project_id)?;
    validate_resource_id(issue_id, "需求 ID")?;
    self.post_empty(&format!(
        "/projects/{project_id}/issues/{issue_id}/accept"
    ))
    .await
}
```

- [ ] **Step 3: 新增 deliver_issue 方法**

```rust
pub async fn deliver_issue(
    &self,
    project_id: &str,
    issue_id: &str,
    summary: Option<&str>,
) -> Result<Issue, ApiError> {
    validate_project_ref(project_id)?;
    validate_resource_id(issue_id, "需求 ID")?;
    let body = if let Some(s) = summary {
        serde_json::json!({ "summary": s })
    } else {
        serde_json::json!({})
    };
    self.post(
        &format!("/projects/{project_id}/issues/{issue_id}/deliver"),
        &body,
    )
    .await
}
```

- [ ] **Step 4: 删除已废弃的参数类型**

从文件底部删除 `TaskListParams`、`CreateTaskData`、`UpdateTaskData`（不再需要）。

- [ ] **Step 5: 编译检查 + 提交**

Run: `cargo check 2>&1 | head -30`
修复所有引用已删除方法的编译错误（后续 Task 详细处理）。

```bash
git add -A && git commit -m "refactor: remove dead Task cloud API methods, add accept/deliver"
```

---

## Task 3: 更新 CLI 命令定义 — 删除旧命令 + 新增 SDD 命令

**Files:**
- Modify: `crates/yan-pm/src/main.rs`

- [ ] **Step 1: 删除已废弃的命令定义**

从 `enum Commands` 中删除：
- `Create` (创建云端 Task)
- `Update` (更新云端 Task)
- `Comment` (云端 Task 评论)
- `Report` (AI 项目报告)
- `DecomposeIssue` (AI 需求分解)
- `Status` (执行状态)
- `ForceUnlock` (强制解锁)

- [ ] **Step 2: 新增 Issue 子命令**

将散落的 Issue 命令整合为子命令组：

```rust
/// Issue（需求）管理
Issue {
    #[command(subcommand)]
    action: IssueAction,
},
```

```rust
#[derive(Subcommand)]
enum IssueAction {
    /// 列出需求
    List {
        /// 项目 slug 或 ID（可省略，自动检测当前工作区）
        project_id: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long = "type")]
        issue_type: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long, alias = "search")]
        keyword: Option<String>,
    },
    /// 查看需求详情
    Show {
        /// 需求编号或 ID
        issue: String,
    },
    /// 创建需求
    Create {
        #[arg(long)]
        title: String,
        #[arg(long, alias = "desc")]
        description: Option<String>,
        #[arg(long = "type")]
        issue_type: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        labels: Option<String>,
    },
    /// 认领需求（open → accepted）
    Accept {
        /// 需求编号或 ID
        issue: String,
    },
    /// 标记已交付（accepted → delivered）
    Deliver {
        /// 需求编号或 ID
        issue: String,
        /// 交付摘要（可选）
        #[arg(long)]
        summary: Option<String>,
    },
}
```

- [ ] **Step 3: 新增 SDD 命令**

```rust
/// 从云端拉取最新 Issue 到本地
Pull,

/// 为 Issue 生成技术规格（Spec）
Spec {
    /// Issue 编号
    issue_number: i32,
},

/// 从 Spec 查看/生成本地 Task
Tasks {
    /// Issue 编号（可选，不指定则显示所有本地 Task）
    issue_number: Option<i32>,
    /// 强制从本地文件读取
    #[arg(long)]
    local: bool,
},

/// 验证 Issue 实现结果
Verify {
    /// Issue 编号
    issue_number: i32,
},
```

- [ ] **Step 4: 更新 match 分发**

更新 `main()` 函数中的 `match cli.command` 分发，移除已删除命令，添加新命令的 handler 调用。

- [ ] **Step 5: 编译检查 + 提交**

```bash
cargo check 2>&1 | head -30
git add -A && git commit -m "refactor: restructure CLI commands for SDD workflow"
```

---

## Task 4: 修复 CLI Handlers — 清理引用已删除 API 的代码

**Files:**
- Modify: `crates/yan-pm/src/cli/task.rs`
- Modify: `crates/yan-pm/src/cli/issue.rs`
- Modify: `crates/yan-pm/src/cli/start.rs`
- Modify: `crates/yan-pm/src/cli/sync.rs`
- Modify: `crates/yan-pm/src/cli/mod.rs`
- Modify: `crates/yan-pm/src/sync/engine.rs`
- Modify: `crates/yan-pm/src/runner/` (如果引用了 task cloud API)
- Modify: `crates/yan-pm/src/daemon/` (如果引用了 task cloud API)
- Modify: `crates/yan-pm/src/mcp/` (如果引用了已删除 tools)

- [ ] **Step 1: 修复 cli/task.rs**

`tasks` 命令改为只读本地 `.yan-pm/tasks/` 文件：
- 删除云端 `list_tasks` 调用
- 只从 `LocalDirectory::scan_tasks()` 读取
- 如果 `.yan-pm/` 未初始化，提示用户先 `link` + `pull`

- [ ] **Step 2: 修复 cli/issue.rs**

- 添加 `accept` handler：调用 `api.accept_issue()`
- 添加 `deliver` handler：调用 `api.deliver_issue()`
- 删除 `decompose_issue` handler
- 更新 `list` 中的状态显示颜色/标签

- [ ] **Step 3: 修复 cli/start.rs**

`start` 命令当前会 lock 云端 task → 执行 → unlock。改为：
- 读取本地 `.yan-pm/tasks/` 中的 task
- 更新本地 task 文件状态 (todo → in_progress → done)
- 不再调用 `lock_task` / `unlock_task` / `report_execution` / `heartbeat`

这是一个大改动，可以先让 `start` 命令报错提示"功能重构中"，Phase 2b 再完善。

- [ ] **Step 4: 修复 sync/engine.rs**

同步引擎改为只同步 Issue：
- 删除 Task pull/push 逻辑
- 保留 Issue pull（从云端拉取 Issue 列表）
- 写入本地 `.yan-pm/issues/` 目录

- [ ] **Step 5: 修复 daemon、runner、mcp**

搜索所有引用已删除 API 方法的代码，逐个修复或暂时禁用。关键搜索：
```bash
grep -rn "list_tasks\|get_task\|create_task\|update_task\|lock_task\|unlock_task\|heartbeat\|report_execution\|decompose_task\|decompose_issue\|force_unlock\|generate_report\|get_execution_status" crates/yan-pm/src/ --include="*.rs"
```

- [ ] **Step 6: 编译通过 + 提交**

```bash
cargo check && cargo test
git add -A && git commit -m "fix: update all handlers to work with simplified cloud API"
```

---

## Task 5: Issue 本地文件 — pull 到 `.yan-pm/issues/`

**Files:**
- Create: `crates/yan-pm/src/local/issuefile.rs`
- Modify: `crates/yan-pm/src/local/directory.rs`
- Modify: `crates/yan-pm/src/local/mod.rs`
- Create: `crates/yan-pm/src/cli/pull.rs`
- Modify: `crates/yan-pm/src/cli/mod.rs`

- [ ] **Step 1: 创建 issuefile.rs — Issue 本地文件格式**

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::api::types::{IssueStatus, IssueType, TaskPriority};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueFrontmatter {
    pub id: String,
    pub number: i32,
    pub title: String,
    #[serde(rename = "type")]
    pub issue_type: IssueType,
    pub priority: TaskPriority,
    pub status: IssueStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_criteria: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    pub created: String,
    pub updated: String,
}

pub struct LocalIssueFile {
    pub frontmatter: IssueFrontmatter,
    pub body: String,
    pub file_path: std::path::PathBuf,
}

pub fn parse_issue_file(content: &str) -> Result<(IssueFrontmatter, String)> {
    // 复用 taskfile.rs 相同的 --- frontmatter --- 解析逻辑
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        anyhow::bail!("Issue file missing YAML frontmatter");
    }
    let after_first = &trimmed[3..];
    let close_idx = after_first
        .find("\n---")
        .context("Issue file missing closing ---")?;
    let yaml_str = &after_first[..close_idx].trim();
    let body_start = 3 + close_idx + 4;
    let body = if body_start < trimmed.len() {
        trimmed[body_start..].trim_start_matches(&['\r', '\n'][..]).to_string()
    } else {
        String::new()
    };
    let frontmatter: IssueFrontmatter =
        serde_yaml::from_str(yaml_str).context("Failed to parse issue YAML")?;
    Ok((frontmatter, body))
}

pub fn render_issue_file(fm: &IssueFrontmatter, body: &str) -> String {
    let yaml = serde_yaml::to_string(fm).unwrap_or_default();
    format!("---\n{}---\n\n{}", yaml, body)
}
```

- [ ] **Step 2: 扩展 directory.rs — 新增 issues/ 目录操作**

在 `LocalDirectory` 中新增：
- `init()` 时创建 `.yan-pm/issues/` 目录
- `scan_issues()` — 扫描 issues/ 下所有 .md 文件
- `write_issue(fm, body)` — 原子写入
- `pull_issues(cloud_issues)` — 从云端 Issue 列表同步到本地文件

Issue 文件命名：`{number:03d}-{slug}.md`（如 `001-oauth-sso.md`）

- [ ] **Step 3: 创建 cli/pull.rs — pull 命令 handler**

```rust
pub async fn handle_pull(api: &ApiClient, json: bool) -> Result<()> {
    // 1. 检测当前工作区关联的项目
    // 2. 调用 api.list_issues() 拉取所有 Issue
    // 3. 写入 .yan-pm/issues/ 目录
    // 4. 输出统计: created/updated/unchanged
}
```

- [ ] **Step 4: 注册 pull 命令到 main.rs**

在 `match cli.command` 中添加 `Commands::Pull => cli::pull::handle_pull(...)`.

- [ ] **Step 5: 测试 + 提交**

```bash
cargo test && cargo check
git add -A && git commit -m "feat: add Issue local files and pull command"
```

---

## Task 6: Spec 本地管理 — specfile + `yan-pm spec` 命令

**Files:**
- Create: `crates/yan-pm/src/local/specfile.rs`
- Modify: `crates/yan-pm/src/local/mod.rs`
- Modify: `crates/yan-pm/src/local/directory.rs`
- Create: `crates/yan-pm/src/cli/spec.rs`
- Modify: `crates/yan-pm/src/cli/mod.rs`

- [ ] **Step 1: 创建 specfile.rs — Spec 文件格式**

按已决策的「轻 frontmatter + 约定式 body」格式：

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecFrontmatter {
    pub issue: i32,           // 关联的 Issue 编号
    pub title: String,
    pub status: SpecStatus,   // draft → ready → in_progress → done
    pub created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecStatus {
    Draft,
    Ready,
    InProgress,
    Done,
}

impl std::fmt::Display for SpecStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Ready => write!(f, "ready"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Done => write!(f, "done"),
        }
    }
}

pub struct LocalSpecFile {
    pub frontmatter: SpecFrontmatter,
    pub body: String,
    pub file_path: std::path::PathBuf,
}

// parse_spec_file / render_spec_file 同 issuefile 模式
```

- [ ] **Step 2: 扩展 directory.rs — specs/ 目录**

新增：
- `init()` 时创建 `.yan-pm/specs/` 目录
- `scan_specs()` — 扫描 specs/ 下所有 .md
- `write_spec(fm, body)` — 原子写入
- `find_spec_by_issue(issue_number)` — 按 Issue 编号查找 Spec

Spec 文件命名：`{issue_number:03d}-{slug}.md`（如 `001-oauth-sso.md`）

- [ ] **Step 3: 创建 cli/spec.rs — spec 命令 handler**

```rust
pub async fn handle_spec(api: &ApiClient, issue_number: i32, json: bool) -> Result<()> {
    // 1. 从 .yan-pm/issues/ 读取对应 Issue
    // 2. 如果 specs/ 已有该 Issue 的 Spec，打开/显示
    // 3. 如果没有，生成初始 Spec 模板：
    //    - frontmatter: issue=N, title=issue.title, status=draft
    //    - body: ## 背景\n(issue.description)\n\n## 技术方案\n\n## 验收标准\n(AC列表)\n\n## 任务拆分\n
    // 4. 写入 .yan-pm/specs/{number}-{slug}.md
    // 5. 输出文件路径，提示用户编辑
}
```

注意：Phase 2 先不做 AI 自动生成 Spec 内容（Phase 3 的事）。先生成模板让用户手动填写。

- [ ] **Step 4: 注册到 main.rs + 编译 + 提交**

```bash
cargo check && cargo test
git add -A && git commit -m "feat: add Spec local management and template generation"
```

---

## Task 7: Task 纯本地化 — 从 Spec 读取任务

**Files:**
- Modify: `crates/yan-pm/src/cli/task.rs`
- Modify: `crates/yan-pm/src/local/taskfile.rs`
- Modify: `crates/yan-pm/src/local/directory.rs`

- [ ] **Step 1: 更新 TaskFrontmatter — 新增 spec 来源字段**

```rust
pub struct TaskFrontmatter {
    // 移除 id（不再有云端 ID）
    pub number: Option<i32>,
    pub title: String,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<i32>,       // 改为 issue number（不是 ID）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    pub created: String,
    pub updated: String,
}
```

- [ ] **Step 2: 更新 tasks 命令 — 按 Issue 筛选**

`tasks` 命令新增 issue_number 参数：
- `yan-pm tasks` — 显示所有本地 task
- `yan-pm tasks --issue 1` 或 `yan-pm tasks 1` — 显示 Issue #1 的 tasks

读取 `.yan-pm/tasks/` 下文件，按 `issue` frontmatter 字段筛选。

Task 文件命名保持：`{issue_number}-{seq}-{slug}.md`（如 `001-01-setup-oauth.md`）

- [ ] **Step 3: 编译 + 提交**

```bash
cargo check && cargo test
git add -A && git commit -m "feat: make Tasks purely local, filter by Issue"
```

---

## Task 8: 全量编译 + 集成测试 + 清理

**Files:**
- All modified files

- [ ] **Step 1: 全量编译**

```bash
cargo build 2>&1
```

修复所有编译错误。

- [ ] **Step 2: 运行现有测试**

```bash
cargo test 2>&1
```

修复失败的测试。

- [ ] **Step 3: 手动验证核心流程**

```bash
# 确认基本命令可用
cargo run -- --help
cargo run -- issue --help
cargo run -- pull --help
cargo run -- spec --help

# 如果有已 link 的项目
cargo run -- pull
cargo run -- issue list
cargo run -- tasks
```

- [ ] **Step 4: 清理 unused imports 和 dead code**

```bash
cargo clippy 2>&1 | grep "unused\|dead_code" | head -20
```

- [ ] **Step 5: 最终提交**

```bash
git add -A && git commit -m "chore: cleanup dead code and fix integration after Phase 2"
```

---

## 风险和注意事项

| 风险 | 应对 |
|------|------|
| `start` 命令深度依赖 Task cloud API | Task 4 Step 3 暂时让它报错提示重构中，Phase 3 再完善 |
| daemon/auto-runner 依赖 Task 同步 | 暂时禁用 auto-runner 中的 task 相关逻辑 |
| MCP tools 需要对齐云端 8 个 tool | Task 4 Step 5 中处理 |
| `sync` 命令逻辑大改 | Task 4 Step 4 改为只同步 Issue |
| 现有本地 task 文件格式不兼容 | `id` 字段改为 optional（向后兼容） |

## 后续（Phase 3）

- `yan-pm spec` AI 自动生成 Spec 内容
- `yan-pm run` 调度 AI Agent 执行本地 Task
- 多 Agent 并行调度
- 交付验证自动化
- `yan-pm verify` 对照 acceptance criteria 验证
