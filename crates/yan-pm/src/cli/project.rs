use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::api::types::UpdateProjectData;
use crate::config;
use crate::output;
use super::detect;
use super::make_client;

pub async fn list(url: Option<&str>, token: Option<&str>, json: bool) -> Result<()> {
    let client = make_client(url, token)?;
    let projects = client.list_projects().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&projects)?);
    } else {
        output::print_projects(&projects);
    }
    Ok(())
}

pub async fn report(url: Option<&str>, token: Option<&str>, json: bool, project_id: &str) -> Result<()> {
    let client = make_client(url, token)?;
    let report = client.generate_report(project_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.report);
    }
    Ok(())
}

pub async fn sync_info(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    yes: bool,
    dry_run: bool,
) -> Result<()> {
    // 1. 从 workspace link 获取 project_id
    let cwd = std::env::current_dir().context("无法获取当前目录")?;
    let link = config::find_workspace_link(Some(&cwd))
        .ok_or_else(|| anyhow::anyhow!("当前目录未关联项目。请先运行 `yan-pm link <project-id>`"))?;

    let client = make_client(url, token)?;

    // 2. 获取云端项目信息
    let project = client.get_project(&link.project_id).await?;

    // 3. 本地检测
    let detected = detect::detect(&cwd)?;

    // 4. 计算 diff
    let mut diffs: Vec<FieldDiff> = Vec::new();

    // repoUrl
    if let Some(ref local_url) = detected.repo_url {
        let remote = project.project.repo_url.as_deref().unwrap_or("");
        if remote != local_url {
            diffs.push(FieldDiff {
                name: "repoUrl".to_string(),
                current: if remote.is_empty() { "(空)".to_string() } else { remote.to_string() },
                detected: local_url.clone(),
            });
        }
    }

    // techStack
    if !detected.tech_stack.is_empty() {
        let remote_stack = get_remote_tech_stack(&project.project.settings);
        let detected_str = detected.tech_stack.join(", ");
        let remote_str = remote_stack.join(", ");
        if remote_str != detected_str {
            diffs.push(FieldDiff {
                name: "techStack".to_string(),
                current: if remote_str.is_empty() { "(空)".to_string() } else { remote_str },
                detected: detected_str,
            });
        }
    }

    // AI 上下文 (customAiPrompt)
    if let (Some(ref ai_ctx), Some(ref source)) = (&detected.ai_context, &detected.ai_context_source) {
        let remote_prompt = get_remote_custom_prompt(&project.project.settings);
        if remote_prompt.as_deref() != Some(ai_ctx.as_str()) {
            let char_count = ai_ctx.chars().count();
            diffs.push(FieldDiff {
                name: "AI 上下文 (customAiPrompt)".to_string(),
                current: match &remote_prompt {
                    Some(p) => format!("已有 ({} 字)", p.chars().count()),
                    None => "(空)".to_string(),
                },
                detected: format!("从 {} 读取 ({} 字)", source, char_count),
            });
        }
    }

    // 5. 展示结果
    if diffs.is_empty() {
        if json {
            println!(r#"{{"status":"up_to_date","changes":0}}"#);
        } else {
            println!("📦 项目：{}", project.project.name);
            println!("✅ 云端信息已是最新，无需同步");
        }
        return Ok(());
    }

    if json {
        let diff_json: Vec<serde_json::Value> = diffs.iter().map(|d| {
            serde_json::json!({
                "field": d.name,
                "current": d.current,
                "detected": d.detected,
            })
        }).collect();
        if dry_run {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "status": "dry_run",
                "changes": diff_json,
            }))?);
            return Ok(());
        }
    } else {
        println!("📦 项目：{} ({})\n", project.project.name, project.project.slug);
        println!("检测到以下信息可同步到云端：\n");
        for d in &diffs {
            println!("  {}:", d.name);
            println!("    当前: {}", d.current);
            println!("    检测: {}", d.detected);
            println!();
        }
    }

    if dry_run {
        if !json {
            println!("(--dry-run 模式，不执行上传)");
        }
        return Ok(());
    }

    // 6. 确认
    if !yes {
        print!("是否同步到云端? [Y/n] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let answer = input.trim().to_lowercase();
        if answer == "n" || answer == "no" {
            println!("已取消");
            return Ok(());
        }
    }

    // 7. 构建 payload 上传
    let mut settings = serde_json::Map::new();
    let mut update = UpdateProjectData::default();
    update.sync_source = Some("cli".to_string());

    for d in &diffs {
        match d.name.as_str() {
            "repoUrl" => {
                update.repo_url = detected.repo_url.clone();
            }
            "techStack" => {
                settings.insert(
                    "techStack".to_string(),
                    serde_json::json!(detected.tech_stack),
                );
            }
            _ if d.name.contains("customAiPrompt") => {
                if let Some(ref ctx) = detected.ai_context {
                    settings.insert("customAiPrompt".to_string(), serde_json::json!(ctx));
                }
            }
            _ => {}
        }
    }

    if !settings.is_empty() {
        update.settings = Some(serde_json::Value::Object(settings));
    }

    client.update_project(&link.project_id, &update).await?;

    let count = diffs.len();
    if json {
        println!(r#"{{"status":"synced","changes":{count}}}"#);
    } else {
        println!("✅ 已同步 {} 个字段", count);
    }

    Ok(())
}

struct FieldDiff {
    name: String,
    current: String,
    detected: String,
}

fn get_remote_tech_stack(settings: &Option<serde_json::Value>) -> Vec<String> {
    settings
        .as_ref()
        .and_then(|s| s.get("techStack"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn get_remote_custom_prompt(settings: &Option<serde_json::Value>) -> Option<String> {
    settings
        .as_ref()
        .and_then(|s| s.get("customAiPrompt"))
        .and_then(|v| v.as_str())
        .map(String::from)
}
