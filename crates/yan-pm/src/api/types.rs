use serde::{Deserialize, Serialize};

// ---- Enums ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Planning,
    Active,
    Completed,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    InReview,
    Done,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Todo => write!(f, "todo"),
            Self::InProgress => write!(f, "in_progress"),
            Self::InReview => write!(f, "in_review"),
            Self::Done => write!(f, "done"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Urgent,
    High,
    Medium,
    Low,
}

impl TaskPriority {
    pub fn order(&self) -> u8 {
        match self {
            Self::Urgent => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Urgent => write!(f, "urgent"),
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Feature,
    Bug,
    Improvement,
    Task,
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Feature => write!(f, "feature"),
            Self::Bug => write!(f, "bug"),
            Self::Improvement => write!(f, "improvement"),
            Self::Task => write!(f, "task"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    Open,
    Analyzing,
    TasksCreated,
    NeedsManual,
    Cancelled,
}

impl std::fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Analyzing => write!(f, "analyzing"),
            Self::TasksCreated => write!(f, "tasks_created"),
            Self::NeedsManual => write!(f, "needs_manual"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum IssueType {
    Feature,
    Bug,
    Improvement,
    Question,
}

impl std::fmt::Display for IssueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Feature => write!(f, "feature"),
            Self::Bug => write!(f, "bug"),
            Self::Improvement => write!(f, "improvement"),
            Self::Question => write!(f, "question"),
        }
    }
}

// ---- API Response Structures ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub status: ProjectStatus,
    pub my_role: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMember {
    pub user_id: String,
    pub role: String,
    pub user_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    #[serde(flatten)]
    pub project: Project,
    pub members: Vec<ProjectMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub tags: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub sort_order: Option<i32>,
    pub due_date: Option<String>,
    pub locked_by: Option<String>,
    pub locked_at: Option<String>,
    pub last_heartbeat: Option<String>,
    pub number: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
    pub assignee_id: Option<String>,
    pub creator_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    #[serde(flatten)]
    pub task: Task,
    pub comments: Option<Vec<Comment>>,
    pub creator_name: Option<String>,
    pub assignee_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub task_id: Option<String>,
    pub user_id: String,
    pub content: String,
    pub created_at: String,
    pub user_name: Option<String>,
}

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
    pub closed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub creator_id: Option<String>,
    pub assignee_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub user_id: String,
    pub name: String,
    pub local_path: String,
    pub machine_id: String,
    pub metadata: Option<serde_json::Value>,
    pub last_heartbeat: Option<String>,
    pub created_at: String,
    pub user_name: Option<String>,
    pub online: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStatus {
    pub tasks: Vec<Task>,
    pub stale_threshold_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecomposeResult {
    pub created: i32,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportResult {
    pub report: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResult {
    pub ok: bool,
    pub last_heartbeat: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub status: String, // "succeeded" | "failed" | "cancelled"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceTokenResponse {
    pub access_token: Option<String>,
    pub error: Option<String>,
}
