use std::time::Duration;

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use super::error::ApiError;
use super::types::*;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
}

impl ApiClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self, ApiError> {
        if base_url.is_empty() {
            return Err(ApiError::Network(
                "未配置服务器地址。运行 yan login 或设置 YAN_PM_BASE_URL 环境变量。".into(),
            ));
        }
        if token.is_empty() {
            return Err(ApiError::Network(
                "未配置认证 Token。运行 yan login 或设置 YAN_PM_TOKEN 环境变量。".into(),
            ));
        }
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<T, ApiError> {
        let url = format!("{}/api{}", self.base_url, path);
        let mut req = self
            .client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.token));

        if let Some(b) = body {
            req = req.json(b);
        }

        let res = req.send().await.map_err(|e| {
            if e.is_timeout() {
                ApiError::Network("请求超时 (30s)".into())
            } else {
                ApiError::Network(e.to_string())
            }
        })?;

        let status = res.status().as_u16();
        if status >= 400 {
            let msg = match res.json::<Value>().await {
                Ok(json) => {
                    if let Some(err) = json.get("error") {
                        if err.is_string() {
                            err.as_str().unwrap().to_string()
                        } else {
                            err.to_string()
                        }
                    } else if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
                        msg.to_string()
                    } else {
                        format!("HTTP {status}")
                    }
                }
                Err(_) => format!("HTTP {status}"),
            };
            return Err(ApiError::Http {
                status,
                message: msg,
            });
        }

        res.json::<T>()
            .await
            .map_err(|e| ApiError::Parse(e.to_string()))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.request(reqwest::Method::GET, path, None).await
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T, ApiError> {
        self.request(reqwest::Method::POST, path, Some(body)).await
    }

    pub async fn post_raw(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        self.post(path, body).await
    }

    async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.request(reqwest::Method::POST, path, None).await
    }

    async fn patch<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T, ApiError> {
        self.request(reqwest::Method::PATCH, path, Some(body)).await
    }

    async fn delete_req<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.request(reqwest::Method::DELETE, path, None).await
    }

    // ---- Projects ----

    pub async fn list_projects(&self) -> Result<Vec<Project>, ApiError> {
        self.get("/projects").await
    }

    pub async fn get_project(&self, project_id: &str) -> Result<ProjectDetail, ApiError> {
        validate_project_ref(project_id)?;
        self.get(&format!("/projects/{project_id}")).await
    }

    pub async fn update_project(
        &self,
        project_id: &str,
        data: &UpdateProjectData,
    ) -> Result<Project, ApiError> {
        validate_project_ref(project_id)?;
        let body = serde_json::to_value(data).map_err(|e| ApiError::Parse(e.to_string()))?;
        self.patch(&format!("/projects/{project_id}"), &body).await
    }

    // ---- Issues ----

    pub async fn list_issues(
        &self,
        project_id: &str,
        params: &IssueListParams,
    ) -> Result<Vec<Issue>, ApiError> {
        validate_project_ref(project_id)?;
        let mut query_parts = Vec::new();
        if let Some(s) = &params.status {
            query_parts.push(format!("status={s}"));
        }
        if let Some(t) = &params.issue_type {
            query_parts.push(format!("type={t}"));
        }
        if let Some(p) = &params.priority {
            query_parts.push(format!("priority={p}"));
        }
        if let Some(a) = &params.assignee_id {
            query_parts.push(format!("assigneeId={}", urlencoded(a)));
        }
        if let Some(s) = &params.search {
            query_parts.push(format!("search={}", urlencoded(s)));
        }
        let qs = if query_parts.is_empty() {
            String::new()
        } else {
            format!("?{}", query_parts.join("&"))
        };
        self.get(&format!("/projects/{project_id}/issues{qs}"))
            .await
    }

    pub async fn get_issue(
        &self,
        project_id: &str,
        issue_id: &str,
    ) -> Result<serde_json::Value, ApiError> {
        validate_project_ref(project_id)?;
        validate_resource_id(issue_id, "需求 ID")?;
        self.get(&format!("/projects/{project_id}/issues/{issue_id}"))
            .await
    }

    pub async fn create_issue(
        &self,
        project_id: &str,
        data: &CreateIssueData,
    ) -> Result<Issue, ApiError> {
        validate_project_ref(project_id)?;
        let body = serde_json::to_value(data).map_err(|e| ApiError::Parse(e.to_string()))?;
        self.post(&format!("/projects/{project_id}/issues"), &body)
            .await
    }

    pub async fn update_issue(
        &self,
        project_id: &str,
        issue_id: &str,
        data: &UpdateIssueData,
    ) -> Result<Issue, ApiError> {
        validate_project_ref(project_id)?;
        validate_resource_id(issue_id, "需求 ID")?;
        let body = serde_json::to_value(data).map_err(|e| ApiError::Parse(e.to_string()))?;
        self.patch(&format!("/projects/{project_id}/issues/{issue_id}"), &body)
            .await
    }

    pub async fn accept_issue(&self, project_id: &str, issue_id: &str) -> Result<Issue, ApiError> {
        validate_project_ref(project_id)?;
        validate_resource_id(issue_id, "需求 ID")?;
        self.post_empty(&format!("/projects/{project_id}/issues/{issue_id}/accept"))
            .await
    }

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

    // ---- Workspaces ----

    pub async fn list_workspaces(&self, project_id: &str) -> Result<Vec<Workspace>, ApiError> {
        validate_project_ref(project_id)?;
        self.get(&format!("/projects/{project_id}/workspaces"))
            .await
    }

    pub async fn register_workspace(
        &self,
        project_id: &str,
        data: &RegisterWorkspaceData,
    ) -> Result<Workspace, ApiError> {
        validate_project_ref(project_id)?;
        let body = serde_json::to_value(data).map_err(|e| ApiError::Parse(e.to_string()))?;
        self.post(&format!("/projects/{project_id}/workspaces"), &body)
            .await
    }

    pub async fn remove_workspace(
        &self,
        project_id: &str,
        workspace_id: &str,
    ) -> Result<Value, ApiError> {
        validate_project_ref(project_id)?;
        validate_resource_id(workspace_id, "工作区 ID")?;
        self.delete_req(&format!("/projects/{project_id}/workspaces/{workspace_id}"))
            .await
    }

    pub async fn workspace_heartbeat(
        &self,
        project_id: &str,
        workspace_id: &str,
        metadata: Option<&Value>,
    ) -> Result<Value, ApiError> {
        validate_project_ref(project_id)?;
        validate_resource_id(workspace_id, "工作区 ID")?;
        let body = if let Some(m) = metadata {
            serde_json::json!({ "metadata": m })
        } else {
            serde_json::json!({})
        };
        self.post(
            &format!("/projects/{project_id}/workspaces/{workspace_id}/heartbeat"),
            &body,
        )
        .await
    }

    // ---- Device Code Flow (unauthenticated) ----

    pub async fn device_code_request(base_url: &str) -> Result<DeviceCodeResponse, ApiError> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let url = format!("{}/api/auth/device/code", base_url.trim_end_matches('/'));
        let res = client
            .post(&url)
            .json(&serde_json::json!({ "client_id": "yan-pm-cli" }))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !res.status().is_success() {
            return Err(ApiError::Http {
                status: res.status().as_u16(),
                message: "Device code request failed".into(),
            });
        }

        res.json().await.map_err(|e| ApiError::Parse(e.to_string()))
    }

    pub async fn device_code_poll(
        base_url: &str,
        device_code: &str,
    ) -> Result<DeviceTokenResponse, ApiError> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let url = format!("{}/api/auth/device/token", base_url.trim_end_matches('/'));
        let res = client
            .post(&url)
            .json(&serde_json::json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": device_code,
                "client_id": "yan-pm-cli"
            }))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if res.status().is_server_error() {
            return Err(ApiError::Http {
                status: res.status().as_u16(),
                message: "Device code poll server error".into(),
            });
        }

        res.json().await.map_err(|e| ApiError::Parse(e.to_string()))
    }
}

