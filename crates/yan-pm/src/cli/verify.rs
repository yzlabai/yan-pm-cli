use anyhow::Result;

use crate::local::directory::LocalDirectory;

/// Result of running a single verification command
struct CheckResult {
    command: String,
    passed: bool,
    output: String,
}

pub async fn handle_verify(issue_number: i32) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let local_dir = LocalDirectory::new(&cwd);

    if !local_dir.is_initialized() {
        anyhow::bail!("当前目录未初始化。请先运行: yan pull");
    }

    // Read the spec
    let spec = local_dir.find_spec_by_issue(issue_number)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Issue #{} 的 Spec 不存在。请先运行: yan spec {}",
            issue_number,
            issue_number
        )
    })?;

    println!(
        "🔍 验证 Issue #{}: {}\n",
        issue_number, spec.frontmatter.title
    );

    // Run auto-detected verification commands
    let checks = detect_and_run_checks(&cwd).await;

    // Print check results
    let total_checks = checks.len();
    let passed_checks = checks.iter().filter(|c| c.passed).count();

    for check in &checks {
        let icon = if check.passed { "✓" } else { "✗" };
        let status = if check.passed { "passed" } else { "FAILED" };
        println!("{} {} — {}", icon, check.command, status);
        if !check.passed && !check.output.is_empty() {
            // Show truncated output for failures
            let display = if check.output.len() > 500 {
                &check.output[..500]
            } else {
                &check.output
            };
            for line in display.lines().take(10) {
                println!("  {}", line);
            }
        }
    }

    // Extract acceptance criteria from spec
    let criteria = extract_acceptance_criteria(&spec.body);

    if !criteria.is_empty() {
        println!("\n验收标准:");
        for ac in &criteria {
            println!("- [ ] {} — 需人工验证", ac);
        }
    }

    // Summary
    println!();
    if total_checks > 0 {
        println!("自动检查: {}/{} 通过", passed_checks, total_checks);
    } else {
        println!("自动检查: 未检测到可运行的检查命令");
    }
    if !criteria.is_empty() {
        println!("验收标准: {} 项需人工验证", criteria.len());
    }

    if passed_checks < total_checks {
        anyhow::bail!(
            "部分检查未通过 ({}/{})",
            total_checks - passed_checks,
            total_checks
        );
    }

    Ok(())
}

/// Detect project type and run appropriate verification commands
async fn detect_and_run_checks(cwd: &std::path::Path) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    // Rust project
    if cwd.join("Cargo.toml").exists() {
        checks.push(run_check(cwd, "cargo test").await);
        checks.push(run_check(cwd, "cargo clippy").await);
    }

    // Node.js project
    if cwd.join("package.json").exists() {
        // Check for test and lint scripts
        if let Ok(content) = std::fs::read_to_string(cwd.join("package.json")) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(scripts) = pkg.get("scripts").and_then(|s| s.as_object()) {
                    if scripts.contains_key("test") {
                        // Detect package manager
                        let pm = detect_node_pm(cwd);
                        checks.push(run_check(cwd, &format!("{} test", pm)).await);
                    }
                    if scripts.contains_key("lint") {
                        let pm = detect_node_pm(cwd);
                        checks.push(run_check(cwd, &format!("{} run lint", pm)).await);
                    }
                }
            }
        }
    }

    // Makefile project
    if cwd.join("Makefile").exists() || cwd.join("makefile").exists() {
        // Check if 'test' target exists
        if let Ok(content) = std::fs::read_to_string(cwd.join("Makefile"))
            .or_else(|_| std::fs::read_to_string(cwd.join("makefile")))
        {
            if content.contains("test:") {
                checks.push(run_check(cwd, "make test").await);
            }
        }
    }

    checks
}

/// Detect Node.js package manager (pnpm > yarn > npm)
fn detect_node_pm(cwd: &std::path::Path) -> &'static str {
    if cwd.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if cwd.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    }
}

/// Run a shell command and capture its result
async fn run_check(cwd: &std::path::Path, command: &str) -> CheckResult {
    let result = tokio::process::Command::new("sh")
        .args(["-c", command])
        .current_dir(cwd)
        .output()
        .await;

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n{}", stdout, stderr)
            };
            CheckResult {
                command: command.to_string(),
                passed: output.status.success(),
                output: combined,
            }
        }
        Err(e) => CheckResult {
            command: command.to_string(),
            passed: false,
            output: format!("命令执行失败: {}", e),
        },
    }
}

/// Extract acceptance criteria lines from spec body
fn extract_acceptance_criteria(body: &str) -> Vec<String> {
    let mut criteria = Vec::new();
    let mut in_ac_section = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## 验收标准") {
            in_ac_section = true;
            continue;
        }
        if in_ac_section && trimmed.starts_with("## ") {
            // Reached next section
            break;
        }
        if in_ac_section {
            // Match lines like "- [ ] AC text" or "- [x] AC text"
            let stripped = trimmed
                .strip_prefix("- [ ] ")
                .or_else(|| trimmed.strip_prefix("- [x] "))
                .or_else(|| trimmed.strip_prefix("- [X] "));
            if let Some(text) = stripped {
                if !text.is_empty() {
                    criteria.push(text.to_string());
                }
            }
        }
    }

    criteria
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_acceptance_criteria() {
        let body = r#"## 背景

Some background.

## 验收标准

- [ ] AC1: 支持 Google OAuth 登录
- [ ] AC2: Token 刷新逻辑正确
- [x] AC3: 已完成的标准

## 任务拆分

- [ ] Task 1
"#;
        let criteria = extract_acceptance_criteria(body);
        assert_eq!(criteria.len(), 3);
        assert_eq!(criteria[0], "AC1: 支持 Google OAuth 登录");
        assert_eq!(criteria[1], "AC2: Token 刷新逻辑正确");
        assert_eq!(criteria[2], "AC3: 已完成的标准");
    }

    #[test]
    fn test_extract_empty_criteria() {
        let body = "## 背景\n\nSome text.\n";
        let criteria = extract_acceptance_criteria(body);
        assert!(criteria.is_empty());
    }

    #[test]
    fn test_detect_node_pm() {
        let tmp = tempfile::tempdir().unwrap();
        // No lock file → npm
        assert_eq!(detect_node_pm(tmp.path()), "npm");

        // pnpm
        std::fs::write(tmp.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(detect_node_pm(tmp.path()), "pnpm");
    }
}
