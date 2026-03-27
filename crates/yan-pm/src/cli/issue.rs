use anyhow::Result;

use super::make_client;
use crate::api::client::{CreateIssueData, IssueListParams, UpdateIssueData};
use crate::output;

pub async fn list(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    status: Option<&str>,
    issue_type: Option<&str>,
    priority: Option<&str>,
    keyword: Option<&str>,
) -> Result<()> {
    let client = make_client(url, token)?;
    let params = IssueListParams {
        status: status.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        issue_type: issue_type.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        priority: priority.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        assignee_id: None,
        search: keyword.map(String::from),
    };
    let issues = client.list_issues(project_id, &params).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&issues)?);
    } else {
        output::print_issues(&issues);
    }
    Ok(())
}

pub async fn create(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    title: &str,
    description: Option<&str>,
    issue_type: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    labels: Option<&str>,
) -> Result<()> {
    let client = make_client(url, token)?;
    let labels_vec = labels.map(|l| l.split(',').map(|s| s.trim().to_string()).collect());
    let data = CreateIssueData {
        title: title.into(),
        description: description.map(String::from),
        issue_type: issue_type.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        priority: priority.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        assignee_id: assignee.map(String::from),
        labels: labels_vec,
    };
    let issue = client.create_issue(project_id, &data).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&issue)?);
    } else {
        println!(
            "✓ 需求已创建: {} [{}]",
            issue.title,
            &issue.id[..8.min(issue.id.len())]
        );
    }
    Ok(())
}

pub async fn update(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    issue_id: &str,
    title: Option<&str>,
    status: Option<&str>,
    priority: Option<&str>,
    issue_type: Option<&str>,
    assignee: Option<&str>,
    labels: Option<&str>,
) -> Result<()> {
    let client = make_client(url, token)?;
    let labels_vec = labels.map(|l| l.split(',').map(|s| s.trim().to_string()).collect());
    let data = UpdateIssueData {
        title: title.map(String::from),
        status: status.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        priority: priority.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        issue_type: issue_type.and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        assignee_id: assignee.map(String::from),
        labels: labels_vec,
    };
    let issue = client.update_issue(project_id, issue_id, &data).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&issue)?);
    } else {
        println!("✓ 需求已更新: {}", issue.title);
    }
    Ok(())
}

pub async fn decompose(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    issue_id: &str,
) -> Result<()> {
    let client = make_client(url, token)?;
    let result = client.decompose_issue(project_id, issue_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("✓ 需求已分解，生成 {} 个任务", result.tasks.len());
        for task in &result.tasks {
            println!("  - {} [{}]", task.title, &task.id[..8.min(task.id.len())]);
        }
    }
    Ok(())
}
