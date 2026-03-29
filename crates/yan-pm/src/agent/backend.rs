use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Capabilities advertised by an agent backend
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub supports_images: bool,
    pub supports_mcp: bool,
    pub supports_worktree: bool,
    pub max_context_tokens: u32,
}

/// Polymorphic interface for agent backends
#[allow(dead_code)]
pub trait AgentBackend: Send + Sync {
    /// Identifier, e.g. "claude"
    fn name(&self) -> &str;

    /// Executable name, e.g. "claude"
    fn command(&self) -> &str;

    /// ACP startup args appended when launching the agent
    fn acp_args(&self) -> Vec<String>;

    /// Default environment variables (empty by default)
    fn env_vars(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    /// Capabilities of this backend
    fn capabilities(&self) -> AgentCapabilities;

    /// ACP protocol version (default "v1")
    fn protocol_version(&self) -> &str {
        "v1"
    }

    /// Command used to check availability (defaults to `command()`)
    fn is_available_cmd(&self) -> &str {
        self.command()
    }

    /// Build a prompt string from task title and description.
    ///
    /// Default implementation produces a Chinese task template.
    fn build_prompt(&self, title: &str, description: &str) -> String {
        format!("## 任务：{title}\n\n{description}\n\n请完成上述任务，确保代码质量和测试覆盖。")
    }

    /// Optional human-readable description
    fn description(&self) -> Option<&str> {
        None
    }

    /// Selection priority — lower number means higher preference (default 100)
    fn priority(&self) -> u32 {
        100
    }
}

#[allow(dead_code)]
impl dyn AgentBackend {
    /// Convert to `AgentDefinition` for compatibility with the registry layer.
    pub fn to_definition(&self) -> super::registry::AgentDefinition {
        super::registry::AgentDefinition {
            name: self.name().to_string(),
            command: self.command().to_string(),
            acp_args: self.acp_args(),
            env: self.env_vars(),
            description: self.description().map(str::to_string),
        }
    }
}
