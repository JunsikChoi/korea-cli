//! Bundle builder: collects all API catalog + Swagger specs from data.go.kr.
//!
//! Usage: cargo run --bin build-bundle -- --api-key YOUR_KEY [--output data/bundle.zstd] [--concurrency 5]
//!
//! Estimated time: ~15-20 minutes for 12K APIs with concurrency=5.

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use korea_cli::core::bundle;
use korea_cli::core::catalog::fetch_all_services;
use korea_cli::core::html_parser::{build_api_spec, parse_openapi_page, parse_operation_detail};
use korea_cli::core::swagger::{extract_swagger_json, extract_swagger_url, parse_swagger};
use korea_cli::core::types::*;

#[derive(Clone)]
struct BuildConfig {
    api_key: String,
    output: String,
    concurrency: usize,
    delay_ms: u64,
    ajax_concurrency: usize,
    ajax_delay_ms: u64,
    retry_timeout_secs: u64,
    retry_stubs: Option<String>,
    max_retry_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ErrorType {
    NetworkTimeout,
    RateLimited,
    BodyReadError,
    ParseError,
    ConnectionError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FailedOp {
    list_id: String,
    seq_no: String,
    op_name: String,
    error_type: ErrorType,
    error_message: String,
}

/// fetch_single_spec 결과 — 스펙 또는 분류 힌트
#[derive(Debug)]
enum SpecResult {
    /// 스펙 추출 성공 (is_gateway: Pattern 3 경유 여부)
    Spec {
        spec: Box<ApiSpec>,
        is_gateway: bool,
        is_partial: bool,
        failed_ops: Vec<FailedOp>,
    },
    /// 스펙 없음 — bail 이유
    Bail {
        reason: String,
        failed_ops: Vec<FailedOp>,
    },
    /// LINK API (PRDE04) — 외부 포탈 URL 포함 가능
    ExternalLink { url: Option<String> },
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = parse_args()?;

    if let Some(ref failed_ops_path) = config.retry_stubs {
        return run_retry(&config, failed_ops_path).await;
    }

    let start = Instant::now();

    // Step 1: Fetch catalog
    eprintln!("=== Step 1/4: 카탈로그 수집 ===");
    let services = fetch_all_services(&config.api_key).await?;
    eprintln!("  {} 서비스 수집 완료", services.len());

    // Step 2: Collect specs (Swagger + Gateway AJAX)
    eprintln!(
        "\n=== Step 2/4: spec 수집 (API 동시 {}건, AJAX 동시 {}건) ===",
        config.concurrency, config.ajax_concurrency
    );
    let all_results = collect_specs(&services, &config).await;

    let mut specs: HashMap<String, ApiSpec> = HashMap::new();
    let mut skeleton_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut link_api_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut external_urls: HashMap<String, String> = HashMap::new();
    let mut partial_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut all_failed_ops: Vec<FailedOp> = Vec::new();

    for (id, result) in all_results {
        match result {
            SpecResult::Spec {
                spec,
                is_partial,
                failed_ops,
                ..
            } => {
                if is_partial {
                    partial_ids.insert(id.clone());
                    all_failed_ops.extend(failed_ops);
                }
                if spec.operations.is_empty() {
                    if !is_partial {
                        skeleton_ids.insert(id);
                    }
                } else {
                    specs.insert(id, *spec);
                }
            }
            SpecResult::ExternalLink { url } => {
                link_api_ids.insert(id.clone());
                if let Some(u) = url {
                    external_urls.insert(id, u);
                }
            }
            SpecResult::Bail { failed_ops, .. } => {
                all_failed_ops.extend(failed_ops);
            }
        }
    }
    eprintln!(
        "  {}/{} spec ({:.1}%), skeleton {}, link_api {}",
        specs.len(),
        services.len(),
        (specs.len() as f64 / services.len() as f64) * 100.0,
        skeleton_ids.len(),
        link_api_ids.len(),
    );
    if !link_api_ids.is_empty() {
        eprintln!(
            "  external_url: {}/{} LINK API ({:.0}%)",
            external_urls.len(),
            link_api_ids.len(),
            (external_urls.len() as f64 / link_api_ids.len() as f64) * 100.0,
        );
    }
    if !partial_ids.is_empty() {
        eprintln!(
            "  partial: {} APIs ({} failed ops)",
            partial_ids.len(),
            all_failed_ops.len()
        );
    }

    // Step 3: ClassificationHints로 classify
    eprintln!("\n=== Step 3/4: 번들 구성 ===");
    let catalog: Vec<CatalogEntry> = services
        .iter()
        .map(|svc| {
            let effective_url = external_urls
                .get(&svc.list_id)
                .cloned()
                .unwrap_or_else(|| svc.endpoint_url.clone());

            let spec_status = SpecStatus::classify(&ClassificationHints {
                has_spec: specs.contains_key(&svc.list_id),
                is_skeleton: skeleton_ids.contains(&svc.list_id),
                endpoint_url: &effective_url,
                is_link_api: link_api_ids.contains(&svc.list_id),
                is_partial: partial_ids.contains(&svc.list_id),
            });
            CatalogEntry {
                list_id: svc.list_id.clone(),
                title: svc.title.clone(),
                description: svc.description.clone(),
                keywords: svc.keywords.clone(),
                org_name: svc.org_name.clone(),
                category: svc.category.clone(),
                request_count: svc.request_count,
                endpoint_url: effective_url,
                spec_status,
            }
        })
        .collect();

    // Log spec_status distribution
    let mut status_counts: HashMap<String, usize> = HashMap::new();
    for entry in &catalog {
        *status_counts
            .entry(format!("{:?}", entry.spec_status))
            .or_default() += 1;
    }
    eprintln!("  spec_status 분포: {:?}", status_counts);

    let checksum = format!(
        "{:x}",
        md5_hash(&format!("{}-{}", catalog.len(), specs.len()))
    );
    let bundle_data = Bundle {
        metadata: BundleMetadata {
            version: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            schema_version: CURRENT_SCHEMA_VERSION,
            api_count: catalog.len(),
            spec_count: specs.len(),
            checksum,
        },
        catalog,
        specs,
    };

    // failed_ops.json 출력
    if !all_failed_ops.is_empty() {
        let failed_ops_path = std::path::Path::new(&config.output)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("failed_ops.json");
        let failed_json = serde_json::to_string_pretty(&all_failed_ops)?;
        std::fs::write(&failed_ops_path, &failed_json)?;
        eprintln!(
            "  failed_ops: {} → {}",
            all_failed_ops.len(),
            failed_ops_path.display()
        );
    }

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

async fn collect_specs(services: &[ApiService], config: &BuildConfig) -> Vec<(String, SpecResult)> {
    let client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-builder/0.1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to build HTTP client"),
    );

    let ajax_semaphore = Arc::new(tokio::sync::Semaphore::new(config.ajax_concurrency));
    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));
    let gateway_count = Arc::new(AtomicUsize::new(0));
    let link_count = Arc::new(AtomicUsize::new(0));
    let total = services.len();

