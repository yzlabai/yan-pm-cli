use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colored::Colorize;

const SKILL_CONTENT: &str = include_str!("../../../../SKILL.md");

// ── Target detection ──

#[derive(Debug, Clone)]
struct DetectedTool {
    name: &'static str,
    kind: ToolKind,
    detail: String,
}

#[derive(Debug, Clone, PartialEq)]
enum ToolKind {
    Claude,
    Vscode,
    Cursor,
}

fn home_dir() -> Result<PathBuf> {
    dirs_next().ok_or_else(|| anyhow::anyhow!("无法获取 HOME 目录"))
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn detect_tools() -> Vec<DetectedTool> {
    let mut tools = vec![];

    // Claude Code: check `claude` command in PATH
    if let Ok(output) = std::process::Command::new("which").arg("claude").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            tools.push(DetectedTool {
                name: "Claude Code",
                kind: ToolKind::Claude,
                detail: path,
            });
        }
    }
    // Fallback: check ~/.claude/ directory
    if !tools.iter().any(|t| t.kind == ToolKind::Claude) {
        if let Ok(home) = home_dir() {
            if home.join(".claude").exists() {
                tools.push(DetectedTool {
                    name: "Claude Code",
                    kind: ToolKind::Claude,
                    detail: home.join(".claude").to_string_lossy().to_string(),
                });
            }
        }
    }

    // VS Code
    if let Ok(home) = home_dir() {
        if home.join(".vscode").exists() {
            tools.push(DetectedTool {
                name: "VS Code",
                kind: ToolKind::Vscode,
                detail: home.join(".vscode").to_string_lossy().to_string(),
            });
        }
    }

    // Cursor
    if let Ok(home) = home_dir() {
        if home.join(".cursor").exists() {
            tools.push(DetectedTool {
                name: "Cursor",
                kind: ToolKind::Cursor,
                detail: home.join(".cursor").to_string_lossy().to_string(),
            });
        }
    }

    tools
}

// ── Binary path resolution ──

fn resolve_binary_path(override_path: Option<&str>) -> Result<String> {
    // 1. User-specified
    if let Some(p) = override_path {
        let path = PathBuf::from(p)
            .canonicalize()
            .with_context(|| format!("指定的路径不存在: {p}"))?;
        return Ok(path.to_string_lossy().to_string());
    }

    // 2. Current executable
    if let Ok(exe) = std::env::current_exe().and_then(|p| p.canonicalize()) {
        let exe_str = exe.to_string_lossy().to_string();
        if exe_str.contains("target/debug") || exe_str.contains("target/release") {
            eprintln!(
                "{}",
                format!("⚠ 当前路径看起来是开发构建: {exe_str}").yellow()
            );
            eprintln!("{}", "  建议使用 --binary-path 指定安装后的路径".yellow());
        }
        return Ok(exe_str);
    }

    // 3. which lookup
    if let Ok(output) = std::process::Command::new("which").arg("yan").output() {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
    }

    anyhow::bail!("无法确定 yan 二进制路径，请使用 --binary-path 指定")
}

// ── Claude Code setup ──

fn setup_claude(binary: &str, scope: &str) -> Result<()> {
    // 1. Try `claude mcp add`
    let result = std::process::Command::new("claude")
        .args([
            "mcp",
            "add",
            "--transport",
            "stdio",
            "--scope",
            scope,
            "yan-pm",
            "--",
            binary,
            "mcp",
        ])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            println!(
                "  {} MCP Server 已注册 (scope: {scope})",
                "✓".green().bold()
            );
        }
        _ => {
            // Fallback: write ~/.claude.json directly
            write_claude_json_fallback(binary)?;
            println!(
                "  {} MCP Server 已注册 (写入 ~/.claude.json)",
                "✓".green().bold()
            );
        }
    }

    // 2. Install Skill
    install_skill()?;
    println!(
        "  {} Skill 已安装 (~/.claude/skills/yan-pm/SKILL.md)",
        "✓".green().bold()
    );

    Ok(())
}

