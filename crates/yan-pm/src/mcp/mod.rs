use std::io::{self, Write};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::{interval, Duration};

use crate::api::client::*;
use crate::config;

/// JSON-RPC 2.0 request
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// MCP Tool definition
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolDef {
    name: String,
    description: String,
    input_schema: Value,
}

fn make_tool(name: &str, desc: &str, props: Value, required: Vec<&str>) -> ToolDef {
    ToolDef {
        name: name.into(),
        description: desc.into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": props,
            "required": required,
        }),
    }
}

fn tool_definitions() -> Vec<ToolDef> {
    vec![
        make_tool(
            "list_projects",
            "列出当前用户参与的所有项目",
            serde_json::json!({}),
            vec![],
        ),
        make_tool(
            "get_project",
            "获取项目详情，包括成员列表",
            serde_json::json!({
                "projectId": { "type": "string", "description": "项目 slug 或 ID" }
            }),
            vec!["projectId"],
        ),
        make_tool(
            "get_issue",
            "获取需求详情",
            serde_json::json!({
                "projectId": { "type": "string", "description": "项目 slug 或 ID" },
                "issueId": { "type": "string", "description": "需求 ID" }
            }),
            vec!["projectId", "issueId"],
        ),
        make_tool(
            "list_issues",
            "列出项目需求",
            serde_json::json!({
                "projectId": { "type": "string", "description": "项目 slug 或 ID" },
                "status": { "type": "string", "enum": ["open", "accepted", "delivered", "closed", "cancelled"] },
                "type": { "type": "string", "enum": ["feature", "bug", "improvement", "question"] },
                "keyword": { "type": "string", "description": "按标题关键词搜索" }
            }),
            vec!["projectId"],
        ),
        make_tool(
            "create_issue",
            "创建需求",
            serde_json::json!({
                "projectId": { "type": "string", "description": "项目 slug 或 ID" },
                "title": { "type": "string", "description": "需求标题" },
                "description": { "type": "string" },
                "type": { "type": "string", "enum": ["feature", "bug", "improvement", "question"] },
                "priority": { "type": "string", "enum": ["urgent", "high", "medium", "low"] },
                "assigneeId": { "type": "string" },
                "labels": { "type": "array", "items": { "type": "string" } }
            }),
            vec!["projectId", "title"],
        ),
        make_tool(
            "update_issue",
            "更新需求",
            serde_json::json!({
                "projectId": { "type": "string", "description": "项目 slug 或 ID" },
                "issueId": { "type": "string", "description": "需求 ID" },
                "title": { "type": "string" },
                "status": { "type": "string", "enum": ["open", "accepted", "delivered", "closed", "cancelled"] },
                "priority": { "type": "string", "enum": ["urgent", "high", "medium", "low"] },
                "type": { "type": "string", "enum": ["feature", "bug", "improvement", "question"] },
                "assigneeId": { "type": "string" },
                "labels": { "type": "array", "items": { "type": "string" } }
            }),
            vec!["projectId", "issueId"],
        ),
        make_tool(
            "accept_issue",
            "接受需求",
            serde_json::json!({
                "projectId": { "type": "string", "description": "项目 slug 或 ID" },
                "issueId": { "type": "string", "description": "需求 ID" }
            }),
            vec!["projectId", "issueId"],
        ),
        make_tool(
            "deliver_issue",
            "交付需求",
            serde_json::json!({
                "projectId": { "type": "string", "description": "项目 slug 或 ID" },
                "issueId": { "type": "string", "description": "需求 ID" },
                "summary": { "type": "string", "description": "交付总结" }
            }),
            vec!["projectId", "issueId"],
        ),
    ]
}

fn ok_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(result),
        error: None,
    }
}

fn err_response(id: Value, code: i32, msg: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: msg.into(),
        }),
    }
}

fn tool_result(data: &impl Serialize) -> Value {
    serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".into())
        }]
    })
}

fn tool_error(msg: &str) -> Value {
    serde_json::json!({
        "content": [{
            "type": "text",
            "text": msg
        }],
        "isError": true
    })
}

