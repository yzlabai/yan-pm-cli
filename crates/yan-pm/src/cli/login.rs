use anyhow::Result;
use colored::Colorize;

use crate::api::client::ApiClient;
use crate::config;

/// Login — direct token mode or Device Authorization flow (RFC 8628)
pub async fn run(direct_token: Option<&str>) -> Result<()> {
    let resolved = config::resolve_config(None, None);
    let base_url = if resolved.base_url.is_empty() {
        eprintln!("请输入 YanChat 服务器地址 (如 https://your-domain.com):");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let url = input.trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            anyhow::bail!("服务器地址不能为空");
        }
        url
    } else {
        resolved.base_url.clone()
    };

    // Direct token mode: skip device flow
    if let Some(token) = direct_token {
        let cfg = config::GlobalConfig {
            base_url: Some(base_url.clone()),
            token: Some(token.to_string()),
            machine_id: Some(config::get_machine_id()),
        };
        config::save_config(&cfg)?;
        println!("{}", "✓ 配置已保存".green().bold());
        return Ok(());
    }

    do_login(&base_url).await
}

async fn do_login(base_url: &str) -> Result<()> {
    println!("{}", "正在请求设备码...".dimmed());

    let device = ApiClient::device_code_request(base_url).await?;

    // Prefer verification_uri_complete (pre-filled code)
    let open_url = device
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&device.verification_uri);

    println!();
    println!("请在浏览器中完成授权：");
    println!();
    println!("  验证码: {}", device.user_code.cyan().bold());
    println!("  链接:   {}", open_url.underline());
    println!();

    // Try to open browser
    let _ = open::that(open_url);
    println!(
        "{}",
        "已自动打开浏览器，如未打开请手动访问上方链接".dimmed()
    );

    println!("{}", "等待授权...".dimmed());

    let base_interval = std::time::Duration::from_secs(device.interval);
    let mut current_interval = base_interval;
    let expires_at = std::time::Instant::now() + std::time::Duration::from_secs(device.expires_in);

    loop {
        if std::time::Instant::now() > expires_at {
            anyhow::bail!("设备码已过期，请重新运行 login");
        }

        tokio::time::sleep(current_interval).await;

        match ApiClient::device_code_poll(base_url, &device.device_code).await {
            Ok(token_resp) => {
                if let Some(error) = &token_resp.error {
                    if error == "authorization_pending" {
                        continue;
                    }
                    if error == "slow_down" {
                        // RFC 8628 §3.5: increase interval by 5 seconds
                        current_interval += std::time::Duration::from_secs(5);
                        continue;
                    }
                    anyhow::bail!("授权失败: {error}");
                }

                let access_token = token_resp
                    .access_token
                    .ok_or_else(|| anyhow::anyhow!("未获取到 token"))?;

                // Save config
                let cfg = config::GlobalConfig {
                    base_url: Some(base_url.to_string()),
                    token: Some(access_token),
                    machine_id: Some(config::get_machine_id()),
                };
                config::save_config(&cfg)?;

                println!("{}", "✓ 登录成功！".green().bold());
                println!(
                    "  Token 已保存到 {}",
                    config::config_dir().join("config.json").display()
                );
                return Ok(());
            }
            Err(e) => {
                // Network/parse error — show to user but keep retrying
                eprintln!("{}", format!("轮询出错: {e}，继续重试...").dimmed());
                continue;
            }
        }
    }
}
