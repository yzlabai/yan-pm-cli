#![allow(
    clippy::too_many_arguments,
    clippy::module_inception,
    dead_code,
    unused_imports
)]

mod agent;
mod api;
mod cli;
mod config;
mod daemon;
mod local;
mod mcp;
mod output;
mod runner;
mod sync;
mod tui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "yan",
    about = "yan.chat CLI — 需求管理 + AI Agent 执行 + MCP 桥接",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// API server URL
    #[arg(long, global = true, env = "YAN_PM_BASE_URL")]
    url: Option<String>,

    /// Auth token
    #[arg(long, global = true, env = "YAN_PM_TOKEN")]
    token: Option<String>,

    /// Output as JSON
    #[arg(long, global = true, default_value_t = false)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// 浏览器登录获取 token（或使用 --token 手动配置）
    Login {
        /// 直接提供 Token（跳过浏览器流程）
        #[arg(long)]
        token: Option<String>,
    },
    /// 列出所有项目
    List,
    /// Issue（需求）管理
    Issue {
        #[command(subcommand)]
        action: IssueAction,
    },
    /// 列出本地任务文件
    Tasks {
        /// Issue 编号（筛选该 Issue 的 tasks）
        issue_number: Option<i32>,
        /// 强制从本地文件读取
        #[arg(long)]
        local: bool,
    },
    /// 从云端拉取最新 Issue
    Pull,
    /// 为 Issue 生成技术规格
    Spec {
        /// Issue 编号
        issue_number: i32,
        /// 使用 AI 自动生成 Spec 内容
        #[arg(long)]
        ai: bool,
        /// Agent 名称 (claude/codex/gemini)
        #[arg(long, default_value = "claude")]
        agent: String,
    },
    /// 验证实现
    Verify {
        /// Issue 编号
        issue_number: i32,
    },
    /// 关联当前目录到项目
    Link {
        /// 项目 slug 或 ID
        project_id: String,
        /// 自定义工作区路径（默认当前目录）
        #[arg(long)]
        path: Option<String>,
        /// 自定义工作区名称
        #[arg(long)]
        name: Option<String>,
    },
    /// 取消当前目录的项目关联
    Unlink,
    /// 列出项目工作区
    Workspaces {
        /// 项目 slug 或 ID
        project_id: String,
    },
    /// 显示当前目录的项目信息
    Info,
    /// 启动 AI Agent 执行任务
    Start {
        /// Issue 编号（执行该 Issue 下的 Task）
        #[arg(long)]
        issue: Option<i32>,
        /// 指定任务编号（如 001-01）
        #[arg(long)]
        task: Option<String>,
        /// Agent 名称 (claude/codex/gemini)
        #[arg(long, default_value = "claude")]
        agent: String,
        /// 权限模式 (auto/plan/deny)
        #[arg(long, default_value = "auto")]
        permission_mode: String,
        /// 显示详细输出
        #[arg(long)]
        verbose: bool,
    },
    /// 启动 MCP stdio 服务
    Mcp,
    /// 手动同步本地任务文件与云端
    Sync,
    /// 列出可用的 AI Agent
    Agents {
        /// 仅显示正在执行的 agent
        #[arg(long)]
        running: bool,
    },
    /// 全局 Dashboard：纵览所有 workspace 状态
    Dashboard {
        /// 紧凑模式（单行 per workspace）
        #[arg(long)]
        compact: bool,
        /// TUI 实时模式
        #[arg(long)]
        live: bool,
    },
    /// 自动执行任务配置
    #[command(name = "auto-run")]
    AutoRun {
        #[command(subcommand)]
        action: AutoRunAction,
    },
    /// Daemon 守护进程管理
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// 项目管理
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// 安装 yan-pm 到 AI 工具（Claude Code / VS Code / Cursor）
    Setup {
        /// 指定目标工具 (claude/vscode/cursor)
        #[arg(long)]
        target: Option<String>,
        /// 卸载配置
        #[arg(long)]
        uninstall: bool,
        /// 查看安装状态
        #[arg(long)]
        status: bool,
        /// 手动指定 yan 二进制路径
        #[arg(long)]
        binary_path: Option<String>,
        /// MCP 注册范围 (user/project，仅 Claude Code)
        #[arg(long, default_value = "user")]
        scope: String,
        /// 跳过确认提示
        #[arg(long)]
        yes: bool,
    },
    /// 自更新到最新版本
    SelfUpdate,
}