fn write_claude_json_fallback(binary: &str) -> Result<()> {
    let home = home_dir()?;
    let config_path = home.join(".claude.json");

    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let mcp_servers = config
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert(serde_json::json!({}));

    mcp_servers.as_object_mut().unwrap().insert(
        "yan-pm".to_string(),
        serde_json::json!({
            "type": "stdio",
            "command": binary,
            "args": ["mcp"]
        }),
    );

    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

fn install_skill() -> Result<()> {
    let home = home_dir()?;
    let skill_dir = home.join(".claude/skills/yan-pm");
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(skill_dir.join("SKILL.md"), SKILL_CONTENT)?;
    Ok(())
}

// ── VS Code / Cursor setup ──

fn setup_vscode(binary: &str, scope: &str) -> Result<()> {
    let config_path = if scope == "project" {
        PathBuf::from(".vscode/mcp.json")
    } else {
        home_dir()?.join(".vscode/mcp.json")
    };
    merge_mcp_json(&config_path, binary, "servers")?;
    println!(
        "  {} MCP Server 已配置 ({})",
        "✓".green().bold(),
        config_path.display()
    );
    Ok(())
}

fn setup_cursor(binary: &str) -> Result<()> {
    let config_path = home_dir()?.join(".cursor/mcp.json");
    merge_mcp_json(&config_path, binary, "mcpServers")?;
    println!(
        "  {} MCP Server 已配置 ({})",
        "✓".green().bold(),
        config_path.display()
    );
    Ok(())
}

fn merge_mcp_json(config_path: &Path, binary: &str, servers_key: &str) -> Result<()> {
    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let servers = config
        .as_object_mut()
        .unwrap()
        .entry(servers_key)
        .or_insert(serde_json::json!({}));

    servers.as_object_mut().unwrap().insert(
        "yan-pm".to_string(),
        serde_json::json!({
            "type": "stdio",
            "command": binary,
            "args": ["mcp"]
        }),
    );

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

// ── Uninstall ──

fn remove_claude() -> Result<()> {
    // Try `claude mcp remove`
    let result = std::process::Command::new("claude")
        .args(["mcp", "remove", "yan-pm"])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            println!("  {} MCP Server 已移除", "✓".green().bold());
        }
        _ => {
            // Fallback: remove from ~/.claude.json
            let home = home_dir()?;
            let config_path = home.join(".claude.json");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                if let Ok(mut config) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(servers) = config.get_mut("mcpServers") {
                        if let Some(obj) = servers.as_object_mut() {
                            obj.remove("yan-pm");
                            std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
                        }
                    }
                }
            }
            println!("  {} MCP Server 已移除", "✓".green().bold());
        }
    }

    // Remove Skill
    let home = home_dir()?;
    let skill_dir = home.join(".claude/skills/yan-pm");
    if skill_dir.exists() {
        std::fs::remove_dir_all(&skill_dir)?;
        println!("  {} Skill 已移除", "✓".green().bold());
    }

    Ok(())
}

fn remove_from_mcp_json(config_path: &Path) -> Result<bool> {
    if !config_path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(config_path)?;
    let mut config: serde_json::Value = serde_json::from_str(&content)?;

    let mut removed = false;
    for key in &["servers", "mcpServers"] {
        if let Some(servers) = config.get_mut(*key) {
            if let Some(obj) = servers.as_object_mut() {
                if obj.remove("yan-pm").is_some() {
                    removed = true;
                }
            }
        }
    }

    if removed {
        std::fs::write(config_path, serde_json::to_string_pretty(&config)?)?;
    }
    Ok(removed)
}

fn remove_vscode() -> Result<()> {
    let home = home_dir()?;
    let global = home.join(".vscode/mcp.json");
    let local = PathBuf::from(".vscode/mcp.json");

    let mut any = false;
    if remove_from_mcp_json(&global)? {
        any = true;
    }
    if remove_from_mcp_json(&local)? {
        any = true;
    }
    if any {
        println!("  {} MCP Server 已移除", "✓".green().bold());
    } else {
        println!("  {} 未找到配置", "−".dimmed());
    }
    Ok(())
}

fn remove_cursor() -> Result<()> {
    let config_path = home_dir()?.join(".cursor/mcp.json");
    if remove_from_mcp_json(&config_path)? {
        println!("  {} MCP Server 已移除", "✓".green().bold());
    } else {
        println!("  {} 未找到配置", "−".dimmed());
    }
    Ok(())
}

// ── Status ──

fn check_claude_status() -> (String, String) {
    // MCP
    let mcp_status = if let Ok(output) = std::process::Command::new("claude")
        .args(["mcp", "list"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("yan-pm") {
                format!("{} 已注册", "✓".green().bold())
            } else {
                format!("{} 未注册", "✗".red())
            }
        } else {
            // Fallback: check ~/.claude.json
            check_claude_json_status()
        }
    } else {
        check_claude_json_status()
    };

    // Skill
    let skill_status = if let Ok(home) = home_dir() {
        if home.join(".claude/skills/yan-pm/SKILL.md").exists() {
            format!("{} 已安装", "✓".green().bold())
        } else {
            format!("{} 未安装", "✗".red())
        }
    } else {
        format!("{} 无法检测", "?".yellow())
    };

    (mcp_status, skill_status)
}

