use std::path::Path;
use std::process::Command;

use anyhow::Result;

/// 自动检测到的项目信息
#[derive(Debug, Clone)]
pub struct DetectedInfo {
    pub repo_url: Option<String>,
    pub tech_stack: Vec<String>,
    pub ai_context: Option<String>,
    pub ai_context_source: Option<String>,
}

const AI_CONTEXT_MAX_CHARS: usize = 10000;

/// AI 上下文文件优先级
const AI_CONTEXT_FILES: &[&str] = &["CLAUDE.md", "AGENTS.md", "GEMINI.md"];

/// 从当前工作目录自动检测项目信息
pub fn detect(work_dir: &Path) -> Result<DetectedInfo> {
    let (ai_context, ai_context_source) = match detect_ai_context(work_dir) {
        Some((ctx, src)) => (Some(ctx), Some(src)),
        None => (None, None),
    };
    Ok(DetectedInfo {
        repo_url: detect_repo_url(work_dir),
        tech_stack: detect_tech_stack(work_dir),
        ai_context,
        ai_context_source,
    })
}

/// git remote get-url origin
fn detect_repo_url(work_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(work_dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() { None } else { Some(url) }
}

/// 粗粒度技术栈检测
fn detect_tech_stack(work_dir: &Path) -> Vec<String> {
    let mut stack = Vec::new();

    // TypeScript
    if work_dir.join("tsconfig.json").exists() {
        stack.push("TypeScript".to_string());
    }

    // package.json dependencies
    if let Some(deps) = read_package_json_deps(work_dir) {
        let dep_checks: &[(&str, &str)] = &[
            ("react", "React"),
            ("vue", "Vue"),
            ("next", "Next.js"),
            ("nuxt", "Nuxt"),
            ("hono", "Hono"),
            ("express", "Express"),
            ("drizzle-orm", "Drizzle ORM"),
            ("prisma", "Prisma"),
            ("@prisma/client", "Prisma"),
            ("tailwindcss", "Tailwind CSS"),
        ];
        for (pkg, name) in dep_checks {
            if deps.contains(&pkg.to_string()) && !stack.contains(&name.to_string()) {
                stack.push(name.to_string());
            }
        }
    }

    // pnpm monorepo
    if work_dir.join("pnpm-workspace.yaml").exists() {
        stack.push("pnpm monorepo".to_string());
    }

    // Rust
    if work_dir.join("Cargo.toml").exists() {
        stack.push("Rust".to_string());
    }

    // Go
    if work_dir.join("go.mod").exists() {
        stack.push("Go".to_string());
    }

    // Python
    if work_dir.join("pyproject.toml").exists() || work_dir.join("requirements.txt").exists() {
        stack.push("Python".to_string());
    }

    // Docker
    if work_dir.join("Dockerfile").exists() {
        stack.push("Docker".to_string());
    }

    // docker-compose services
    detect_compose_services(work_dir, &mut stack);

    stack
}

/// 从 package.json 读取所有 dependency 名
fn read_package_json_deps(work_dir: &Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(work_dir.join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let mut deps = Vec::new();
    for key in ["dependencies", "devDependencies"] {
        if let Some(obj) = json.get(key).and_then(|v| v.as_object()) {
            for k in obj.keys() {
                deps.push(k.clone());
            }
        }
    }
    Some(deps)
}

/// 从 docker-compose 文件检测常用服务
fn detect_compose_services(work_dir: &Path, stack: &mut Vec<String>) {
    let candidates = ["docker-compose.yml", "docker-compose.yaml", "docker-compose.dev.yml"];
    for name in candidates {
        let path = work_dir.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let content_lower = content.to_lowercase();
            if content_lower.contains("postgres") {
                if !stack.contains(&"PostgreSQL".to_string()) {
                    stack.push("PostgreSQL".to_string());
                }
            }
            if content_lower.contains("redis") || content_lower.contains("valkey") {
                if !stack.contains(&"Redis".to_string()) {
                    stack.push("Redis".to_string());
                }
            }
            if content_lower.contains("minio") {
                if !stack.contains(&"MinIO".to_string()) {
                    stack.push("MinIO".to_string());
                }
            }
        }
    }
}

/// 按优先级查找 AI 上下文文件，返回 (内容, 文件名)
fn detect_ai_context(work_dir: &Path) -> Option<(String, String)> {
    for &name in AI_CONTEXT_FILES {
        let path = work_dir.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.trim().is_empty() {
                continue;
            }
            let truncated = truncate_at_boundary(&content, AI_CONTEXT_MAX_CHARS);
            return Some((truncated, name.to_string()));
        }
    }
    None
}

/// 在段落/标题边界截断，避免切断 markdown 结构
fn truncate_at_boundary(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }
    // 按字符数找到安全的字节偏移
    let byte_offset = content
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(content.len());
    let slice = &content[..byte_offset];
    // 从截断点往前找最近的标题或空行边界
    if let Some(pos) = slice.rfind("\n## ") {
        return content[..pos].to_string();
    }
    if let Some(pos) = slice.rfind("\n\n") {
        return content[..pos].to_string();
    }
    // 没找到好的边界，在最近的换行符截断
    if let Some(pos) = slice.rfind('\n') {
        return content[..pos].to_string();
    }
    slice.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_at_boundary_no_truncation() {
        let content = "短文本";
        assert_eq!(truncate_at_boundary(content, 100), content);
    }

    #[test]
    fn test_truncate_at_heading_boundary() {
        let content = "# Title\n\nFirst section content.\n\n## Second\n\nMore content here that goes on and on and on.";
        // 截断点在 "## Second" 之前
        let result = truncate_at_boundary(content, 60);
        assert!(!result.contains("## Second"));
        assert!(result.contains("First section content."));
    }

    #[test]
    fn test_truncate_at_paragraph_boundary() {
        let content = "Line one.\n\nParagraph two.\n\nParagraph three is longer.";
        let result = truncate_at_boundary(content, 30);
        assert!(result.ends_with("Paragraph two."));
    }
}
