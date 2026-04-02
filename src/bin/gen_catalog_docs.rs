//! 번들에서 API 카탈로그 markdown 문서를 생성한다.
//!
//! Usage: cargo run --bin gen-catalog-docs -- --bundle data/bundle.zstd --output docs/api-catalog

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(about = "번들에서 API 카탈로그 markdown 문서를 생성한다")]
struct Args {
    /// 번들 파일 경로
    #[arg(long, default_value = "data/bundle.zstd")]
    bundle: PathBuf,

    /// 출력 디렉토리
    #[arg(long, default_value = "docs/api-catalog")]
    output: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    eprintln!("번들: {:?}", args.bundle);
    eprintln!("출력: {:?}", args.output);
    Ok(())
}