fn check_claude_json_status() -> String {
    if let Ok(home) = home_dir() {
        let config_path = home.join(".claude.json");
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                if config
                    .get("mcpServers")
                    .and_then(|s| s.get("yan-pm"))
                    .is_some()
                {
                    return format!("{} 已注册 (claude.json)", "✓".green().bold());
                }
            }
        }
    }
    format!("{} 未注册", "✗".red())
}

fn check_json_mcp_status(config_path: &Path) -> String {
    if !config_path.exists() {
        return format!("{} 未配置", "✗".red());
    }
    if let Ok(content) = std::fs::read_to_string(config_path) {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
            for key in &["servers", "mcpServers"] {
                if config.get(*key).and_then(|s| s.get("yan-pm")).is_some() {
                    return format!("{} 已配置", "✓".green().bold());
                }
            }
        }
    }
    format!("{} 未配置", "✗".red())
}

// ── Public entry points ──

pub async fn install(
    target: Option<&str>,
    binary_path: Option<&str>,
    scope: &str,
    yes: bool,
) -> Result<()> {
    let binary = resolve_binary_path(binary_path)?;

    if let Some(t) = target {
        // Install for specific target
        match t {
            "claude" => {
                println!("{}", "Claude Code:".bold());
                setup_claude(&binary, scope)?;
            }
            "vscode" => {
                println!("{}", "VS Code:".bold());
                setup_vscode(&binary, scope)?;
            }
            "cursor" => {
                println!("{}", "Cursor:".bold());
                setup_cursor(&binary)?;
            }
            _ => anyhow::bail!("不支持的目标: {t}"),
        }
    } else {
        // Auto-detect and install all
        let tools = detect_tools();
        if tools.is_empty() {
            anyhow::bail!("未检测到任何 AI 工具 (Claude Code / VS Code / Cursor)");
        }

        println!("检测到以下 AI 工具:");
        for (i, tool) in tools.iter().enumerate() {
            println!("  [{}] {} ({})", i + 1, tool.name, tool.detail.dimmed());
        }
        println!();

        if !yes {
            println!("将为以上工具配置 yan MCP Server。确认? [Y/n]");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();
            if input == "n" || input == "no" {
                println!("已取消");
                return Ok(());
            }
        }

        println!();
        for tool in &tools {
            println!("{}:", tool.name.bold());
            match tool.kind {
                ToolKind::Claude => setup_claude(&binary, scope)?,
                ToolKind::Vscode => setup_vscode(&binary, scope)?,
                ToolKind::Cursor => setup_cursor(&binary)?,
            }
        }
    }

    println!();
    println!("{}", "安装完成! 重启 AI 工具后即可使用。".green().bold());
    println!("试试在 Claude Code 中说: {}", "\"查看我的待办任务\"".cyan());
    Ok(())
}

pub async fn uninstall(target: Option<&str>) -> Result<()> {
    if let Some(t) = target {
        match t {
            "claude" => {
                println!("{}", "Claude Code:".bold());
                remove_claude()?;
            }
            "vscode" => {
                println!("{}", "VS Code:".bold());
                remove_vscode()?;
            }
            "cursor" => {
                println!("{}", "Cursor:".bold());
                remove_cursor()?;
            }
            _ => anyhow::bail!("不支持的目标: {t}"),
        }
    } else {
        println!("{}", "卸载 yan 配置...".bold());
        println!();
        println!("{}", "Claude Code:".bold());
        remove_claude()?;
        println!("{}", "VS Code:".bold());
        remove_vscode()?;
        println!("{}", "Cursor:".bold());
        remove_cursor()?;
    }

    println!();
    println!("{}", "卸载完成。".green().bold());
    Ok(())
}

pub async fn status() -> Result<()> {
    println!("{}", "yan setup 状态:".bold());
    println!();

    // Claude Code
    println!("  {}:", "Claude Code".bold());
    let (mcp, skill) = check_claude_status();
    println!("    MCP Server: {mcp}");
    println!("    Skill:      {skill}");

    // VS Code
    println!("  {}:", "VS Code".bold());
    if let Ok(home) = home_dir() {
        let status = check_json_mcp_status(&home.join(".vscode/mcp.json"));
        println!("    MCP Server: {status}");
    }

    // Cursor
    println!("  {}:", "Cursor".bold());
    if let Ok(home) = home_dir() {
        if home.join(".cursor").exists() {
            let status = check_json_mcp_status(&home.join(".cursor/mcp.json"));
            println!("    MCP Server: {status}");
        } else {
            println!("    {}", "未检测到 Cursor".dimmed());
        }
    }

    Ok(())
}
