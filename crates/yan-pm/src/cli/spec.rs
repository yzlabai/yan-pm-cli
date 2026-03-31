use anyhow::Result;

use crate::local::directory::LocalDirectory;
use crate::local::specfile::{SpecFrontmatter, SpecStatus};

pub async fn handle_spec(issue_number: i32, json: bool, ai: bool, agent_name: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let local_dir = LocalDirectory::new(&cwd);

    if !local_dir.is_initialized() {
        anyhow::bail!("当前目录未初始化。请先运行: yan pull");
    }

    // Check if spec already exists
    if let Some(spec) = local_dir.find_spec_by_issue(issue_number)? {
        if json {
            println!(
                "{}",
                serde_json::json!({ "path": spec.file_path.display().to_string(), "exists": true })
            );
        } else {
            println!("Spec 已存在: {}", spec.file_path.display());
            println!("{}", spec.body);
        }
        return Ok(());
    }

    // Find the issue
    let issues = local_dir.scan_issues()?;
    let issue = issues
        .iter()
        .find(|i| i.frontmatter.number == issue_number)
        .ok_or_else(|| anyhow::anyhow!("Issue #{} 不存在。请先运行: yan pull", issue_number))?;

    if ai {
        handle_ai_spec(&cwd, &local_dir, issue, agent_name, json).await
    } else {
        handle_template_spec(&local_dir, issue, json)
    }
}

/// Generate spec using a template (original behavior)
fn handle_template_spec(
    local_dir: &LocalDirectory,
    issue: &crate::local::issuefile::LocalIssueFile,
    json: bool,
) -> Result<()> {
    let fm = &issue.frontmatter;

    let spec_fm = SpecFrontmatter {
        issue: fm.number,
        title: fm.title.clone(),
        status: SpecStatus::Draft,
        created: chrono::Utc::now().to_rfc3339(),
        updated: None,
    };

    let mut body = String::new();
    body.push_str("## 背景\n\n");
    if !issue.body.is_empty() {
        body.push_str(&issue.body);
        if !issue.body.ends_with('\n') {
            body.push('\n');
        }
        body.push('\n');
    }
    body.push_str("## 技术方案\n\n\n\n");
    body.push_str("## 验收标准\n\n");
    for ac in &fm.acceptance_criteria {
        body.push_str(&format!("- [ ] {}\n", ac));
    }
    if fm.acceptance_criteria.is_empty() {
        body.push_str("- [ ] \n");
    }
    body.push_str("\n## 任务拆分\n\n");
    body.push_str("- [ ] \n");

    let path = local_dir.write_spec(&spec_fm, &body)?;

    if json {
        println!(
            "{}",
            serde_json::json!({ "path": path.display().to_string() })
        );
    } else {
        println!("✓ Spec 已生成: {}", path.display());
        println!("  请编辑此文件填写技术方案和任务拆分。");
    }
    Ok(())
}

/// Generate spec using an AI agent via ACP
async fn handle_ai_spec(
    cwd: &std::path::Path,
    local_dir: &LocalDirectory,
    issue: &crate::local::issuefile::LocalIssueFile,
    agent_name: &str,
    json: bool,
) -> Result<()> {
    use crate::agent::registry::find_backend;
    use crate::agent::session::{execute_agent, AgentOptions};

    let backend = find_backend(agent_name).ok_or_else(|| {
        anyhow::anyhow!("Agent '{}' 未找到。可用: claude, codex, gemini", agent_name)
    })?;

    let fm = &issue.frontmatter;
    let now = chrono::Utc::now().to_rfc3339();

    // Build the spec file path
    let spec_filename = crate::local::specfile::spec_filename(fm.number, &fm.title);
    let spec_path = cwd.join(".yan-pm").join("specs").join(&spec_filename);

    // Read project context files if they exist
    let mut context = String::new();
    for ctx_file in &["CLAUDE.md", "README.md", "README"] {
        let path = cwd.join(ctx_file);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                context.push_str(&format!("\n## 项目上下文 ({})\n{}\n", ctx_file, content));
            }
        }
    }

    // Build acceptance criteria string
    let ac_str = if fm.acceptance_criteria.is_empty() {
        "(无)".to_string()
    } else {
        fm.acceptance_criteria
            .iter()
            .map(|ac| format!("- {}", ac))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let prompt = format!(
        r#"你是一个资深软件架构师。请为以下需求生成技术规格（Spec）。

## 需求
- 编号: #{number}
- 标题: {title}
- 描述: {description}
- 验收标准:
{acceptance_criteria}
{context}
## 输出要求
请将 Spec 写入文件 `{spec_path}`，格式如下：

```
---
issue: {number}
title: "{title}"
status: draft
created: "{now}"
---

## 背景
(从需求描述中提取)

## 技术方案
(详细的技术实现方案)

## 验收标准
(checkbox 格式)

## 任务拆分
(checkbox 格式，标注 [P] 可并行 / [D:xx] 有依赖)
```

请确保文件格式正确，YAML frontmatter 用 --- 包围。直接写入文件，不需要额外确认。"#,
        number = fm.number,
        title = fm.title,
        description = if issue.body.is_empty() {
            "(无描述)"
        } else {
            &issue.body
        },
        acceptance_criteria = ac_str,
        context = if context.is_empty() {
            String::new()
        } else {
            format!("\n## 项目上下文\n{}\n", context)
        },
        spec_path = spec_path.display(),
        now = now,
    );

    if !json {
        println!("🤖 使用 {} 生成 Spec...", agent_name);
    }

    let options = AgentOptions {
        cwd: cwd.to_string_lossy().to_string(),
        prompt,
        max_budget_usd: None,
        permission_mode: Some("auto".to_string()),
        allowed_tools: None,
        mcp_configs: None,
        model: None,
        verbose: false,
    };

    let result = execute_agent(backend.as_ref(), options, None).await?;

    if result.success {
        // Check if the spec file was actually created
        if let Some(spec) = local_dir.find_spec_by_issue(fm.number)? {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "path": spec.file_path.display().to_string(),
                        "ai": true,
                        "agent": agent_name,
                    })
                );
            } else {
                println!("✓ AI Spec 已生成: {}", spec.file_path.display());
                println!("  请检查并确认内容。");
            }
        } else if spec_path.exists() {
            // File exists but may not parse — still report it
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "path": spec_path.display().to_string(),
                        "ai": true,
                        "agent": agent_name,
                        "warning": "文件已创建但可能格式不正确",
                    })
                );
            } else {
                println!("⚠ Spec 文件已创建但格式可能不正确: {}", spec_path.display());
                println!("  请手动检查文件格式。");
            }
        } else {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "error": "Agent 完成但未创建 Spec 文件",
                        "summary": result.summary,
                    })
                );
            } else {
                println!("⚠ Agent 完成但未创建 Spec 文件。");
                println!(
                    "  Agent 输出: {}",
                    &result.summary[..result.summary.len().min(500)]
                );
            }
        }
    } else {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "error": "Agent 执行失败",
                    "summary": result.summary,
                    "exit_code": result.exit_code,
                })
            );
        } else {
            eprintln!(
                "❌ Agent 执行失败: {}",
                &result.summary[..result.summary.len().min(500)]
            );
        }
        anyhow::bail!("AI Spec 生成失败");
    }

    Ok(())
}
