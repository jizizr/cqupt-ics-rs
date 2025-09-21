mod cache;
mod commands;
mod registry;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "cqupt-ics")]
#[command(about = "CQUPT课程表导出工具")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 启用详细日志
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// 获取课程表并生成ICS文件
    Generate {
        /// 数据provider (jwzxdirect, redrock, wecqupt)
        #[arg(short, long)]
        provider: String,

        /// 用户名/学号
        #[arg(short, long)]
        username: String,

        /// 密码
        #[arg(short = 'P', long)]
        password: String,

        /// 学期开始日期（格式：YYYY-MM-DD，如 2024-03-04）
        #[arg(short = 's', long)]
        start_date: Option<String>,

        /// 输出文件路径
        #[arg(short, long)]
        output: Option<String>,

        /// 日历名称
        #[arg(long)]
        calendar_name: Option<String>,

        /// 是否包含教师信息
        #[arg(long, default_value = "true")]
        include_teacher: bool,

        /// 提醒时间（分钟）
        #[arg(long, default_value = "15")]
        reminder_minutes: u32,
    },

    /// 验证用户凭据
    Validate {
        /// 数据provider
        #[arg(short, long)]
        provider: String,

        /// 用户名/学号
        #[arg(short, long)]
        username: String,

        /// 密码
        #[arg(short = 'P', long)]
        password: String,
    },

    /// 列出可用的数据provider
    Providers,

    /// 位置管理相关命令
    Location {
        #[command(subcommand)]
        action: LocationCommands,
    },
}

#[derive(Subcommand)]
enum LocationCommands {
    /// 列出所有位置映射
    List,

    /// 标准化位置名称
    Normalize {
        /// 原始位置名称
        location: String,
    },

    /// 从JSON文件导入位置映射
    Import {
        /// JSON文件路径
        file: String,
    },

    /// 导出位置映射到JSON文件
    Export {
        /// 输出文件路径
        file: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    registry::init();
    let cli = Cli::parse();

    // 设置日志级别
    let log_level = if cli.verbose { "debug" } else { "info" };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("cqupt_ics_cli={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    match cli.command {
        Commands::Generate {
            provider,
            username,
            password,
            start_date,
            output,
            calendar_name,
            include_teacher,
            reminder_minutes,
        } => {
            commands::generate_command(commands::GenerateParams {
                provider_name: provider,
                username,
                password,
                start_date,
                output,
                calendar_name,
                include_teacher,
                reminder_minutes,
            })
            .await
        }

        Commands::Validate {
            provider,
            username,
            password,
        } => commands::validate_command(provider, username, password).await,

        Commands::Providers => commands::providers_command().await,

        Commands::Location { action } => match action {
            LocationCommands::List => commands::location_list_command().await,
            LocationCommands::Normalize { location } => {
                commands::location_normalize_command(location).await
            }
            LocationCommands::Import { file } => commands::location_import_command(file).await,
            LocationCommands::Export { file } => commands::location_export_command(file).await,
        },
    }
}
