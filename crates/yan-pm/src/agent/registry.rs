use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config;

/// Definition of an AI coding agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Display name
    pub name: String,
    /// Command to run (e.g. "claude", "codex")
    pub command: String,
    /// Args appended to enable ACP mode
    pub acp_args: Vec<String>,
    /// Extra environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Optional description
    pub description: Option<String>,
}

/// Built-in agent definitions
pub fn builtin_agents() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            name: "claude".into(),
            command: "claude".into(),
            acp_args: vec!["--acp".into()],
            env: HashMap::new(),
            description: Some("Anthropic Claude Code".into()),
        },
        AgentDefinition {
            name: "codex".into(),
            command: "codex".into(),
            acp_args: vec!["--acp".into()],
            env: HashMap::new(),
            description: Some("OpenAI Codex CLI".into()),
        },
        AgentDefinition {
            name: "gemini".into(),
            command: "gemini".into(),
            acp_args: vec!["--experimental-acp".into()],
            env: HashMap::new(),
            description: Some("Google Gemini CLI".into()),
        },
    ]
}

/// Agent config file (agents.toml)
#[derive(Debug, Deserialize)]
struct AgentsConfig {
    #[serde(default)]
    agent: Vec<AgentDefinition>,
}

/// Load all agents: built-in + user-defined from agents.toml
pub fn load_agents() -> Vec<AgentDefinition> {
    let mut agents = builtin_agents();

    let config_path = agents_toml_path();
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(cfg) = toml::from_str::<AgentsConfig>(&content) {
                for custom in cfg.agent {
                    // Override built-in by name
                    if let Some(pos) = agents.iter().position(|a| a.name == custom.name) {
                        agents[pos] = custom;
                    } else {
                        agents.push(custom);
                    }
                }
            }
        }
    }

    agents
}

/// Find agent by name
pub fn find_agent(name: &str) -> Option<AgentDefinition> {
    load_agents().into_iter().find(|a| a.name == name)
}

/// List available agents (those that exist in PATH)
#[allow(dead_code)]
pub async fn list_available_agents() -> Vec<AgentDefinition> {
    let agents = load_agents();
    let mut available = Vec::new();
    for agent in agents {
        if is_command_available(&agent.command).await {
            available.push(agent);
        }
    }
    available
}

/// Check if a command exists in PATH
pub async fn is_command_available(command: &str) -> bool {
    // Validate command name to prevent injection (allow only alphanumeric, dash, underscore, dot)
    if !command
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return false;
    }
    // Use "command -v" on Unix (sh built-in, works on macOS/Linux)
    // and "where" on Windows
    #[cfg(unix)]
    let result = tokio::process::Command::new("sh")
        .args(["-c", &format!("command -v '{}'", command)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    #[cfg(windows)]
    let result = tokio::process::Command::new("where")
        .arg(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    result.map(|s| s.success()).unwrap_or(false)
}

fn agents_toml_path() -> PathBuf {
    config::config_dir().join("agents.toml")
}
