use crate::agent::backend::{AgentBackend, AgentCapabilities};

pub struct GeminiBackend;

impl AgentBackend for GeminiBackend {
    fn name(&self) -> &str {
        "gemini"
    }

    fn command(&self) -> &str {
        "gemini"
    }

    fn acp_args(&self) -> Vec<String> {
        vec!["--experimental-acp".to_string()]
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            supports_images: true,
            supports_mcp: true,
            supports_worktree: false,
            max_context_tokens: 1_000_000,
        }
    }

    fn description(&self) -> Option<&str> {
        Some("Google Gemini CLI")
    }

    fn priority(&self) -> u32 {
        3
    }
}