    let results: Vec<(String, SpecResult)> = stream::iter(services.iter())
        .map(|svc| {
            let client = client.clone();
            let list_id = svc.list_id.clone();
            let delay_ms = config.delay_ms;
            let ajax_delay_ms = config.ajax_delay_ms;
            let timeout_secs = config.retry_timeout_secs;
            let ajax_sem = ajax_semaphore.clone();
            let sc = success_count.clone();
            let fc = fail_count.clone();
            let gc = gateway_count.clone();
            let lc = link_count.clone();

            async move {
                let result =
                    fetch_single_spec(&client, &list_id, &ajax_sem, ajax_delay_ms, timeout_secs)
                        .await;

                match &result {
                    SpecResult::Spec { is_gateway, .. } => {
                        sc.fetch_add(1, Ordering::Relaxed);
                        if *is_gateway {
                            gc.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    SpecResult::ExternalLink { .. } => {
                        lc.fetch_add(1, Ordering::Relaxed);
                    }
                    SpecResult::Bail { ref reason, .. } => {
                        fc.fetch_add(1, Ordering::Relaxed);
                        let done = sc.load(Ordering::Relaxed)
                            + fc.load(Ordering::Relaxed)
                            + lc.load(Ordering::Relaxed);
                        if done <= 20 || done.is_multiple_of(500) {
                            eprintln!("  SKIP {list_id}: {reason}");
                        }
                    }
                }

                let done = sc.load(Ordering::Relaxed)
                    + fc.load(Ordering::Relaxed)
                    + lc.load(Ordering::Relaxed);
                if done.is_multiple_of(500) {
                    eprintln!(
                        "  진행: {done}/{total} ({} OK, {} FAIL, {} Link, {} Gateway)",
                        sc.load(Ordering::Relaxed),
                        fc.load(Ordering::Relaxed),
                        lc.load(Ordering::Relaxed),
                        gc.load(Ordering::Relaxed),
                    );
                }

                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                (list_id, result)
            }
        })
        .buffer_unordered(config.concurrency)
        .collect()
        .await;

    results
}

async fn fetch_single_spec(
    client: &reqwest::Client,
    list_id: &str,
    ajax_semaphore: &tokio::sync::Semaphore,
    ajax_delay_ms: u64,
    timeout_secs: u64,
) -> SpecResult {
    let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");
    let html = match client.get(&page_url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(text) => text,
            Err(e) => {
                return SpecResult::Bail {
                    reason: format!("페이지 본문 읽기 실패: {e}"),
                    failed_ops: vec![],
                }
            }
        },
        Err(e) => {
            return SpecResult::Bail {
                reason: format!("페이지 요청 실패: {e}"),
                failed_ops: vec![],
            }
        }
    };

    // ① 타입 판별: tyDetailCode로 LINK API 즉시 분류
    let page_info = parse_openapi_page(&html)
        .inspect_err(|e| eprintln!("  PARSE_ERR {list_id}: {e}"))
        .ok();
    if let Some(ref info) = page_info {
        if info.ty_detail_code.as_deref() == Some("PRDE04") {
            return SpecResult::ExternalLink {
                url: info.external_url.clone(),
            };
        }
    }

    // ② Pattern 1: inline swaggerJson
    if let Some(json) = extract_swagger_json(&html) {
        return match parse_swagger(list_id, &json) {
            Ok(spec) => SpecResult::Spec {
                spec: Box::new(spec),
                is_gateway: false,
                is_partial: false,
                failed_ops: vec![],
            },
            Err(e) => SpecResult::Bail {
                reason: format!("Swagger 파싱 실패: {e}"),
                failed_ops: vec![],
            },
        };
    }

    // ③ Pattern 2: external swaggerUrl
    if let Some(url) = extract_swagger_url(&html) {
        let spec_result = async {
            let resp = client.get(&url).send().await?;
            if !resp.status().is_success() {
                anyhow::bail!("HTTP {}", resp.status());
            }
            let spec_json: serde_json::Value = resp.json().await?;
            parse_swagger(list_id, &spec_json)
        }
        .await;
        return match spec_result {
            Ok(spec) => SpecResult::Spec {
                spec: Box::new(spec),
                is_gateway: false,
                is_partial: false,
                failed_ops: vec![],
            },
            Err(e) => SpecResult::Bail {
                reason: format!("Swagger URL 실패: {e}"),
                failed_ops: vec![],
            },
        };
    }

    // ④ Pattern 3: Gateway API (select 있음 → AJAX)
    if let Some(ref info) = page_info {
        if !info.operations.is_empty() {
            return fetch_gateway_spec(list_id, info, ajax_semaphore, ajax_delay_ms, timeout_secs)
                .await;
        }
    }

    // ⑤ bail — 어떤 패턴에도 매칭 안 됨
    SpecResult::Bail {
        reason: "swaggerJson/swaggerUrl/Gateway 모두 없음".into(),
        failed_ops: vec![],
    }
}

async fn fetch_gateway_spec(
    list_id: &str,
    page_info: &korea_cli::core::html_parser::PageInfo,
    ajax_semaphore: &tokio::sync::Semaphore,
    ajax_delay_ms: u64,
    timeout_secs: u64,
) -> SpecResult {
    // API별 독립 Client 생성 (쿠키 격리)
    let ajax_client: reqwest::Client = match reqwest::Client::builder()
        .user_agent("korea-cli-builder/0.1.0")
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .cookie_store(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return SpecResult::Bail {
                reason: format!("AJAX client 생성 실패: {e}"),
                failed_ops: vec![],
            }
        }
    };

