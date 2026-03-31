use assert_cmd::Command;
use predicates::prelude::*;
use std::io::{BufRead, Write};

fn cmd() -> Command {
    Command::cargo_bin("yan").unwrap()
}

// =====================
// CLI 基础测试
// =====================

#[test]
fn test_help() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("yan.chat"))
        .stdout(predicate::str::contains("Commands:"));
}

#[test]
fn test_version() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("yan"));
}

#[test]
fn test_subcommand_help_daemon() {
    cmd()
        .args(["daemon", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("stop"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("install"))
        .stdout(predicate::str::contains("uninstall"));
}

#[test]
fn test_subcommand_help_auto_run() {
    cmd()
        .args(["auto-run", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("on"))
        .stdout(predicate::str::contains("off"))
        .stdout(predicate::str::contains("status"));
}

#[test]
fn test_subcommand_help_auto_run_on() {
    cmd()
        .args(["auto-run", "on", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--budget"))
        .stdout(predicate::str::contains("--concurrency"))
        .stdout(predicate::str::contains("--agent"));
}

// =====================
// 无需服务器的命令
// =====================

#[test]
fn test_agents() {
    cmd()
        .arg("agents")
        .assert()
        .success()
        .stdout(predicate::str::contains("Agent"))
        .stdout(predicate::str::contains("claude"));
}

#[test]
fn test_auto_run_status_no_link() {
    let tmp = tempfile::tempdir().unwrap();
    cmd()
        .arg("auto-run")
        .arg("status")
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("未关联项目"));
}

#[test]
fn test_info_no_link() {
    let tmp = tempfile::tempdir().unwrap();
    cmd()
        .arg("info")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("未关联").or(predicate::str::contains("null")));
}

#[test]
fn test_unlink_no_link() {
    let tmp = tempfile::tempdir().unwrap();
    cmd()
        .arg("unlink")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("未关联"));
}

// =====================
// 需要服务器但未配置 — 应报错
// =====================

#[test]
fn test_list_no_config() {
    let tmp = tempfile::tempdir().unwrap();
    cmd()
        .arg("list")
        .env("HOME", tmp.path())
        .env_remove("YAN_PM_BASE_URL")
        .env_remove("YAN_PM_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("yan-pm login").or(predicate::str::contains("未配置")));
}

#[test]
fn test_issues_no_config() {
    let tmp = tempfile::tempdir().unwrap();
    cmd()
        .args(["issue", "list", "test-project"])
        .env("HOME", tmp.path())
        .env_remove("YAN_PM_BASE_URL")
        .env_remove("YAN_PM_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("yan-pm login").or(predicate::str::contains("未配置")));
}

// =====================
// Mockito API 测试
// =====================

#[tokio::test]
async fn test_list_projects_with_mock_server() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/projects")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!([
                {
                    "id": "proj-1",
                    "slug": "test-project",
                    "name": "Test Project",
                    "description": null,
                    "status": "active",
                    "myRole": "admin",
                    "createdAt": "2026-03-25T00:00:00Z",
                    "updatedAt": "2026-03-25T00:00:00Z"
                }
            ])
            .to_string(),
        )
        .create_async()
        .await;

    cmd()
        .arg("list")
        .arg("--url")
        .arg(server.url())
        .arg("--token")
        .arg("test-token")
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Project"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_list_projects_json_output() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/projects")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!([
                {
                    "id": "proj-1",
                    "slug": "demo",
                    "name": "Demo",
                    "description": "A demo project",
                    "status": "active",
                    "myRole": "admin",
                    "createdAt": "2026-03-25T00:00:00Z",
                    "updatedAt": "2026-03-25T00:00:00Z"
                }
            ])
            .to_string(),
        )
        .create_async()
        .await;

    cmd()
        .args(["list", "--json", "--url"])
        .arg(server.url())
        .args(["--token", "test-token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"slug\""))
        .stdout(predicate::str::contains("\"demo\""));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_list_issues_with_mock() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/projects/proj-1/issues")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!([
                {
                    "id": "issue-1111-2222-3333-444444444444",
                    "projectId": "proj-1",
                    "number": 1,
                    "title": "Login Timeout Bug",
                    "description": null,
                    "type": "bug",
                    "priority": "urgent",
                    "status": "open",
                    "labels": ["auth"],
                    "closedAt": null,
                    "createdAt": "2026-03-25T00:00:00Z",
                    "updatedAt": "2026-03-25T00:00:00Z"
                }
            ])
            .to_string(),
        )
        .create_async()
        .await;

    cmd()
        .args(["issue", "list", "proj-1", "--url"])
        .arg(server.url())
        .args(["--token", "test-token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Login Timeout Bug"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_error_401() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/projects")
        .with_status(401)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error": "Unauthorized"}"#)
        .create_async()
        .await;

    // Error path: ApiError::Http { 401, "Unauthorized" } → "HTTP 401: Unauthorized"
    cmd()
        .args(["list", "--url"])
        .arg(server.url())
        .args(["--token", "bad-token"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("HTTP 401: Unauthorized"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_error_500() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/projects")
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"message": "Internal Server Error"}"#)
        .create_async()
        .await;

    cmd()
        .args(["list", "--url"])
        .arg(server.url())
        .args(["--token", "test-token"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("HTTP 500"));

    mock.assert_async().await;
}

// =====================
// Issue subcommand tests
// =====================

#[test]
fn test_issue_help_shows_subcommands() {
    cmd()
        .args(["issue", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("accept"))
        .stdout(predicate::str::contains("deliver"));
}

#[test]
fn test_issue_list_help_shows_filters() {
    cmd()
        .args(["issue", "list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--priority"))
        .stdout(predicate::str::contains("--type"))
        .stdout(predicate::str::contains("--status"));
}

#[test]
fn test_issue_update_help_shows_assignee() {
    cmd()
        .args(["issue", "update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--assignee"))
        .stdout(predicate::str::contains("--labels"));
}

#[test]
fn test_issue_create_help_shows_desc_alias() {
    cmd()
        .args(["issue", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--description"));
}

#[tokio::test]
async fn test_issue_accept_with_mock() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/api/projects/proj-1/issues/issue-123/accept")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "id": "issue-123",
                "projectId": "proj-1",
                "number": 1,
                "title": "Test Issue",
                "type": "feature",
                "priority": "high",
                "status": "accepted",
                "labels": [],
                "createdAt": "2026-03-25T00:00:00Z",
                "updatedAt": "2026-03-25T00:00:00Z"
            })
            .to_string(),
        )
        .create_async()
        .await;

    cmd()
        .args(["issue", "accept", "proj-1", "issue-123", "--url"])
        .arg(server.url())
        .args(["--token", "test-token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("已接受"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_issue_deliver_with_mock() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/api/projects/proj-1/issues/issue-123/deliver")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "id": "issue-123",
                "projectId": "proj-1",
                "number": 1,
                "title": "Test Issue",
                "type": "feature",
                "priority": "high",
                "status": "delivered",
                "labels": [],
                "createdAt": "2026-03-25T00:00:00Z",
                "updatedAt": "2026-03-25T00:00:00Z"
            })
            .to_string(),
        )
        .create_async()
        .await;

    cmd()
        .args(["issue", "deliver", "proj-1", "issue-123", "--url"])
        .arg(server.url())
        .args(["--token", "test-token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("已交付"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_issues_with_priority_filter() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/projects/proj-1/issues")
        .match_query(mockito::Matcher::AllOf(vec![mockito::Matcher::UrlEncoded(
            "priority".into(),
            "urgent".into(),
        )]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .create_async()
        .await;

    cmd()
        .args(["issue", "list", "proj-1", "--priority", "urgent", "--url"])
        .arg(server.url())
        .args(["--token", "test-token"])
        .assert()
        .success();

    mock.assert_async().await;
}

// =====================
// Tasks (local only) tests
// =====================

#[test]
fn test_tasks_help_shows_local() {
    cmd()
        .args(["tasks", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--local"));
}

// =====================
// MCP stdio 端到端测试
// =====================

#[tokio::test]
async fn test_mcp_stdio_initialize_and_tools_list() {
    let mut server = mockito::Server::new_async().await;

    // MCP server validates token on startup by calling list_projects
    let _validate_mock = server
        .mock("GET", "/api/projects")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .create_async()
        .await;

    let binary = assert_cmd::cargo::cargo_bin("yan");
    let mut child = std::process::Command::new(&binary)
        .arg("mcp")
        .env("YAN_PM_BASE_URL", server.url())
        .env("YAN_PM_TOKEN", "test-mcp-token")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn yan-pm mcp");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);

    // --- initialize ---
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    });
    writeln!(stdin, "{}", init_req).unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let init_resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();

    assert_eq!(init_resp["jsonrpc"], "2.0");
    assert_eq!(init_resp["id"], 1);
    assert!(init_resp["result"]["protocolVersion"].is_string());
    assert!(init_resp["result"]["capabilities"]["tools"].is_object());
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "yan-pm");

    // --- notifications/initialized (should be silently consumed) ---
    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{}", notif).unwrap();
    stdin.flush().unwrap();

    // --- tools/list ---
    let list_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    writeln!(stdin, "{}", list_req).unwrap();
    stdin.flush().unwrap();

    let mut line2 = String::new();
    reader.read_line(&mut line2).unwrap();
    let list_resp: serde_json::Value = serde_json::from_str(line2.trim()).unwrap();

    assert_eq!(list_resp["jsonrpc"], "2.0");
    assert_eq!(list_resp["id"], 2);
    let tools = list_resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 8);

    // Verify key tool names are present
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"list_projects"));
    assert!(names.contains(&"list_issues"));
    assert!(names.contains(&"get_issue"));
    assert!(names.contains(&"create_issue"));
    assert!(names.contains(&"accept_issue"));
    assert!(names.contains(&"deliver_issue"));

    // Each tool must have inputSchema with type "object"
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object");
    }

    // --- unknown method → error ---
    let bad_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "unsupported/method",
        "params": {}
    });
    writeln!(stdin, "{}", bad_req).unwrap();
    stdin.flush().unwrap();

    let mut line3 = String::new();
    reader.read_line(&mut line3).unwrap();
    let err_resp: serde_json::Value = serde_json::from_str(line3.trim()).unwrap();

    assert_eq!(err_resp["jsonrpc"], "2.0");
    assert_eq!(err_resp["id"], 3);
    assert!(err_resp["error"]["code"].is_number());
    assert!(err_resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Method not found"));

    // Close stdin to let the process exit
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn test_start_help_shows_tools_and_mcp_config() {
    let cmd = Command::cargo_bin("yan")
        .unwrap()
        .args(["start", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8(cmd.stdout).unwrap();
    assert!(help.contains("--tools"), "should show --tools flag");
    assert!(
        help.contains("--mcp-config"),
        "should show --mcp-config flag"
    );
}

#[test]
fn test_link_help_shows_path_and_name() {
    cmd()
        .args(["link", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--path"))
        .stdout(predicate::str::contains("--name"));
}

#[test]
fn test_login_help_shows_token() {
    cmd()
        .args(["login", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--token"));
}

#[test]
fn test_start_help_shows_cwd_and_budgets() {
    cmd()
        .args(["start", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--cwd"))
        .stdout(predicate::str::contains("--budget"))
        .stdout(predicate::str::contains("--total-budget"));
}

// =====================
// setup 命令测试
// =====================

#[test]
fn test_setup_help() {
    cmd()
        .arg("setup")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--target"))
        .stdout(predicate::str::contains("--uninstall"))
        .stdout(predicate::str::contains("--status"));
}

#[test]
fn test_setup_status() {
    cmd().arg("setup").arg("--status").assert().success();
}
