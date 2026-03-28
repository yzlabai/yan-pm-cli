use yan_pm_cli::agent::backend::AgentBackend;
use yan_pm_cli::agent::backends::{builtin_backends, ClaudeBackend, CodexBackend, GeminiBackend};

#[test]
fn test_claude_backend() {
    let b = ClaudeBackend;
    assert_eq!(b.name(), "claude");
    assert_eq!(b.command(), "claude");
    assert_eq!(b.acp_args(), vec!["--acp"]);
    let caps = b.capabilities();
    assert!(caps.supports_images);
    assert!(caps.supports_mcp);
    assert!(caps.supports_worktree);
    assert_eq!(caps.max_context_tokens, 200_000);
    assert_eq!(b.priority(), 1);
    assert_eq!(b.protocol_version(), "v1");
}

#[test]
fn test_codex_backend() {
    let b = CodexBackend;
    assert_eq!(b.name(), "codex");
    let caps = b.capabilities();
    assert!(!caps.supports_images);
    assert!(!caps.supports_mcp);
    assert_eq!(b.priority(), 2);
}

#[test]
fn test_gemini_backend() {
    let b = GeminiBackend;
    assert_eq!(b.name(), "gemini");
    assert_eq!(b.acp_args(), vec!["--experimental-acp"]);
    let caps = b.capabilities();
    assert!(caps.supports_images);
    assert_eq!(caps.max_context_tokens, 1_000_000);
    assert_eq!(b.priority(), 3);
}

#[test]
fn test_builtin_backends_sorted_by_priority() {
    let mut backends = builtin_backends();
    backends.sort_by_key(|b| b.priority());
    let names: Vec<&str> = backends.iter().map(|b| b.name()).collect();
    assert_eq!(names, vec!["claude", "codex", "gemini"]);
}

#[test]
fn test_build_prompt() {
    let b = ClaudeBackend;
    let prompt = b.build_prompt("Fix bug", "Crash on startup");
    assert!(prompt.contains("Fix bug"));
    assert!(prompt.contains("Crash on startup"));
}

#[test]
fn test_to_definition() {
    let b: Box<dyn AgentBackend> = Box::new(ClaudeBackend);
    let def = b.as_ref().to_definition();
    assert_eq!(def.name, "claude");
    assert_eq!(def.command, "claude");
    assert_eq!(def.acp_args, vec!["--acp"]);
}