fn get_str(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn require_str(params: &Value, key: &str) -> Result<String, String> {
    get_str(params, key).ok_or_else(|| format!("Missing required parameter: {key}"))
}

async fn handle_tool_call(client: &ApiClient, name: &str, params: &Value) -> Value {
    let result: Result<Value, String> = async {
        match name {
            "list_projects" => client
                .list_projects()
                .await
                .map(|v| tool_result(&v))
                .map_err(|e| e.to_string()),

            "get_project" => {
                let pid = require_str(params, "projectId")?;
                client
                    .get_project(&pid)
                    .await
                    .map(|v| tool_result(&v))
                    .map_err(|e| e.to_string())
            }

            "get_issue" => {
                let pid = require_str(params, "projectId")?;
                let iid = require_str(params, "issueId")?;
                client
                    .get_issue(&pid, &iid)
                    .await
                    .map(|v| tool_result(&v))
                    .map_err(|e| e.to_string())
            }

            "list_issues" => {
                let pid = require_str(params, "projectId")?;
                let status = get_str(params, "status")
                    .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok());
                let issue_type = get_str(params, "type")
                    .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok());
                let keyword = get_str(params, "keyword");
                client
                    .list_issues(
                        &pid,
                        &IssueListParams {
                            status,
                            issue_type,
                            search: keyword,
                            ..Default::default()
                        },
                    )
                    .await
                    .map(|v| tool_result(&v))
                    .map_err(|e| e.to_string())
            }

            "create_issue" => {
                let pid = require_str(params, "projectId")?;
                let title = require_str(params, "title")?;
                let labels = params.get("labels").and_then(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                });
                let data = CreateIssueData {
                    title,
                    description: get_str(params, "description"),
                    issue_type: get_str(params, "type")
                        .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
                    priority: get_str(params, "priority")
                        .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
                    assignee_id: get_str(params, "assigneeId"),
                    labels,
                };
                client
                    .create_issue(&pid, &data)
                    .await
                    .map(|v| tool_result(&v))
                    .map_err(|e| e.to_string())
            }

            "update_issue" => {
                let pid = require_str(params, "projectId")?;
                let iid = require_str(params, "issueId")?;
                let labels = params.get("labels").and_then(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                });
                let data = crate::api::UpdateIssueData {
                    title: get_str(params, "title"),
                    status: get_str(params, "status")
                        .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
                    priority: get_str(params, "priority")
                        .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
                    issue_type: get_str(params, "type")
                        .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
                    assignee_id: get_str(params, "assigneeId"),
                    labels,
                };
                client
                    .update_issue(&pid, &iid, &data)
                    .await
                    .map(|v| tool_result(&v))
                    .map_err(|e| e.to_string())
            }

            "accept_issue" => {
                let pid = require_str(params, "projectId")?;
                let iid = require_str(params, "issueId")?;
                client
                    .accept_issue(&pid, &iid)
                    .await
                    .map(|v| tool_result(&v))
                    .map_err(|e| e.to_string())
            }

            "deliver_issue" => {
                let pid = require_str(params, "projectId")?;
                let iid = require_str(params, "issueId")?;
                let summary = get_str(params, "summary");
                client
                    .deliver_issue(&pid, &iid, summary.as_deref())
                    .await
                    .map(|v| tool_result(&v))
                    .map_err(|e| e.to_string())
            }

            _ => Err(format!("Unknown tool: {name}")),
        }
    }
    .await;

    match result {
        Ok(v) => v,
        Err(e) => tool_error(&e),
    }
}

/// Try to create an API client from current config. Returns None if not configured.
fn try_create_client() -> Option<ApiClient> {
    let resolved = config::resolve_config(None, None);
    if resolved.base_url.is_empty() || resolved.token.is_empty() {
        return None;
    }
    ApiClient::new(&resolved.base_url, &resolved.token).ok()
}

