use clap::{Parser, Subcommand};

mod api;
mod catalog;
mod config;
mod mcp;

#[derive(Parser)]
#[command(name = "korea-cli")]
#[command(about = "한국 공공데이터포털 API를 자연어로 접근하는 CLI + MCP 서버")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 자연어 질의 (서브커맨드 없이 바로 사용)
    query: Option<Vec<String>>,
}

#[derive(Subcommand)]
enum Commands {
    /// API 카탈로그에서 검색
    Search {
        /// 검색어
        query: String,
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
    Set {
        /// 키 (예: api-key)
        key: String,
        /// 값
        value: String,
    },
    /// 설정값 조회
    Get {
        /// 키
        key: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Search { query }) => {
            println!("카탈로그에서 '{query}' 검색 중...");
            // TODO: catalog::search(&query)
        }
        Some(Commands::Mcp) => {
            println!("MCP 서버 모드 시작...");
            // TODO: mcp::serve()
        }
        Some(Commands::Config { action }) => match action {
            ConfigAction::Set { key, value } => {
                println!("설정: {key} = {value}");
                // TODO: config::set(&key, &value)
            }
            ConfigAction::Get { key } => {
                println!("조회: {key}");
                // TODO: config::get(&key)
            }
        },
        Some(Commands::Update) => {
            println!("API 카탈로그 업데이트 중...");
            // TODO: catalog::update()
        }
        None => {
            if let Some(words) = cli.query {
                let query = words.join(" ");
                println!("질의: {query}");
                // TODO: 자연어 → API 검색 → 호출 → 결과 출력
            } else {
                println!("korea-cli: 한국 공공데이터포털 API를 자연어로 접근합니다.");
                println!("사용법: korea-cli \"서울 미세먼지\"");
                println!("        korea-cli search \"대기질\"");
                println!("        korea-cli mcp");
                println!("        korea-cli --help");
            }
        }
    }

    Ok(())
}
