use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Global config stored at ~/.config/yan-pm/config.json
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,
}

/// Resolved config with all values present
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub base_url: String,
    pub token: String,
}

/// Config directory: ~/.config/yan/ on all platforms
/// Falls back to ~/.config/yan-pm-cli/ or ~/.config/yan-pm/ if the new dir doesn't exist (migration)
pub fn config_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    let base = PathBuf::from(home).join(".config");
    let new_dir = base.join("yan");
    let legacy_dir = base.join("yan-pm-cli");
    let old_dir = base.join("yan-pm");
    if new_dir.exists() {
        new_dir
    } else if legacy_dir.exists() {
        legacy_dir
    } else if old_dir.exists() {
        old_dir
    } else {
        new_dir
    }
}

fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load_config() -> GlobalConfig {
    load_config_from(&config_file())
}

fn load_config_from(path: &std::path::Path) -> GlobalConfig {
    if !path.exists() {
        return GlobalConfig::default();
    }
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => GlobalConfig::default(),
    }
}

pub fn save_config(updates: &GlobalConfig) -> Result<()> {
    save_config_to(updates, &config_dir(), &config_file())
}

fn save_config_to(
    updates: &GlobalConfig,
    dir: &std::path::Path,
    path: &std::path::Path,
) -> Result<()> {
    fs::create_dir_all(dir).context("Failed to create config directory")?;

    let mut existing = load_config_from(path);
    if let Some(url) = &updates.base_url {
        existing.base_url = Some(url.clone());
    }
    if let Some(token) = &updates.token {
        existing.token = Some(token.clone());
    }
    if let Some(mid) = &updates.machine_id {
        existing.machine_id = Some(mid.clone());
    }

    let content = serde_json::to_string_pretty(&existing)? + "\n";
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &content)?;

    // Set file permissions to 0600 on Unix (contains token) — before rename to avoid race
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
    }

    fs::rename(&tmp_path, path)?;

    Ok(())
}

/// Resolve config with priority: CLI args > env vars > config file
pub fn resolve_config(cli_url: Option<&str>, cli_token: Option<&str>) -> ResolvedConfig {
    let file = load_config();
    let base_url = cli_url
        .map(String::from)
        .or_else(|| std::env::var("YAN_PM_BASE_URL").ok())
        .or(file.base_url)
        .unwrap_or_default();
    let token = cli_token
        .map(String::from)
        .or_else(|| std::env::var("YAN_PM_TOKEN").ok())
        .or(file.token)
        .unwrap_or_default();
    ResolvedConfig { base_url, token }
}

/// Get or generate machine ID
pub fn get_machine_id() -> String {
    let config = load_config();
    if let Some(mid) = config.machine_id {
        return mid;
    }
    let host = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let random_hex: String = (0..3)
        .map(|_| format!("{:02x}", rand::random::<u8>()))
        .collect();
    let id = format!("{host}-{random_hex}");
    let _ = save_config(&GlobalConfig {
        machine_id: Some(id.clone()),
        ..Default::default()
    });
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_config_serde_roundtrip() {
        let config = GlobalConfig {
            base_url: Some("https://example.com".into()),
            token: Some("tok_123".into()),
            machine_id: Some("mac-abc".into()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: GlobalConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.base_url.as_deref(), Some("https://example.com"));
        assert_eq!(parsed.token.as_deref(), Some("tok_123"));
        assert_eq!(parsed.machine_id.as_deref(), Some("mac-abc"));
    }

    #[test]
    fn test_global_config_camel_case() {
        let json = r#"{"baseUrl":"http://a.com","token":"t","machineId":"m"}"#;
        let config: GlobalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.base_url.as_deref(), Some("http://a.com"));
        assert_eq!(config.machine_id.as_deref(), Some("m"));
    }

    #[test]
    fn test_global_config_default_is_empty() {
        let config = GlobalConfig::default();
        assert!(config.base_url.is_none());
        assert!(config.token.is_none());
        assert!(config.machine_id.is_none());
    }

    #[test]
    fn test_global_config_skip_serializing_none() {
        let config = GlobalConfig {
            base_url: Some("http://x.com".into()),
            token: None,
            machine_id: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("baseUrl"));
        assert!(!json.contains("token"));
        assert!(!json.contains("machineId"));
    }

    #[test]
    fn test_resolve_config_cli_overrides_file() {
        // CLI args take priority over everything
        let resolved = resolve_config(Some("http://cli.com"), Some("cli-token"));
        assert_eq!(resolved.base_url, "http://cli.com");
        assert_eq!(resolved.token, "cli-token");
    }

    #[test]
    fn test_save_and_load_config() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".config").join("yan-pm");
        let file = dir.join("config.json");

        let updates = GlobalConfig {
            base_url: Some("http://test.com".into()),
            token: Some("test-token".into()),
            machine_id: None,
        };
        save_config_to(&updates, &dir, &file).unwrap();

        let loaded = load_config_from(&file);
        assert_eq!(loaded.base_url.as_deref(), Some("http://test.com"));
        assert_eq!(loaded.token.as_deref(), Some("test-token"));
    }

    #[test]
    fn test_save_config_merges() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".config").join("yan-pm");
        let file = dir.join("config.json");

        // First save
        save_config_to(
            &GlobalConfig {
                base_url: Some("http://a.com".into()),
                token: None,
                machine_id: None,
            },
            &dir,
            &file,
        )
        .unwrap();

        // Second save — should merge, not overwrite
        save_config_to(
            &GlobalConfig {
                base_url: None,
                token: Some("new-token".into()),
                machine_id: None,
            },
            &dir,
            &file,
        )
        .unwrap();

        let loaded = load_config_from(&file);
        assert_eq!(loaded.base_url.as_deref(), Some("http://a.com"));
        assert_eq!(loaded.token.as_deref(), Some("new-token"));
    }
}
