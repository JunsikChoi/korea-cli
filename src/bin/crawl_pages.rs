//! openapi.do 페이지 HTML 크롤러: 12K+ API 페이지를 로컬에 저장한다.
//! 수집과 분석을 분리하여, 분석 로직 변경 시 재크롤링이 불필요하다.
//!
//! Usage: cargo run --bin crawl-pages -- --api-key YOUR_KEY [--concurrency 5] [--delay 100] [--resume]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use serde::Serialize;

use korea_cli::core::catalog::fetch_all_services;

const PAGES_DIR: &str = "data/pages";
const FAILURES_PATH: &str = "data/crawl_failures.json";

/// 크롤링 실패 엔트리
#[derive(Debug, Serialize)]
struct CrawlFailure {
    list_id: String,
    status: Option<u16>,
    error: String,
}

/// 단일 페이지 다운로드. 성공 시 바이트 수 반환.
async fn download_page(
    client: &reqwest::Client,
    list_id: &str,
    out_dir: &Path,
) -> std::result::Result<usize, CrawlFailure> {
    let url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");

    let resp = client.get(&url).send().await.map_err(|e| CrawlFailure {
        list_id: list_id.to_string(),
        status: None,
        error: format!("request error: {e}"),
    })?;

    let status = resp.status().as_u16();
    if status != 200 {
        return Err(CrawlFailure {
            list_id: list_id.to_string(),
            status: Some(status),
            error: format!(
                "{} {}",
                status,
                resp.status().canonical_reason().unwrap_or("")
            ),
        });
    }

    let body = resp.bytes().await.map_err(|e| CrawlFailure {
        list_id: list_id.to_string(),
        status: Some(status),
        error: format!("body read error: {e}"),
    })?;

    let file_path = out_dir.join(format!("{list_id}.html"));
    tokio::fs::write(&file_path, &body)
        .await
        .map_err(|e| CrawlFailure {
            list_id: list_id.to_string(),
            status: Some(status),
            error: format!("write error: {e}"),
        })?;

    Ok(body.len())
}

/// data/pages/ 디렉토리에서 이미 다운로드된 list_id 집합을 반환한다.
fn scan_existing_pages(dir: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(stem) = entry.path().file_stem() {
                set.insert(stem.to_string_lossy().to_string());
            }
        }
    }
    set
}

struct CrawlConfig {
    api_key: String,
    concurrency: usize,
    delay_ms: u64,
    resume: bool,
    out_dir: PathBuf,
}

