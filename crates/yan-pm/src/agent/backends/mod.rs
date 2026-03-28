pub mod claude;
pub mod codex;
pub mod gemini;

pub use claude::ClaudeBackend;
pub use codex::CodexBackend;
pub use gemini::GeminiBackend;

use super::backend::AgentBackend;

pub fn builtin_backends() -> Vec<Box<dyn AgentBackend>> {
    vec![
        Box::new(ClaudeBackend),
        Box::new(CodexBackend),
        Box::new(GeminiBackend),
    ]
}
