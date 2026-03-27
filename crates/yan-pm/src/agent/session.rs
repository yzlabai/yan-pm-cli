use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use agent_client_protocol::{self as acp, Agent as _};
use anyhow::Result;
use colored::Colorize;
use futures::lock::Mutex;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::registry::{is_command_available, AgentDefinition};

/// Result from agent execution
#[derive(Debug)]
#[allow(dead_code)]
pub struct AgentResult {
    pub success: bool,
    pub summary: String,
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
    pub exit_code: i32,
}

/// Options for agent execution
#[allow(dead_code)]
pub struct AgentOptions {
    pub cwd: String,
    pub prompt: String,
    pub max_budget_usd: Option<f64>,
    pub permission_mode: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub mcp_configs: Option<Vec<String>>,
    pub model: Option<String>,
    pub verbose: bool,
}

/// Permission policy for agent tool calls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionPolicy {
    /// Auto-approve all permission requests
    AutoApprove,
    /// Deny all permission requests
    Deny,
}

/// Internal client state for ACP communication
#[allow(dead_code)]
struct YanPmAcpClient {
    policy: PermissionPolicy,
    output: Arc<Mutex<String>>,
    verbose: bool,
    cancelled: Arc<AtomicBool>,
}

#[async_trait::async_trait(?Send)]
impl acp::Client for YanPmAcpClient {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        match self.policy {
            PermissionPolicy::AutoApprove => {
                if self.verbose {
                    let tool_name = args.tool_call.fields.title.as_deref().unwrap_or("unknown");
                    eprintln!("{}", format!("  [auto-approve] {tool_name}").dimmed());
                }
                // Find the first AllowOnce/AllowAlways option, or fallback to first option
                let option_id = args
                    .options
                    .iter()
                    .find(|o| {
                        matches!(
                            o.kind,
                            acp::PermissionOptionKind::AllowOnce
                                | acp::PermissionOptionKind::AllowAlways
                        )
                    })
                    .or(args.options.first())
                    .map(|o| o.option_id.clone());
                match option_id {
                    Some(id) => Ok(acp::RequestPermissionResponse::new(
                        acp::RequestPermissionOutcome::Selected(
                            acp::SelectedPermissionOutcome::new(id),
                        ),
                    )),
                    None => Ok(acp::RequestPermissionResponse::new(
                        acp::RequestPermissionOutcome::Cancelled,
                    )),
                }
            }
            PermissionPolicy::Deny => Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Cancelled,
            )),
        }
    }

    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        match args.update {
            acp::SessionUpdate::AgentMessageChunk(chunk) => {
                if let acp::ContentBlock::Text(text_content) = &chunk.content {
                    let mut output = self.output.lock().await;
                    // Cap output buffer at 1 MB to prevent OOM from runaway agents
                    const MAX_OUTPUT: usize = 1_048_576;
                    if output.len() < MAX_OUTPUT {
                        let remaining = MAX_OUTPUT - output.len();
                        if text_content.text.len() <= remaining {
                            output.push_str(&text_content.text);
                        } else {
                            // Safe UTF-8 truncation to avoid panic on multi-byte chars
                            let safe =
                                crate::output::format::truncate_utf8(&text_content.text, remaining);
                            output.push_str(safe);
                        }
                    }
                    if self.verbose {
                        eprint!("{}", text_content.text);
                    }
                }
            }
            acp::SessionUpdate::ToolCall(tc) => {
                if self.verbose {
                    eprintln!("{}", format!("  [tool call] {}", tc.title).dimmed());
                }
            }
            acp::SessionUpdate::ToolCallUpdate(tc) => {
                if self.verbose {
                    if let Some(title) = &tc.fields.title {
                        eprintln!("{}", format!("  [tool update] {title}").dimmed());
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Execute a task using an ACP-compatible agent.
///
/// Spawns the agent process, connects via ACP over stdio, sends the prompt,
/// and collects the result.
pub async fn execute_agent(agent: &AgentDefinition, options: AgentOptions) -> Result<AgentResult> {
    if !is_command_available(&agent.command).await {
        return Ok(AgentResult {
            success: false,
            summary: format!(
                "{} CLI 未安装。请确保 {} 已安装且在 PATH 中。",
                agent.name, agent.command
            ),
            cost_usd: None,
            session_id: None,
            exit_code: 127,
        });
    }

    let policy = match options.permission_mode.as_deref() {
        Some("auto") | Some("bypassPermissions") | None => PermissionPolicy::AutoApprove,
        Some("plan") | Some("deny") => PermissionPolicy::Deny,
        Some(other) => {
            return Ok(AgentResult {
                success: false,
                summary: format!("无效的 permission_mode: \"{other}\"。可选: auto, plan, deny"),
                cost_usd: None,
                session_id: None,
                exit_code: 1,
            });
        }
    };

    let output = Arc::new(Mutex::new(String::new()));
    let cancelled = Arc::new(AtomicBool::new(false));

    let client_handler = YanPmAcpClient {
        policy,
        output: output.clone(),
        verbose: options.verbose,
        cancelled: cancelled.clone(),
    };

    // Build args
    let mut args = agent.acp_args.clone();

    // Some agents accept extra CLI flags even in ACP mode
    if let Some(model) = &options.model {
        args.push("--model".into());
        args.push(model.clone());
    }

    // Spawn agent process
    let mut child = tokio::process::Command::new(&agent.command)
        .args(&args)
        .current_dir(&options.cwd)
        .envs(&agent.env)
        .env("CI", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");

    // stderr collector (capped at 1MB to prevent OOM)
    let stderr_handle = {
        let stderr = child.stderr.take().expect("stderr piped");
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut reader = stderr;
            let mut buf = Vec::with_capacity(8192);
            const MAX_STDERR: usize = 1_048_576; // 1MB
            loop {
                let mut chunk = [0u8; 8192];
                match reader.read(&mut chunk).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let remaining = MAX_STDERR.saturating_sub(buf.len());
                        buf.extend_from_slice(&chunk[..n.min(remaining)]);
                        if buf.len() >= MAX_STDERR {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            String::from_utf8_lossy(&buf).to_string()
        })
    };

    // ACP requires non-Send futures, use LocalSet
    let local_set = tokio::task::LocalSet::new();
    let prompt = options.prompt.clone();
    let cwd = options.cwd.clone();
    let verbose = options.verbose;

    const ACP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);
    let acp_future = local_set.run_until(async move {
        let outgoing = stdin.compat_write();
        let incoming = stdout.compat();

        let (conn, io_task) =
            acp::ClientSideConnection::new(client_handler, outgoing, incoming, |fut| {
                tokio::task::spawn_local(fut);
            });

        // Run I/O in background
        tokio::task::spawn_local(io_task);

        // Initialize
        let init_result = conn
            .initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(
                    acp::Implementation::new("yan-pm", env!("CARGO_PKG_VERSION"))
                        .title("YanChat PM"),
                ),
            )
            .await;

        if let Err(e) = init_result {
            return Err(anyhow::anyhow!("ACP 初始化失败: {e}"));
        }

        // Create session
        let session_result = conn
            .new_session(acp::NewSessionRequest::new(std::path::PathBuf::from(&cwd)))
            .await;

        let session = match session_result {
            Ok(s) => s,
            Err(e) => return Err(anyhow::anyhow!("ACP 创建会话失败: {e}")),
        };

        if verbose {
            eprintln!("{}", format!("  [session] {}", session.session_id).dimmed());
        }

        // Send prompt
        let prompt_result = conn
            .prompt(acp::PromptRequest::new(
                session.session_id.clone(),
                vec![prompt.into()],
            ))
            .await;

        match prompt_result {
            Ok(resp) => Ok((session.session_id, resp)),
            Err(e) => Err(anyhow::anyhow!("ACP prompt 失败: {e}")),
        }
    });

    let acp_result = match tokio::time::timeout(ACP_TIMEOUT, acp_future).await {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            return Ok(AgentResult {
                success: false,
                summary: format!("Agent 执行超时 ({}s)", ACP_TIMEOUT.as_secs()),
                cost_usd: None,
                session_id: None,
                exit_code: 1,
            });
        }
    };

    // Wait for process to exit
    let status = child.wait().await?;
    let exit_code = status.code().unwrap_or(1);
    let stderr_text = stderr_handle.await.unwrap_or_default();

    // Collect accumulated output
    let collected_output = output.lock().await.clone();

    match acp_result {
        Ok((session_id, resp)) => {
            let success = match &resp.stop_reason {
                acp::StopReason::EndTurn | acp::StopReason::MaxTokens => exit_code == 0,
                acp::StopReason::Cancelled => false,
                #[allow(unreachable_patterns)]
                _ => exit_code == 0,
            };

            let summary = if collected_output.is_empty() {
                format!("Agent 完成 (stop_reason: {:?})", resp.stop_reason)
            } else {
                collected_output
            };

            Ok(AgentResult {
                success,
                summary,
                cost_usd: None, // ACP doesn't expose cost in standard protocol
                session_id: Some(session_id.0.to_string()),
                exit_code,
            })
        }
        Err(e) => Ok(AgentResult {
            success: false,
            summary: format!(
                "{}{}{}",
                e,
                if !collected_output.is_empty() {
                    format!("\n\n{collected_output}")
                } else {
                    String::new()
                },
                if !stderr_text.is_empty() {
                    format!(
                        "\n\nstderr:\n{}",
                        crate::output::format::truncate_utf8(&stderr_text, 1000)
                    )
                } else {
                    String::new()
                }
            ),
            cost_usd: None,
            session_id: None,
            exit_code,
        }),
    }
}