fn parse_args() -> Result<CrawlConfig> {
    let args: Vec<String> = std::env::args().collect();

    let api_key = get_arg(&args, "--api-key")
        .or_else(|| std::env::var("DATA_GO_KR_API_KEY").ok())
        .ok_or_else(|| anyhow::anyhow!("--api-key 또는 DATA_GO_KR_API_KEY 환경변수 필요"))?;

    let concurrency: usize = get_arg(&args, "--concurrency")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let delay_ms: u64 = get_arg(&args, "--delay")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let resume = args.iter().any(|a| a == "--resume");

    let out_dir = get_arg(&args, "--out-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(PAGES_DIR));

    Ok(CrawlConfig {
        api_key,
        concurrency,
        delay_ms,
        resume,
        out_dir,
    })
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = parse_args()?;
    let start = Instant::now();

    // Step 1: 카탈로그 수집
    eprintln!("=== Step 1/3: 카탈로그 수집 ===");
    let services = fetch_all_services(&config.api_key).await?;
    let total_all = services.len();
    eprintln!("  {} 서비스 수집 완료", total_all);

    // list_id 추출
    let all_ids: Vec<String> = services.into_iter().map(|s| s.list_id).collect();

    // Step 2: Resume 처리
    std::fs::create_dir_all(&config.out_dir).context("data/pages 디렉토리 생성 실패")?;

    let skipped = if config.resume {
        let existing = scan_existing_pages(&config.out_dir);
        if !existing.is_empty() {
            eprintln!("Resume mode: {} files already exist", existing.len());
        }
        existing
    } else {
        HashSet::new()
    };

    let ids_to_crawl: Vec<String> = all_ids
        .into_iter()
        .filter(|id| !skipped.contains(id))
        .collect();

    let total = ids_to_crawl.len();
    let skipped_count = skipped.len();

    eprintln!(
        "\n=== Step 2/3: 페이지 다운로드 (동시 {}건) ===",
        config.concurrency
    );
    eprintln!("  대상: {} / 스킵: {}", total, skipped_count);

    // HTTP 클라이언트
    let client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-crawler/0.1.0")
            .timeout(Duration::from_secs(15))
            .build()
            .context("HTTP 클라이언트 생성 실패")?,
    );

    let done_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));
    let out_dir = Arc::new(config.out_dir.clone());

    // Step 3: 병렬 다운로드
    let failures: Vec<CrawlFailure> = stream::iter(ids_to_crawl.into_iter())
        .map(|list_id| {
            let client = client.clone();
            let done = done_count.clone();
            let failed = fail_count.clone();
            let dir = out_dir.clone();
            let delay_ms = config.delay_ms;

            async move {
                let result = download_page(&client, &list_id, &dir).await;

                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                let f = failed.load(Ordering::Relaxed);
                let remaining = total.saturating_sub(d);

                match &result {
                    Ok(bytes) => {
                        eprintln!(
                            "[{d}/{total}] {list_id}.html ({}KB) \u{2014} {d} done, {f} failed, {remaining} remaining",
                            bytes / 1024
                        );
                    }
                    Err(failure) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        let f = f + 1;
                        eprintln!(
                            "[{d}/{total}] FAIL {list_id} ({}) \u{2014} {d} done, {f} failed, {remaining} remaining",
                            failure.error
                        );
                    }
                }

                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }

                result.err()
            }
        })
        .buffer_unordered(config.concurrency)
        .filter_map(|opt| async { opt })
        .collect()
        .await;

    // Step 4: 실패 목록 저장
    let success_count = total - failures.len();

    if !failures.is_empty() {
        let json = serde_json::to_string_pretty(&failures)?;
        std::fs::write(FAILURES_PATH, &json)?;
    }

    // 최종 요약
    let elapsed = start.elapsed();
    let mins = elapsed.as_secs() / 60;
    let secs = elapsed.as_secs() % 60;

    eprintln!("\n=== Step 3/3: 완료 ===");
    eprintln!("Crawl complete in {mins}m {secs}s");
    eprintln!("  Total:   {total_all}");
    if skipped_count > 0 {
        eprintln!("  Skipped: {skipped_count} (resume)");
    }
    eprintln!("  Success: {success_count}");
    eprintln!("  Failed:  {}", failures.len());
    if !failures.is_empty() {
        eprintln!("  Failures saved to {FAILURES_PATH}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_existing_pages_empty() {
        let dir = tempfile::tempdir().unwrap();
        let result = scan_existing_pages(dir.path());
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_existing_pages_with_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("15001234.html"), "test").unwrap();
        std::fs::write(dir.path().join("15005678.html"), "test").unwrap();

        let result = scan_existing_pages(dir.path());
        assert_eq!(result.len(), 2);
        assert!(result.contains("15001234"));
        assert!(result.contains("15005678"));
    }

    #[test]
    fn test_scan_existing_pages_nonexistent_dir() {
        let result = scan_existing_pages(Path::new("/nonexistent/path"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_crawl_failure_json_roundtrip() {
        let failure = CrawlFailure {
            list_id: "15009999".into(),
            status: Some(404),
            error: "404 Not Found".into(),
        };

        let json = serde_json::to_string(&failure).unwrap();
        assert!(json.contains("15009999"));
        assert!(json.contains("404"));
    }
}
