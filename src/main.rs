use clap::{Parser, Subcommand};

mod core;
mod config;
mod mcp;
mod cli;

#[derive(Parser)]
#[command(name = "korea-cli")]
#[command(about = "한국 공공데이터포털 API를 자연어로 접근하는 CLI + MCP 서버")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// API 카탈로그에서 검색
    Search {
        /// 검색어
        query: String,
        /// 카테고리 필터
        #[arg(long)]
        category: Option<String>,
        /// 결과 수 제한
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// API 상세 스펙 조회
    Spec {
        /// API 서비스 ID (list_id)
        list_id: String,
    },
    /// API 호출
    Call {
        /// API 서비스 ID (list_id)
        list_id: String,
        /// 오퍼레이션 경로 (예: /status)
        operation: String,
        /// 파라미터 (key=value 형식, 반복 가능)
        #[arg(long = "param", value_parser = parse_param)]
        params: Vec<(String, String)>,
    },
    /// MCP 서버 모드로 실행 (stdio JSON-RPC)
    Mcp,
    /// 설정 관리
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// API 카탈로그 업데이트
    Update,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// 설정값 지정
    Set { key: String, value: String },
    /// 설정값 조회
    Get { key: String },
}

fn parse_param(s: &str) -> Result<(String, String), String> {
    let pos = s.find('=').ok_or_else(|| format!("invalid param: {s} (expected key=value)"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present (dev convenience, not required)
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Search { query, category, limit }) => {
            cli::search::run(&query, category.as_deref(), limit).await?;
        }
        Some(Commands::Spec { list_id }) => {
            cli::spec::run(&list_id).await?;
        }
        Some(Commands::Call { list_id, operation, params }) => {
            cli::call::run(&list_id, &operation, &params).await?;
        }
        Some(Commands::Mcp) => {
            mcp::server::run().await?;
        }
        Some(Commands::Config { action }) => match action {
            ConfigAction::Set { key, value } => {
                let mut cfg = config::AppConfig::load()?;
                cfg.set(&key, &value)?;
                let response = serde_json::json!({ "success": true, "key": key, "value": value });
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            ConfigAction::Get { key } => {
                let cfg = config::AppConfig::load()?;
                match cfg.get(&key) {
                    Ok(value) => {
                        let response = serde_json::json!({ "success": true, "key": key, "value": value });
                        println!("{}", serde_json::to_string_pretty(&response)?);
                    }
                    Err(e) => {
                        let response = serde_json::json!({
                            "success": false, "error": "CONFIG_NOT_SET",
                            "message": e.to_string(),
                            "action": "korea-cli config set api-key YOUR_KEY"
                        });
                        println!("{}", serde_json::to_string_pretty(&response)?);
                    }
                }
            }
        },
        Some(Commands::Update) => {
            cli::update::run().await?;
        }
        None => {
            eprintln!("korea-cli: 한국 공공데이터포털 API를 자연어로 접근합니다.");
            eprintln!("사용법: korea-cli search \"사업자등록\"");
            eprintln!("        korea-cli --help");
        }
    }

    Ok(())
}
