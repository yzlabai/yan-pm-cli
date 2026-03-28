---
name: yan-pm
description: |
  YanChat 项目管理助手。当用户提到以下场景时激活：
  查看项目、查看任务、我的待办、开始任务、领取任务、更新任务状态、
  添加评论、任务进度、项目报告、完成任务、标记完成。
  Triggers: "my tasks", "project tasks", "check todos", "start working on",
  "update task", "mark done", "project report", "what should I work on"
---

# YanChat 项目管理助手

> 此 Skill 由 `yan-pm-cli setup` 自动安装。更新: `yan-pm-cli setup --target claude`

通过 MCP 连接 YanChat 云端，管理项目任务。支持查询任务、领取任务、更新状态、添加评论、生成报告。

## 可用 MCP Tools

| Tool | 用途 | 关键参数 |
|------|------|----------|
| `list_projects` | 列出我的项目 | 无 |
| `get_project` | 项目详情+成员 | `projectId` |
| `list_tasks` | 任务列表（可筛选） | `projectId`, `status?`, `keyword?` |
| `create_task` | 创建任务 | `projectId`, `title`, `description?`, `type?`, `priority?`, `assigneeId?` |
| `update_task` | 更新任务 | `projectId`, `taskId`, `title?`, `status?`, `priority?`, `assigneeId?` |
| `add_comment` | 添加评论 | `projectId`, `taskId`, `content` |
| `get_report` | AI 项目报告 | `projectId` |
| `decompose_task` | AI 拆解复杂任务为子任务 | `projectId`, `taskId` |
| `list_issues` | 列出需求 | `projectId`, `status?`, `type?`, `keyword?` |
| `get_issue` | 需求详情（含关联任务+进度） | `projectId`, `issueId` |
| `create_issue` | 创建需求 | `projectId`, `title`, `description?`, `type?`, `priority?` |
| `update_issue` | 更新需求 | `projectId`, `issueId`, `title?`, `status?`, `priority?` |
| `decompose_issue` | AI 需求分解为任务 | `projectId`, `issueId` |

## 工作流

### 流程一：查看待办任务

1. 调用 `list_projects` 获取项目列表
2. 调用 `list_tasks(projectId, status="todo")` 获取待办任务
3. 按优先级排列展示，建议用户下一步操作

### 流程二：领取并开始任务

1. 用户指定（或推荐优先级最高的）待办任务
2. 调用 `update_task(projectId, taskId, status="in_progress")` 标记为进行中
3. 阅读任务描述，分析需求
4. 开始编码实现

### 流程三：完成任务并报告

编码完成且测试通过后：

1. 调用 `update_task(projectId, taskId, status="done")` 标记为完成
2. 调用 `add_comment(projectId, taskId, content="...")` 添加完成说明
   - 说明应包含：完成了什么、修改了哪些文件、关键决策
3. 如果用户要求，调用 `get_report(projectId)` 生成项目整体报告

### 流程四：完整 Agent Loop（用户监督）

用户说"开始处理我的下一个任务"时执行完整循环：

1. `list_projects` → 选择项目（或使用用户指定的项目）
2. `list_tasks(status="todo")` → 选取最高优先级任务
3. `update_task(status="in_progress")` → 标记为进行中
4. 分析任务描述 → 阅读相关代码 → 实现功能
5. 运行测试验证
6. `update_task(status="done")` → 标记为完成
7. `add_comment` → 添加实现总结

每一步都向用户汇报进展，等待确认后再继续。

### 流程五：拆解复杂任务

用户说"这个任务太大了，拆一下"或"分解一下这个任务"时：

1. `list_tasks` → 找到目标任务
2. `decompose_task(projectId, taskId)` → AI 自动拆解为子任务
3. 展示生成的子任务列表，让用户确认
4. 建议用户按优先级逐个处理子任务

## 任务状态流转

```
todo → in_progress → done
                  ↘ cancelled
```

- `todo`：待开始
- `in_progress`：进行中（已领取）
- `done`：已完成
- `cancelled`：已取消

## 任务类型

- `feature`：新功能
- `bug`：缺陷修复
- `improvement`：改进优化
- `task`：一般任务

## 优先级

- `urgent`：紧急
- `high`：高
- `medium`：中（默认）
- `low`：低

## 最佳实践

1. **先查再做**：开始前先 `list_tasks` 了解全局，避免重复工作
2. **及时更新**：开始编码时立即更新状态为 `in_progress`
3. **详细评论**：完成后的评论应包含具体改动内容，便于审核
4. **小步提交**：每完成一个子目标就添加评论记录进展
5. **状态同步**：遇到阻塞时添加评论说明原因，便于团队协作
