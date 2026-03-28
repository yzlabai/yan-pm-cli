use crate::agent::backend::{AgentBackend, AgentCapabilities};

pub struct CodexBackend;

impl AgentBackend for CodexBackend {
    fn name(&self) -> &str {
        "codex"
    }

    fn command(&self) -> &str {
        "codex"
    }

    fn acp_args(&self) -> Vec<String> {
        vec!["--acp".to_string()]
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_images: false,
            supports_mcp: false,
            supports_worktree: true,
            max_context_tokens: 200_000,
        }
    }

    fn description(&self) -> Option<&str> {
        Some("OpenAI Codex CLI")
    }

    fn priority(&self) -> u32 {
        2
    }
}
