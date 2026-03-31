//! Bundle builder: collects all API catalog + Swagger specs from data.go.kr.
//!
//! Usage: cargo run --bin build-bundle -- --api-key YOUR_KEY [--output data/bundle.zstd] [--concurrency 5]
//!
//! Estimated time: ~15-20 minutes for 12K APIs with concurrency=5.

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use korea_cli::core::bundle;
use korea_cli::core::catalog::fetch_all_services;
use korea_cli::core::swagger::{extract_swagger_json, extract_swagger_url, parse_swagger};
use korea_cli::core::types::*;

#[derive(Clone)]
struct BuildConfig {
    api_key: String,
    output: String,
    concurrency: usize,
    delay_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = parse_args()?;
    let start = Instant::now();

    // Step 1: Fetch catalog
    eprintln!("=== Step 1/4: 카탈로그 수집 ===");
    let services = fetch_all_services(&config.api_key).await?;
    eprintln!("  {} 서비스 수집 완료", services.len());

    // Step 2: Collect Swagger specs
    eprintln!(
        "\n=== Step 2/4: Swagger spec 수집 (동시 {}건) ===",
        config.concurrency
    );
    let specs = collect_specs(&services, &config).await;
    eprintln!(
        "  {}/{} spec 수집 완료 ({:.1}%)",
        specs.len(),
        services.len(),
        (specs.len() as f64 / services.len() as f64) * 100.0
    );

    // Step 3: Build lightweight catalog
    eprintln!("\n=== Step 3/4: 번들 구성 ===");
    let catalog: Vec<CatalogEntry> = services
        .iter()
        .map(|svc| CatalogEntry {
            list_id: svc.list_id.clone(),
            title: svc.title.clone(),
            description: svc.description.clone(),
            keywords: svc.keywords.clone(),
            org_name: svc.org_name.clone(),
            category: svc.category.clone(),
            request_count: svc.request_count,
        })
        .collect();

    let checksum = format!(
        "{:x}",
        md5_hash(&format!("{}-{}", catalog.len(), specs.len()))
    );
    let bundle_data = Bundle {
        metadata: BundleMetadata {
            version: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            api_count: catalog.len(),
            spec_count: specs.len(),
            checksum,
        },
        catalog,
        specs,
    };

    // Step 4: Serialize + compress
    eprintln!("\n=== Step 4/4: 직렬화 + 압축 ===");
    let compressed = bundle::serialize_and_compress(&bundle_data, 3)?;

    std::fs::create_dir_all(
        std::path::Path::new(&config.output)
            .parent()
            .unwrap_or(std::path::Path::new(".")),
    )?;
    std::fs::write(&config.output, &compressed)?;

    let elapsed = start.elapsed();
    eprintln!("\n=== 완료 ===");
    eprintln!("  버전: {}", bundle_data.metadata.version);
    eprintln!(
        "  API: {} / Spec: {}",
        bundle_data.metadata.api_count, bundle_data.metadata.spec_count
    );
    eprintln!("  크기: {:.2} MB", compressed.len() as f64 / 1_048_576.0);
    eprintln!("  경로: {}", config.output);
    eprintln!("  소요: {:.1}분", elapsed.as_secs_f64() / 60.0);

    Ok(())
}

async fn collect_specs(services: &[ApiService], config: &BuildConfig) -> HashMap<String, ApiSpec> {
    let client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-builder/0.1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to build HTTP client"),
    );

    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));
    let total = services.len();

    let results: Vec<(String, Option<ApiSpec>)> = stream::iter(services.iter())
        .map(|svc| {
            let client = client.clone();
            let list_id = svc.list_id.clone();
            let delay_ms = config.delay_ms;
            let sc = success_count.clone();
            let fc = fail_count.clone();

            async move {
                let result = fetch_single_spec(&client, &list_id).await;

                match &result {
                    Ok(_) => {
                        sc.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        fc.fetch_add(1, Ordering::Relaxed);
                        eprintln!("  SKIP {list_id}: {e}");
                    }
                }

                let done = sc.load(Ordering::Relaxed) + fc.load(Ordering::Relaxed);
                if done % 500 == 0 {
                    eprintln!(
                        "  진행: {done}/{total} ({} OK, {} FAIL)",
                        sc.load(Ordering::Relaxed),
                        fc.load(Ordering::Relaxed),
                    );
                }

                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                (list_id, result.ok())
            }
        })
        .buffer_unordered(config.concurrency)
        .collect()
        .await;

    results
        .into_iter()
        .filter_map(|(id, spec)| spec.map(|s| (id, s)))
        .collect()
}

async fn fetch_single_spec(client: &reqwest::Client, list_id: &str) -> Result<ApiSpec> {
    let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");
    let html = client
        .get(&page_url)
        .send()
        .await
        .context("페이지 요청 실패")?
        .text()
        .await
        .context("페이지 본문 읽기 실패")?;

    // Pattern 1: inline swaggerJson (99% of cases)
    if let Some(json) = extract_swagger_json(&html) {
        return parse_swagger(list_id, &json);
    }

    // Pattern 2: external swaggerUrl (1% of cases)
    if let Some(url) = extract_swagger_url(&html) {
        let spec_json: serde_json::Value = client
            .get(&url)
            .send()
            .await
            .context("Swagger URL 요청 실패")?
            .json()
            .await
            .context("Swagger JSON 파싱 실패")?;
        return parse_swagger(list_id, &spec_json);
    }

    anyhow::bail!("swaggerJson/swaggerUrl 모두 없음")
}

fn parse_args() -> Result<BuildConfig> {
    let args: Vec<String> = std::env::args().collect();

    let api_key = get_arg(&args, "--api-key")
        .or_else(|| std::env::var("DATA_GO_KR_API_KEY").ok())
        .ok_or_else(|| anyhow::anyhow!("--api-key 또는 DATA_GO_KR_API_KEY 환경변수 필요"))?;

    let output = get_arg(&args, "--output").unwrap_or_else(|| "data/bundle.zstd".into());
    let concurrency: usize = get_arg(&args, "--concurrency")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let delay_ms: u64 = get_arg(&args, "--delay")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    Ok(BuildConfig {
        api_key,
        output,
        concurrency,
        delay_ms,
    })
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Simple hash for checksum (not cryptographic, just for version tracking).
fn md5_hash(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}