    // 쿠키 획득을 위해 페이지 재요청
    let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");
    match ajax_client.get(&page_url).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                return SpecResult::Bail {
                    reason: format!("쿠키 획득 HTTP {}", resp.status()),
                    failed_ops: vec![],
                };
            }
            // 응답 본문 소비하여 연결 정리
            let _ = resp.bytes().await;
        }
        Err(e) => {
            return SpecResult::Bail {
                reason: format!("쿠키 획득 실패: {e}"),
                failed_ops: vec![],
            };
        }
    }

    let public_data_pk = match page_info.public_data_pk.as_deref() {
        Some(pk) if !pk.is_empty() => pk,
        _ => {
            eprintln!("  WARN {list_id}: publicDataPk 없음, list_id로 대체");
            list_id
        }
    };
    let detail_pk = &page_info.public_data_detail_pk;

    let mut parsed_ops = Vec::new();
    let mut failed_ops = Vec::new();
    let total_ops = page_info.operations.len();

    for op in &page_info.operations {
        // 글로벌 AJAX 동시 요청 제한
        let _permit = match ajax_semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => break,
        };

        let form = [
            ("oprtinSeqNo", op.seq_no.as_str()),
            ("publicDataDetailPk", detail_pk),
            ("publicDataPk", public_data_pk),
        ];

        let ajax_result = ajax_client
            .post("https://www.data.go.kr/tcs/dss/selectApiDetailFunction.do")
            .form(&form)
            .send()
            .await;

        // permit을 sleep 동안 보유 — burst 방지를 위한 의도적 설계
        tokio::time::sleep(std::time::Duration::from_millis(ajax_delay_ms)).await;
        drop(_permit);

        match ajax_result {
            Ok(resp) => match resp.text().await {
                Ok(html) => match parse_operation_detail(&html) {
                    Ok(detail) => parsed_ops.push(detail),
                    Err(e) => {
                        eprintln!("  PARTIAL SKIP {list_id}/{}: parse: {e}", op.seq_no);
                        failed_ops.push(FailedOp {
                            list_id: list_id.to_string(),
                            seq_no: op.seq_no.clone(),
                            op_name: op.name.clone(),
                            error_type: ErrorType::ParseError,
                            error_message: e.to_string(),
                        });
                    }
                },
                Err(e) => {
                    eprintln!("  PARTIAL SKIP {list_id}/{}: body: {e}", op.seq_no);
                    failed_ops.push(FailedOp {
                        list_id: list_id.to_string(),
                        seq_no: op.seq_no.clone(),
                        op_name: op.name.clone(),
                        error_type: ErrorType::BodyReadError,
                        error_message: e.to_string(),
                    });
                }
            },
            Err(e) => {
                let error_type = if e.is_timeout() {
                    ErrorType::NetworkTimeout
                } else if e.is_status()
                    && e.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS)
                {
                    ErrorType::RateLimited
                } else if e.is_connect() {
                    ErrorType::ConnectionError
                } else {
                    ErrorType::NetworkTimeout
                };
                eprintln!("  PARTIAL SKIP {list_id}/{}: {e}", op.seq_no);
                failed_ops.push(FailedOp {
                    list_id: list_id.to_string(),
                    seq_no: op.seq_no.clone(),
                    op_name: op.name.clone(),
                    error_type,
                    error_message: e.to_string(),
                });
            }
        }
    }

    if parsed_ops.is_empty() {
        return SpecResult::Bail {
            reason: format!("Gateway AJAX 전부 실패 (0/{total_ops} ops)"),
            failed_ops,
        };
    }

    let is_partial = !failed_ops.is_empty();
    if is_partial {
        eprintln!(
            "  PARTIAL: {}/{total_ops} operations ({list_id})",
            parsed_ops.len()
        );
    }

    match build_api_spec(list_id, &parsed_ops) {
        Some(spec) => SpecResult::Spec {
            spec: Box::new(spec),
            is_gateway: true,
            is_partial,
            failed_ops,
        },
        None => SpecResult::Bail {
            reason: format!(
                "Gateway build_api_spec 실패 ({}/{total_ops} ops)",
                parsed_ops.len()
            ),
            failed_ops,
        },
    }
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
    let ajax_concurrency: usize = get_arg(&args, "--ajax-concurrency")
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let ajax_delay_ms: u64 = get_arg(&args, "--ajax-delay")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    let retry_stubs = get_arg(&args, "--retry-stubs");
    let max_retry_time: u64 = get_arg(&args, "--max-retry-time")
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    let retry_timeout_secs: u64 = if retry_stubs.is_some() { 30 } else { 15 };

    Ok(BuildConfig {
        api_key,
        output,
        concurrency,
        delay_ms,
        ajax_concurrency,
        ajax_delay_ms,
        retry_timeout_secs,
        retry_stubs,
        max_retry_time,
    })
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