/// Start the MCP stdio server (blocking, reads from stdin)
pub async fn start_mcp_server() -> Result<()> {
    // Lazily initialized: allow startup without token so login can happen mid-session
    let mut client = try_create_client();

    // Best-effort startup validation + heartbeat
    let mut heartbeat_project_id: Option<String> = None;
    let mut heartbeat_workspace_id: Option<String> = None;
    if let Some(ref c) = client {
        if c.list_projects().await.is_ok() {
            let workspace_id = start_workspace_heartbeat(c).await;
            heartbeat_project_id = workspace_id.as_ref().map(|(p, _)| p.clone());
            heartbeat_workspace_id = workspace_id.as_ref().map(|(_, w)| w.clone());
        } else {
            // Token invalid, clear client so it will be re-read on next request
            client = None;
        }
    }

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let stdout = io::stdout();

    // Heartbeat every 2 minutes
    let mut heartbeat_timer = interval(Duration::from_secs(120));
    heartbeat_timer.tick().await; // consume the immediate first tick

    loop {
        tokio::select! {
            line_result = lines.next_line() => {
                let line = match line_result? {
                    Some(l) => l,
                    None => break, // EOF
                };
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                let req: JsonRpcRequest = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(e) => {
                        let resp = err_response(Value::Null, -32700, &format!("Parse error: {e}"));
                        let json = serde_json::to_string(&resp)?;
                        let mut out = stdout.lock();
                        writeln!(out, "{json}")?;
                        out.flush()?;
                        continue;
                    }
                };

                // JSON-RPC 2.0: notifications have no id — must not send a response
                if req.id.is_none() {
                    // Silently drop notifications (e.g. notifications/cancelled, notifications/progress)
                    continue;
                }
                let id = req.id.clone().unwrap_or(Value::Null);

                // Validate JSON-RPC version (spec §4.1)
                if req.jsonrpc != "2.0" {
                    let resp = err_response(id, -32600, "Invalid Request: jsonrpc must be \"2.0\"");
                    let json = serde_json::to_string(&resp)?;
                    let mut out = stdout.lock();
                    writeln!(out, "{json}")?;
                    out.flush()?;
                    continue;
                }

                let resp = match req.method.as_str() {
                    "initialize" => ok_response(
                        id,
                        serde_json::json!({
                            "protocolVersion": "2024-11-05",
                            "capabilities": {
                                "tools": {}
                            },
                            "serverInfo": {
                                "name": "yan-pm",
                                "version": env!("CARGO_PKG_VERSION")
                            }
                        }),
                    ),

                    "notifications/initialized" => ok_response(id, serde_json::json!({})),

                    "ping" => ok_response(id, serde_json::json!({})),

                    "tools/list" => {
                        let tools = tool_definitions();
                        ok_response(id, serde_json::json!({ "tools": tools }))
                    }

                    "tools/call" => {
                        // Lazily refresh client from config (supports login mid-session)
                        if client.is_none() {
                            client = try_create_client();
                        }

                        if let Some(ref c) = client {
                            let params = req.params.unwrap_or(Value::Null);
                            let tool_name = params
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let tool_args = params
                                .get("arguments")
                                .cloned()
                                .unwrap_or(serde_json::json!({}));

                            let result = handle_tool_call(c, tool_name, &tool_args).await;
                            ok_response(id, result)
                        } else {
                            err_response(id, -32000, "未登录。请先在终端运行 `yan-pm-cli login`，然后重试。")
                        }
                    }

                    _ => err_response(id, -32601, &format!("Method not found: {}", req.method)),
                };

                let json = serde_json::to_string(&resp)?;
                let mut out = stdout.lock();
                writeln!(out, "{json}")?;
                out.flush()?;
            }
            _ = heartbeat_timer.tick() => {
                // Lazily start heartbeat if client became available after login
                if heartbeat_project_id.is_none() {
                    if client.is_none() {
                        client = try_create_client();
                    }
                    if let Some(ref c) = client {
                        let ws = start_workspace_heartbeat(c).await;
                        heartbeat_project_id = ws.as_ref().map(|(p, _)| p.clone());
                        heartbeat_workspace_id = ws.as_ref().map(|(_, w)| w.clone());
                    }
                }
                // Send workspace heartbeat (best-effort, errors are silently ignored)
                if let (Some(ref c), Some(pid), Some(wid)) = (&client, &heartbeat_project_id, &heartbeat_workspace_id) {
                    let _ = c.workspace_heartbeat(pid, wid, None).await;
                }
            }
        }
    }

    Ok(())
}

/// Register workspace and return (project_id, workspace_id) if successful.
async fn start_workspace_heartbeat(client: &ApiClient) -> Option<(String, String)> {
    let link = config::find_workspace_link(None)?;
    let local_path = &link.path;
    let machine_id = config::get_machine_id();
    let name = std::path::Path::new(local_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());

    let data = RegisterWorkspaceData {
        name,
        local_path: local_path.to_string(),
        machine_id,
        metadata: None,
    };

    let ws = client
        .register_workspace(&link.project_id, &data)
        .await
        .ok()?;

    let workspace_id = ws.id;
    eprintln!(
        "📡 工作区心跳已启动 (project={}, workspace={})",
        link.project_id,
        &workspace_id[..8.min(workspace_id.len())]
    );
    Some((link.project_id, workspace_id))
}
