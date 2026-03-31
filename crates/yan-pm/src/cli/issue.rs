use anyhow::Result;

use super::make_client;
use crate::api::client::{CreateIssueData, IssueListParams, UpdateIssueData};
use crate::output;
use crate::IssueAction;

pub async fn handle(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    action: IssueAction,
) -> Result<()> {
    match action {
        IssueAction::List {
            project_id,
            status,
            issue_type,
            priority,
            keyword,
        } => {
            list(
                url,
                token,
                json,
                &project_id,
                status.as_deref(),
                issue_type.as_deref(),
                priority.as_deref(),
                keyword.as_deref(),
            )
            .await
        }
        IssueAction::Show {
            project_id,
            issue_id,
        } => show(url, token, json, &project_id, &issue_id).await,
        IssueAction::Create {
            project_id,
            title,
            description,
            issue_type,
            priority,
            assignee,
            labels,
        } => {
            create(
                url,
                token,
                json,
                &project_id,
                &title,
                description.as_deref(),
                issue_type.as_deref(),
                priority.as_deref(),
                assignee.as_deref(),
                labels.as_deref(),
            )
            .await
        }
        IssueAction::Update {
            project_id,
            issue_id,
            title,
            status,
            priority,
            issue_type,
            assignee,
            labels,
        } => {
            update(
                url,
                token,
                json,
                &project_id,
                &issue_id,
                title.as_deref(),
                status.as_deref(),
                priority.as_deref(),
                issue_type.as_deref(),
                assignee.as_deref(),
                labels.as_deref(),
            )
            .await
        }
        IssueAction::Accept {
            project_id,
            issue_id,
        } => handle_accept(url, token, json, &project_id, &issue_id).await,
        IssueAction::Deliver {
            project_id,
            issue_id,
            summary,
        } => handle_deliver(url, token, json, &project_id, &issue_id, summary.as_deref()).await,
    }
}

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

pub async fn show(
    url: Option<&str>,
    token: Option<&str>,
    _json: bool,
    project_id: &str,
    issue_id: &str,
) -> Result<()> {
    let client = make_client(url, token)?;
    let issue = client.get_issue(project_id, issue_id).await?;
    println!("{}", serde_json::to_string_pretty(&issue)?);
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

pub async fn handle_accept(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    issue_id: &str,
) -> Result<()> {
    let client = make_client(url, token)?;
    let issue = client.accept_issue(project_id, issue_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&issue)?);
    } else {
        println!("✓ 需求已接受: {} [#{}]", issue.title, issue.number);
    }
    Ok(())
}

pub async fn handle_deliver(
    url: Option<&str>,
    token: Option<&str>,
    json: bool,
    project_id: &str,
    issue_id: &str,
    summary: Option<&str>,
) -> Result<()> {
    let client = make_client(url, token)?;
    let issue = client.deliver_issue(project_id, issue_id, summary).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&issue)?);
    } else {
        println!("✓ 需求已交付: {} [#{}]", issue.title, issue.number);
    }
    Ok(())
}
