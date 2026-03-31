use anyhow::Result;

use crate::local::directory::LocalDirectory;
use crate::local::specfile::{SpecFrontmatter, SpecStatus};

pub async fn handle_spec(issue_number: i32, json: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let local_dir = LocalDirectory::new(&cwd);

    if !local_dir.is_initialized() {
        anyhow::bail!("当前目录未初始化。请先运行: yan-pm pull");
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
        .ok_or_else(|| anyhow::anyhow!("Issue #{} 不存在。请先运行: yan-pm pull", issue_number))?;

    let fm = &issue.frontmatter;

    // Generate spec template
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