async fn run_retry(config: &BuildConfig, failed_ops_path: &str) -> Result<()> {
    let start = Instant::now();
    eprintln!("=== Retry: {} 읽기 ===", failed_ops_path);

    // 1. failed_ops.json 읽기
    let failed_json = std::fs::read_to_string(failed_ops_path)
        .with_context(|| format!("failed_ops.json 읽기 실패: {}", failed_ops_path))?;
    let failed_ops: Vec<FailedOp> = serde_json::from_str(&failed_json)?;

    // ParseError는 재시도 불가 — 제외
    let retryable: Vec<&FailedOp> = failed_ops
        .iter()
        .filter(|op| !matches!(op.error_type, ErrorType::ParseError))
        .collect();

    // 고유 list_id 추출
    let mut retry_ids: Vec<String> = retryable
        .iter()
        .map(|op| op.list_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    retry_ids.sort();

    eprintln!(
        "  {} failed ops → {} retryable → {} unique list_ids",
        failed_ops.len(),
        retryable.len(),
        retry_ids.len()
    );

    if retry_ids.is_empty() {
        eprintln!("  재시도 대상 없음 (ParseError만 존재)");
        return Ok(());
    }

    // 2. 기존 번들 로드
    let bundle_bytes = std::fs::read(&config.output)
        .with_context(|| format!("기존 번들 읽기 실패: {}", config.output))?;
    let mut bundle_data: Bundle = bundle::decompress_and_deserialize(&bundle_bytes)?;

    // 에러 타입별 최대 딜레이 결정
    let has_rate_limited = retryable
        .iter()
        .any(|op| matches!(op.error_type, ErrorType::RateLimited));
    let delays: &[u64] = if has_rate_limited {
        &[60, 120, 300]
    } else {
        &[2, 8, 30]
    };
    let max_retries = 3usize;

    // 3. list_id별 재수집
    let retry_client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-builder/0.1.0")
            .timeout(std::time::Duration::from_secs(config.retry_timeout_secs))
            .build()?,
    );
    let ajax_semaphore = Arc::new(tokio::sync::Semaphore::new(config.ajax_concurrency));
    let mut success_count = 0usize;
    let mut still_partial = 0usize;

    for (i, list_id) in retry_ids.iter().enumerate() {
        if start.elapsed().as_secs() > config.max_retry_time {
            let remaining = retry_ids.len() - i;
            eprintln!(
                "  MAX_RETRY_TIME({}s) 초과 — 남은 {} APIs skip",
                config.max_retry_time, remaining
            );
            still_partial += remaining;
            break;
        }

        eprintln!("  [{}/{}] retry: {}", i + 1, retry_ids.len(), list_id);

        let mut succeeded = false;
        for attempt in 0..max_retries {
            let result = fetch_single_spec(
                &retry_client,
                list_id,
                &ajax_semaphore,
                config.ajax_delay_ms,
                config.retry_timeout_secs,
            )
            .await;

            match result {
                SpecResult::Spec {
                    spec, is_partial, ..
                } => {
                    if let Some(existing) = bundle_data.specs.get(list_id) {
                        let merged = merge_operations(existing, &spec);
                        bundle_data.specs.insert(list_id.clone(), merged);
                    } else {
                        bundle_data.specs.insert(list_id.clone(), *spec);
                    }

                    if is_partial {
                        still_partial += 1;
                        eprintln!("    여전히 부분 성공 (attempt {})", attempt + 1);
                    } else {
                        succeeded = true;
                        success_count += 1;
                        eprintln!("    완전 성공!");
                    }
                    break;
                }
                SpecResult::Bail { ref reason, .. } => {
                    if attempt < max_retries - 1 {
                        let delay = delays.get(attempt).copied().unwrap_or(30);
                        eprintln!("    attempt {}: {} — {}s 대기", attempt + 1, reason, delay);
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    } else {
                        eprintln!("    {}회 시도 실패: {}", max_retries, reason);
                        still_partial += 1;
                    }
                }
                SpecResult::ExternalLink { .. } => {
                    eprintln!("    LINK API — skip");
                    break;
                }
            }
        }

        // list_id 간 딜레이 — rate limit 재발 방지
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // catalog에서 PartialStub → Available 승격 (완전 성공 시)
        if succeeded {
            if let Some(entry) = bundle_data
                .catalog
                .iter_mut()
                .find(|e| e.list_id == *list_id)
            {
                entry.spec_status = SpecStatus::Available;
            }
        }
    }

    // 4. 메타데이터 갱신 + 번들 재직렬화 + 저장
    bundle_data.metadata.spec_count = bundle_data.specs.len();
    bundle_data.metadata.checksum = format!(
        "{:x}",
        md5_hash(&format!(
            "{}-{}",
            bundle_data.metadata.api_count, bundle_data.metadata.spec_count
        ))
    );
    let compressed = bundle::serialize_and_compress(&bundle_data, 3)?;
    std::fs::write(&config.output, &compressed)?;

    let elapsed = start.elapsed();
    eprintln!("\n=== Retry 완료 ===");
    eprintln!("  성공: {}/{}", success_count, retry_ids.len());
    eprintln!("  여전히 partial: {}", still_partial);
    eprintln!("  소요: {:.1}초", elapsed.as_secs_f64());

    Ok(())
}

/// 기존 spec의 operation + retry 결과의 operation을 합집합
/// path + method 쌍으로 identity를 판별
fn merge_operations(existing: &ApiSpec, new_spec: &ApiSpec) -> ApiSpec {
    let mut merged = new_spec.clone();
    for existing_op in &existing.operations {
        let dominated = merged.operations.iter().any(|op| {
            op.path == existing_op.path
                && std::mem::discriminant(&op.method) == std::mem::discriminant(&existing_op.method)
        });
        if !dominated {
            merged.operations.push(existing_op.clone());
        }
    }
    merged
}

/// Simple hash for checksum (not cryptographic, just for version tracking).
fn md5_hash(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}
