use anyhow::{bail, Result};

use crate::agent::AgentBackend;
use crate::api::client::ApiClient;

/// Options for task execution
pub struct TaskRunnerOptions {
    pub cwd: String,
    pub workspace_id: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub permission_mode: Option<String>,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub mcp_configs: Option<Vec<String>>,
    pub verbose: bool,
    pub agent: Box<dyn AgentBackend>,
}

/// Options for the start command
pub struct StartOptions {
    pub project_id: String,
    pub task_id: Option<String>,
    pub auto: bool,
    pub total_budget_usd: Option<f64>,
    pub runner: TaskRunnerOptions,
}

/// Main start entry point — disabled during Phase 2 refactor
pub async fn start(_client: &ApiClient, _options: StartOptions) -> Result<()> {
    bail!("yan-pm start 正在重构中，请使用 AI 编程工具直接执行")
}