#[derive(Subcommand)]
enum IssueAction {
    /// 列出项目需求
    List {
        /// 项目 slug 或 ID
        project_id: String,
        /// 按状态筛选
        #[arg(long)]
        status: Option<String>,
        /// 按类型筛选
        #[arg(long = "type")]
        issue_type: Option<String>,
        /// 按优先级筛选
        #[arg(long)]
        priority: Option<String>,
        /// 关键词搜索
        #[arg(long, alias = "search")]
        keyword: Option<String>,
    },
    /// 查看需求详情
    Show {
        /// 项目 slug 或 ID
        project_id: String,
        /// 需求 ID
        issue_id: String,
    },
    /// 创建新需求
    Create {
        /// 项目 slug 或 ID
        project_id: String,
        /// 需求标题
        #[arg(long)]
        title: String,
        /// 需求描述
        #[arg(long, alias = "desc")]
        description: Option<String>,
        /// 类型 (feature/bug/improvement/question)
        #[arg(long = "type")]
        issue_type: Option<String>,
        /// 优先级
        #[arg(long)]
        priority: Option<String>,
        /// 负责人 ID
        #[arg(long)]
        assignee: Option<String>,
        /// 标签 (逗号分隔)
        #[arg(long)]
        labels: Option<String>,
    },
    /// 更新需求
    Update {
        /// 项目 slug 或 ID
        project_id: String,
        /// 需求 ID
        issue_id: String,
        /// 新标题
        #[arg(long)]
        title: Option<String>,
        /// 新状态
        #[arg(long)]
        status: Option<String>,
        /// 新优先级
        #[arg(long)]
        priority: Option<String>,
        /// 新类型
        #[arg(long = "type")]
        issue_type: Option<String>,
        /// 负责人 ID
        #[arg(long)]
        assignee: Option<String>,
        /// 新标签 (逗号分隔)
        #[arg(long)]
        labels: Option<String>,
    },
    /// 接受需求
    Accept {
        /// 项目 slug 或 ID
        project_id: String,
        /// 需求 ID
        issue_id: String,
    },
    /// 交付需求
    Deliver {
        /// 项目 slug 或 ID
        project_id: String,
        /// 需求 ID
        issue_id: String,
        /// 交付总结
        #[arg(long)]
        summary: Option<String>,
    },
}

#[derive(Subcommand)]
enum AutoRunAction {
    /// 启用 auto-run
    On {
        /// 预算限制 (USD)
        #[arg(long)]
        budget: Option<f64>,
        /// 最大并发数
        #[arg(long)]
        concurrency: Option<u32>,
        /// 优先级过滤 (逗号分隔: urgent,high,medium,low)
        #[arg(long)]
        filter_priority: Option<String>,
        /// Agent 名称 (claude/codex/gemini)
        #[arg(long)]
        agent: Option<String>,
    },
    /// 禁用 auto-run
    Off,
    /// 查看 auto-run 状态
    Status,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// 启动 daemon
    Start {
        /// 在前台运行（不 fork）
        #[arg(long)]
        foreground: bool,
    },
    /// 停止 daemon
    Stop,
    /// 重启 daemon
    Restart,
    /// 查看 daemon 状态
    Status,
    /// 查看日志
    Logs {
        /// 持续跟踪日志
        #[arg(short, long)]
        follow: bool,
    },
    /// 注册系统服务（开机自启）
    Install,
    /// 卸载系统服务
    Uninstall,
}

#[derive(Subcommand)]
enum ProjectAction {
    /// 同步本地项目信息到云端（repoUrl / techStack / CLAUDE.md）
    SyncInfo {
        /// 跳过确认，直接上传
        #[arg(long)]
        yes: bool,
        /// 只展示 diff，不上传
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Skip default tracing init for daemon foreground mode — it sets up its own file logger
    let is_daemon_foreground = matches!(
        &cli.command,
        Commands::Daemon {
            action: DaemonAction::Start { foreground: true }
        }
    );
    if !is_daemon_foreground {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
            )
            .with_writer(std::io::stderr)
            .init();
    }

