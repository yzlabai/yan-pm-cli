use crate::agent::backend::{AgentBackend, AgentCapabilities};

pub struct ClaudeBackend;

impl AgentBackend for ClaudeBackend {
    fn name(&self) -> &str {
        "claude"
    }

    fn command(&self) -> &str {
        "claude"
    }

    fn acp_args(&self) -> Vec<String> {
        vec!["--acp".to_string()]
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_images: true,
            supports_mcp: true,
            supports_worktree: true,
            max_context_tokens: 200_000,
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Anthropic Claude Code")
    }

    fn priority(&self) -> u32 {
        1
    }
}