// ---- Parameter types ----

#[derive(Debug, Default)]
pub struct IssueListParams {
    pub status: Option<IssueStatus>,
    pub issue_type: Option<IssueType>,
    pub priority: Option<TaskPriority>,
    pub assignee_id: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssueData {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub issue_type: Option<IssueType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<TaskPriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIssueData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<TaskPriority>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub issue_type: Option<IssueType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterWorkspaceData {
    pub name: String,
    pub local_path: String,
    pub machine_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

// ---- Helpers ----

fn validate_project_ref(project_ref: &str) -> Result<(), ApiError> {
    if project_ref.is_empty()
        || project_ref.len() > 100
        || !project_ref
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::Http {
            status: 400,
            message: format!("无效的项目标识: {project_ref}"),
        });
    }
    Ok(())
}

fn validate_resource_id(id: &str, label: &str) -> Result<(), ApiError> {
    if id.is_empty()
        || id.len() > 100
        || !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::Http {
            status: 400,
            message: format!("无效的{label}: {id}"),
        });
    }
    Ok(())
}

fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            // RFC 3986 unreserved characters
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_project_ref_rejects_path_traversal() {
        assert!(validate_project_ref("foo/../bar").is_err());
        assert!(validate_project_ref("foo/bar").is_err());
        assert!(validate_project_ref("foo%00bar").is_err());
        assert!(validate_project_ref("").is_err());
    }

    #[test]
    fn test_validate_project_ref_accepts_valid() {
        assert!(validate_project_ref("my-project").is_ok());
        assert!(validate_project_ref("my_project").is_ok());
        assert!(validate_project_ref("abc123").is_ok());
        assert!(validate_project_ref("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn test_urlencoded_special_chars() {
        assert_eq!(urlencoded("a b"), "a%20b");
        assert_eq!(urlencoded("C# guide"), "C%23%20guide");
        assert_eq!(urlencoded("100%"), "100%25");
        assert_eq!(urlencoded("a/b?c=d&e"), "a%2Fb%3Fc%3Dd%26e");
        // Unicode should be percent-encoded per UTF-8 bytes
        assert_eq!(urlencoded("中文"), "%E4%B8%AD%E6%96%87");
        // Unreserved chars pass through
        assert_eq!(urlencoded("hello-world_123.~"), "hello-world_123.~");
    }
}