    let result = match cli.command {
        Commands::Login { token } => cli::login::run(token.as_deref()).await,
        Commands::List => {
            cli::project::list(cli.url.as_deref(), cli.token.as_deref(), cli.json).await
        }
        Commands::Issue { action } => {
            cli::issue::handle(cli.url.as_deref(), cli.token.as_deref(), cli.json, action).await
        }
        Commands::Tasks {
            issue_number,
            local: _,
        } => cli::task::list_local(cli.json, issue_number).await,
        Commands::Pull => {
            cli::pull::handle_pull(cli.url.as_deref(), cli.token.as_deref(), cli.json).await
        }
        Commands::Spec {
            issue_number,
            ai,
            agent,
        } => cli::spec::handle_spec(issue_number, cli.json, ai, &agent).await,
        Commands::Verify { issue_number } => cli::verify::handle_verify(issue_number).await,
        Commands::Link {
            project_id,
            path,
            name,
        } => {
            cli::workspace::link(
                cli.url.as_deref(),
                cli.token.as_deref(),
                &project_id,
                path.as_deref(),
                name.as_deref(),
            )
            .await
        }
        Commands::Unlink => cli::workspace::unlink(cli.url.as_deref(), cli.token.as_deref()).await,
        Commands::Workspaces { project_id } => {
            cli::workspace::list(
                cli.url.as_deref(),
                cli.token.as_deref(),
                cli.json,
                &project_id,
            )
            .await
        }
        Commands::Info => {
            cli::workspace::info(cli.url.as_deref(), cli.token.as_deref(), cli.json).await
        }
        Commands::Start {
            issue,
            task,
            agent,
            permission_mode,
            verbose,
        } => cli::start::run(issue, task.as_deref(), &agent, &permission_mode, verbose).await,
        Commands::Mcp => mcp::start_mcp_server().await,
        Commands::Sync => cli::sync::run(cli.url.as_deref(), cli.token.as_deref()).await,
        Commands::Agents { running } => cli::agents::run(running, cli.json).await,
        Commands::Dashboard { compact, live } => cli::dashboard::run(cli.json, compact, live).await,
        Commands::AutoRun { action } => match action {
            AutoRunAction::On {
                budget,
                concurrency,
                filter_priority,
                agent,
            } => cli::auto_run::enable(
                budget,
                concurrency,
                filter_priority.as_deref(),
                agent.as_deref(),
            ),
            AutoRunAction::Off => cli::auto_run::disable(),
            AutoRunAction::Status => cli::auto_run::status(),
        },
        Commands::Project { action } => match action {
            ProjectAction::SyncInfo { yes, dry_run } => {
                cli::project::sync_info(
                    cli.url.as_deref(),
                    cli.token.as_deref(),
                    cli.json,
                    yes,
                    dry_run,
                )
                .await
            }
        },
        Commands::Setup {
            target,
            uninstall,
            status,
            binary_path,
            scope,
            yes,
        } => {
            if status {
                cli::setup::status().await
            } else if uninstall {
                cli::setup::uninstall(target.as_deref()).await
            } else {
                cli::setup::install(target.as_deref(), binary_path.as_deref(), &scope, yes).await
            }
        }
        Commands::SelfUpdate => cli::self_update::run().await,
        Commands::Daemon { action } => match action {
            DaemonAction::Start { foreground } => {
                cli::daemon::start(cli.url.as_deref(), cli.token.as_deref(), foreground).await
            }
            DaemonAction::Stop => cli::daemon::stop(),
            DaemonAction::Restart => cli::daemon::restart(cli.url.as_deref(), cli.token.as_deref()),
            DaemonAction::Status => cli::daemon::status(),
            DaemonAction::Logs { follow } => cli::daemon::logs(follow),
            DaemonAction::Install => cli::daemon::install(),
            DaemonAction::Uninstall => cli::daemon::uninstall(),
        },
    };

    if let Err(e) = result {
        eprintln!("❌ {e}");
        std::process::exit(1);
    }
}
