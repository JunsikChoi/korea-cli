//! API 전수조사: data.go.kr 12K+ API의 openapi.do 페이지를 스캔하여
//! Swagger/HTML/외부URL 등 모든 패턴을 분류한다.
//!
//! Usage: cargo run --bin survey -- --api-key YOUR_KEY [--output data/survey.json] [--concurrency 5]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};

use korea_cli::core::catalog::fetch_all_services;

/// 개별 API 조사 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiSurveyEntry {
    list_id: String,
    title: String,
    endpoint_url: String,

    // --- 페이지 접근성 ---
    /// openapi.do 페이지 HTTP 상태 코드 (200, 404, 500 등)
    http_status: Option<u16>,
    /// 페이지 로드 실패 시 에러 메시지
    fetch_error: Option<String>,

    // --- Swagger 탐지 ---
    /// var swaggerJson = `{...}` 존재 여부
    has_swagger_json: bool,
    /// var swaggerUrl = '...' 존재 여부
    has_swagger_url: bool,
    /// swaggerUrl의 실제 값 (있는 경우)
    swagger_url_value: Option<String>,
    /// Swagger 파싱 성공 시 operation 수 (None = 파싱 안 함/실패)
    swagger_ops_count: Option<usize>,
    /// Swagger 파싱 에러 메시지
    swagger_error: Option<String>,

    // --- HTML 스펙 탐지 ---
    /// publicDataDetailPk 존재 여부
    has_html_pk: bool,
    /// publicDataDetailPk 값
    html_pk_value: Option<String>,
    /// <select id="open_api_detail_select"> 안의 operation 옵션 수
    html_ops_count: usize,

    // --- Endpoint URL 분석 ---
    /// endpoint URL에서 추출한 도메인 패턴
    endpoint_domain: String,
    /// 현재 classify() 로직으로 분류한 결과
    current_classification: String,

    // --- 예상치 못한 패턴 탐지 ---
    /// 페이지에서 발견된 비정상 신호들
    anomalies: Vec<String>,
}

/// 전수조사 전체 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SurveyReport {
    /// 조사 시작 시각
    started_at: String,
    /// 조사 완료 시각
    completed_at: String,
    /// 전체 API 수
    total_apis: usize,
    /// 조사 성공 수
    surveyed_count: usize,
    /// 조사 실패 수 (페이지 접근 불가)
    failed_count: usize,
    /// 개별 결과
    entries: Vec<ApiSurveyEntry>,
}

struct PageAnalysis {
    has_swagger_json: bool,
    has_swagger_url: bool,
    swagger_url_value: Option<String>,
    swagger_ops_count: Option<usize>,
    swagger_error: Option<String>,
    has_html_pk: bool,
    html_pk_value: Option<String>,
    html_ops_count: usize,
    anomalies: Vec<String>,
}

/// openapi.do HTML 페이지를 분석하여 모든 신호를 추출한다.
/// HTTP 호출 없이 순수하게 HTML 문자열만 받아 분석.
fn analyze_page(html: &str) -> PageAnalysis {
    // Swagger 탐지
    let swagger_json = korea_cli::core::swagger::extract_swagger_json(html);
    let swagger_url = korea_cli::core::swagger::extract_swagger_url(html);

    let (swagger_ops_count, swagger_error) = if let Some(ref json) = swagger_json {
        match korea_cli::core::swagger::parse_swagger("_survey", json) {
            Ok(spec) => (Some(spec.operations.len()), None),
            Err(e) => (None, Some(e.to_string())),
        }
    } else {
        (None, None)
    };

    // HTML 스펙 탐지
    let html_result = korea_cli::core::html_parser::parse_openapi_page(html);
    let (has_html_pk, html_pk_value, html_ops_count) = match html_result {
        Ok(info) => (true, Some(info.public_data_detail_pk), info.operations.len()),
        Err(_) => (false, None, 0),
    };

    // 예상치 못한 패턴 탐지
    let anomalies = detect_anomalies(html, &swagger_json, &swagger_url, has_html_pk);

    PageAnalysis {
        has_swagger_json: swagger_json.is_some(),
        has_swagger_url: swagger_url.is_some(),
        swagger_url_value: swagger_url,
        swagger_ops_count,
        swagger_error,
        has_html_pk,
        html_pk_value,
        html_ops_count,
        anomalies,
    }
}

/// 예상치 못한 패턴 탐지 — 우리가 아직 모르는 케이스를 찾아낸다.
fn detect_anomalies(
    html: &str,
    swagger_json: &Option<serde_json::Value>,
    swagger_url: &Option<String>,
    has_html_pk: bool,
) -> Vec<String> {
    let mut anomalies = Vec::new();

    // 1. Swagger JSON이 있는데 operation이 0개 (skeleton)
    if let Some(json) = swagger_json {
        if let Some(paths) = json.get("paths").and_then(|p| p.as_object()) {
            if paths.is_empty() {
                anomalies.push("swagger_empty_paths".into());
            }
        } else {
            anomalies.push("swagger_no_paths_field".into());
        }

        // swagger version이 2.0이 아닌 경우
        let version = json.get("swagger").and_then(|v| v.as_str()).unwrap_or("");
        if !version.is_empty() && version != "2.0" {
            anomalies.push(format!("swagger_version_{version}"));
        }

        // openapi 3.x 형태인 경우
        if json.get("openapi").is_some() {
            let oa_ver = json["openapi"].as_str().unwrap_or("unknown");
            anomalies.push(format!("openapi3_{oa_ver}"));
        }
    }

    // 2. Swagger도 없고 HTML pk도 없는 페이지
    if swagger_json.is_none() && swagger_url.is_none() && !has_html_pk {
        anomalies.push("no_swagger_no_html_pk".into());
    }

    // 3. iframe 존재 (외부 문서 임베딩)
    if html.contains("<iframe") || html.contains("<IFRAME") {
        anomalies.push("has_iframe".into());
    }

    // 4. "서비스 종료" 또는 "폐기" 문구
    if html.contains("서비스 종료") || html.contains("서비스종료") {
        anomalies.push("service_terminated".into());
    }
    if html.contains("폐기") {
        anomalies.push("deprecated_notice".into());
    }

    // 5. 리다이렉트 안내
    if html.contains("meta http-equiv=\"refresh\"") || html.contains("meta http-equiv='refresh'") {
        anomalies.push("meta_redirect".into());
    }
    if html.contains("location.href") || html.contains("location.replace") {
        if !html.contains("swaggerUrl") {
            anomalies.push("js_redirect".into());
        }
    }

    // 6. 로그인 필요
    if html.contains("로그인이 필요") || html.contains("login") && html.contains("session") {
        anomalies.push("login_required".into());
    }

    // 7. SOAP/WSDL
    if html.contains("wsdl") || html.contains("WSDL") || html.contains("soap:") {
        anomalies.push("soap_wsdl_detected".into());
    }

    // 8. GraphQL
    if html.contains("graphql") || html.contains("GraphQL") {
        anomalies.push("graphql_detected".into());
    }

    // 9. swaggerJson 변수가 있지만 파싱 안 됨
    if swagger_json.is_none()
        && (html.contains("swaggerJson") || html.contains("swagger_json"))
    {
        anomalies.push("swagger_json_var_but_unparsed".into());
    }

    // 10. 페이지가 비정상적으로 짧음
    if html.len() < 1000 {
        anomalies.push(format!("very_short_page_{}_bytes", html.len()));
    }

    anomalies
}

/// endpoint URL에서 도메인 패턴을 추출한다.
fn extract_domain(url: &str) -> String {
    if url.is_empty() {
        return "(empty)".into();
    }
    url.split("//")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

async fn survey_single_api(
    client: &reqwest::Client,
    list_id: &str,
    title: &str,
    endpoint_url: &str,
) -> ApiSurveyEntry {
    let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");

    let (http_status, fetch_error, analysis) = match client.get(&page_url).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            match resp.text().await {
                Ok(html) => (Some(status), None, Some(analyze_page(&html))),
                Err(e) => (Some(status), Some(format!("body read error: {e}")), None),
            }
        }
        Err(e) => (None, Some(format!("request error: {e}")), None),
    };

    let endpoint_domain = extract_domain(endpoint_url);

    let has_spec = analysis
        .as_ref()
        .map(|a| a.swagger_ops_count.unwrap_or(0) > 0)
        .unwrap_or(false);
    let is_skeleton = analysis
        .as_ref()
        .map(|a| a.has_swagger_json && a.swagger_ops_count == Some(0))
        .unwrap_or(false);
    let current_classification = format!(
        "{:?}",
        korea_cli::core::types::SpecStatus::classify(has_spec, is_skeleton, endpoint_url)
    );

    match analysis {
        Some(a) => ApiSurveyEntry {
            list_id: list_id.to_string(),
            title: title.to_string(),
            endpoint_url: endpoint_url.to_string(),
            http_status,
            fetch_error,
            has_swagger_json: a.has_swagger_json,
            has_swagger_url: a.has_swagger_url,
            swagger_url_value: a.swagger_url_value,
            swagger_ops_count: a.swagger_ops_count,
            swagger_error: a.swagger_error,
            has_html_pk: a.has_html_pk,
            html_pk_value: a.html_pk_value,
            html_ops_count: a.html_ops_count,
            endpoint_domain,
            current_classification,
            anomalies: a.anomalies,
        },
        None => ApiSurveyEntry {
            list_id: list_id.to_string(),
            title: title.to_string(),
            endpoint_url: endpoint_url.to_string(),
            http_status,
            fetch_error,
            has_swagger_json: false,
            has_swagger_url: false,
            swagger_url_value: None,
            swagger_ops_count: None,
            swagger_error: None,
            has_html_pk: false,
            html_pk_value: None,
            html_ops_count: 0,
            endpoint_domain,
            current_classification,
            anomalies: vec!["page_fetch_failed".into()],
        },
    }
}

/// 기존 조사 결과 파일에서 이미 조사된 list_id 목록을 로드한다.
fn load_existing_survey(path: &str) -> HashMap<String, ApiSurveyEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let report: SurveyReport = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };
    report
        .entries
        .into_iter()
        .map(|e| (e.list_id.clone(), e))
        .collect()
}

#[derive(Clone)]
struct SurveyConfig {
    api_key: String,
    output: String,
    concurrency: usize,
    delay_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = parse_args()?;
    let start = Instant::now();
    let started_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    // Step 1: 카탈로그 수집
    eprintln!("=== Step 1/3: 카탈로그 수집 ===");
    let services = fetch_all_services(&config.api_key).await?;
    eprintln!("  {} 서비스 수집 완료", services.len());

    // Resume: 기존 결과 로드
    let args: Vec<String> = std::env::args().collect();
    let resume = args.iter().any(|a| a == "--resume");
    let mut existing = if resume {
        let loaded = load_existing_survey(&config.output);
        if !loaded.is_empty() {
            eprintln!("  기존 조사 결과 {} 건 로드 (이어하기)", loaded.len());
        }
        loaded
    } else {
        HashMap::new()
    };

    // Step 2: 전수조사
    let services_to_survey: Vec<_> = services
        .iter()
        .filter(|svc| !existing.contains_key(&svc.list_id))
        .collect();

    eprintln!(
        "\n=== Step 2/3: openapi.do 전수조사 (동시 {}건) ===",
        config.concurrency
    );
    eprintln!(
        "  신규 조사 대상: {} / 기존: {}",
        services_to_survey.len(),
        existing.len()
    );

    let client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-survey/0.1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .context("HTTP 클라이언트 생성 실패")?,
    );

    let done_count = Arc::new(AtomicUsize::new(0));
    let total = services_to_survey.len();

    let new_entries: Vec<ApiSurveyEntry> = stream::iter(services_to_survey.into_iter())
        .map(|svc| {
            let client = client.clone();
            let list_id = svc.list_id.clone();
            let title = svc.title.clone();
            let endpoint_url = svc.endpoint_url.clone();
            let delay_ms = config.delay_ms;
            let done = done_count.clone();

            async move {
                let entry = survey_single_api(&client, &list_id, &title, &endpoint_url).await;

                let count = done.fetch_add(1, Ordering::Relaxed) + 1;
                if count % 500 == 0 {
                    eprintln!("  진행: {count}/{total}");
                }

                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                entry
            }
        })
        .buffer_unordered(config.concurrency)
        .collect()
        .await;

    let completed_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    // 결과 병합
    for entry in new_entries {
        existing.insert(entry.list_id.clone(), entry);
    }
    let all_entries: Vec<ApiSurveyEntry> = existing.into_values().collect();

    let surveyed_count = all_entries.iter().filter(|e| e.fetch_error.is_none()).count();
    let failed_count = all_entries.len() - surveyed_count;

    let report = SurveyReport {
        started_at,
        completed_at,
        total_apis: all_entries.len(),
        surveyed_count,
        failed_count,
        entries: all_entries,
    };

    // Step 3: 보고서 출력
    eprintln!("\n=== Step 3/3: 보고서 생성 ===");
    print_summary(&report);

    // JSON 저장
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::create_dir_all(
        std::path::Path::new(&config.output)
            .parent()
            .unwrap_or(std::path::Path::new(".")),
    )?;
    std::fs::write(&config.output, &json)?;

    let elapsed = start.elapsed();
    eprintln!("\n=== 완료 ===");
    eprintln!("  소요: {:.1}분", elapsed.as_secs_f64() / 60.0);
    eprintln!("  결과: {}", config.output);

    Ok(())
}

fn print_summary(report: &SurveyReport) {
    eprintln!("\n--- 전수조사 요약 ---");
    eprintln!("전체: {}", report.total_apis);
    eprintln!(
        "조사 성공: {} / 실패: {}",
        report.surveyed_count, report.failed_count
    );

    // Swagger 분포
    let swagger_full = report
        .entries
        .iter()
        .filter(|e| e.swagger_ops_count.unwrap_or(0) > 0)
        .count();
    let swagger_skeleton = report
        .entries
        .iter()
        .filter(|e| e.has_swagger_json && e.swagger_ops_count == Some(0))
        .count();
    let swagger_url_only = report
        .entries
        .iter()
        .filter(|e| !e.has_swagger_json && e.has_swagger_url)
        .count();
    let no_swagger = report
        .entries
        .iter()
        .filter(|e| !e.has_swagger_json && !e.has_swagger_url)
        .count();

    eprintln!("\n[Swagger 분포]");
    eprintln!("  inline JSON (ops > 0): {swagger_full}");
    eprintln!("  inline JSON (ops = 0, skeleton): {swagger_skeleton}");
    eprintln!("  swaggerUrl만: {swagger_url_only}");
    eprintln!("  Swagger 없음: {no_swagger}");

    // HTML 스펙 분포
    let html_available = report
        .entries
        .iter()
        .filter(|e| e.has_html_pk && e.html_ops_count > 0)
        .count();
    let html_pk_no_ops = report
        .entries
        .iter()
        .filter(|e| e.has_html_pk && e.html_ops_count == 0)
        .count();
    let no_html = report.entries.iter().filter(|e| !e.has_html_pk).count();

    eprintln!("\n[HTML 스펙 분포]");
    eprintln!("  pk + operations: {html_available}");
    eprintln!("  pk만 (operations 없음): {html_pk_no_ops}");
    eprintln!("  pk 없음: {no_html}");

    // 교차 분석
    let html_fallback_candidates = report
        .entries
        .iter()
        .filter(|e| e.swagger_ops_count.unwrap_or(0) == 0 && e.has_html_pk && e.html_ops_count > 0)
        .count();
    eprintln!("\n[교차 분석]");
    eprintln!("  Swagger 없음 + HTML 폴백 가능: {html_fallback_candidates}");

    // Endpoint 도메인 분포
    let mut domain_counts: HashMap<String, usize> = HashMap::new();
    for entry in &report.entries {
        *domain_counts.entry(entry.endpoint_domain.clone()).or_default() += 1;
    }
    let mut domains: Vec<_> = domain_counts.into_iter().collect();
    domains.sort_by(|a, b| b.1.cmp(&a.1));

    eprintln!("\n[Endpoint 도메인 Top 20]");
    for (domain, count) in domains.iter().take(20) {
        eprintln!("  {count:>5}  {domain}");
    }

    // SpecStatus 분류 분포
    let mut status_counts: HashMap<String, usize> = HashMap::new();
    for entry in &report.entries {
        *status_counts.entry(entry.current_classification.clone()).or_default() += 1;
    }
    eprintln!("\n[현재 SpecStatus 분류]");
    for (status, count) in &status_counts {
        eprintln!("  {count:>5}  {status}");
    }

    // Anomaly 분포
    let mut anomaly_counts: HashMap<String, usize> = HashMap::new();
    for entry in &report.entries {
        for anomaly in &entry.anomalies {
            *anomaly_counts.entry(anomaly.clone()).or_default() += 1;
        }
    }
    if !anomaly_counts.is_empty() {
        let mut anomalies: Vec<_> = anomaly_counts.into_iter().collect();
        anomalies.sort_by(|a, b| b.1.cmp(&a.1));
        eprintln!("\n[Anomaly 분포]");
        for (anomaly, count) in &anomalies {
            eprintln!("  {count:>5}  {anomaly}");
        }
    }
}

fn parse_args() -> Result<SurveyConfig> {
    let args: Vec<String> = std::env::args().collect();

    let api_key = get_arg(&args, "--api-key")
        .or_else(|| std::env::var("DATA_GO_KR_API_KEY").ok())
        .ok_or_else(|| anyhow::anyhow!("--api-key 또는 DATA_GO_KR_API_KEY 환경변수 필요"))?;

    let output = get_arg(&args, "--output").unwrap_or_else(|| "data/survey.json".into());
    let concurrency: usize = get_arg(&args, "--concurrency")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let delay_ms: u64 = get_arg(&args, "--delay")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    Ok(SurveyConfig {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://apis.data.go.kr/1360000/Weather"),
            "apis.data.go.kr"
        );
        assert_eq!(
            extract_domain("https://api.odcloud.kr/api/test"),
            "api.odcloud.kr"
        );
        assert_eq!(
            extract_domain("https://apihub.kma.go.kr/api"),
            "apihub.kma.go.kr"
        );
        assert_eq!(extract_domain(""), "(empty)");
        assert_eq!(
            extract_domain("http://example.com:8080/path"),
            "example.com:8080"
        );
    }

    #[test]
    fn test_survey_entry_json_roundtrip() {
        let entry = ApiSurveyEntry {
            list_id: "15084084".into(),
            title: "Test API".into(),
            endpoint_url: "https://apis.data.go.kr/test".into(),
            http_status: Some(200),
            fetch_error: None,
            has_swagger_json: true,
            has_swagger_url: false,
            swagger_url_value: None,
            swagger_ops_count: Some(3),
            swagger_error: None,
            has_html_pk: true,
            html_pk_value: Some("uddi:12345".into()),
            html_ops_count: 2,
            endpoint_domain: "apis.data.go.kr".into(),
            current_classification: "Available".into(),
            anomalies: vec![],
        };

        let json = serde_json::to_string(&entry).unwrap();
        let decoded: ApiSurveyEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.list_id, "15084084");
        assert_eq!(decoded.swagger_ops_count, Some(3));
        assert_eq!(decoded.html_ops_count, 2);
    }

    #[test]
    fn test_analyze_page_swagger_available() {
        // 1000바이트 이상이어야 very_short_page anomaly가 안 뜸
        let padding = " ".repeat(800);
        let html = format!(
            r#"
            <html><body>
            <input type="hidden" name="publicDataDetailPk" value="uddi:abc123">
            <select id="open_api_detail_select">
                <option value="1">op1</option>
            </select>
            <script>
            var swaggerJson = `{{"swagger":"2.0","host":"api.test.kr","basePath":"/","schemes":["https"],"paths":{{"/items":{{"get":{{"summary":"test","parameters":[],"responses":{{"200":{{}}}}}}}}}}}}`
            </script>
            <!-- {padding} -->
            </body></html>
        "#
        );

        let analysis = analyze_page(&html);
        assert!(analysis.has_swagger_json);
        assert!(!analysis.has_swagger_url);
        assert_eq!(analysis.swagger_ops_count, Some(1));
        assert!(analysis.has_html_pk);
        assert_eq!(analysis.html_pk_value.as_deref(), Some("uddi:abc123"));
        assert_eq!(analysis.html_ops_count, 1);
        assert!(analysis.anomalies.is_empty(), "unexpected anomalies: {:?}", analysis.anomalies);
    }

    #[test]
    fn test_analyze_page_html_only() {
        let html = r#"
            <html><body>
            <input type="hidden" name="publicDataDetailPk" value="uddi:xyz789">
            <select id="open_api_detail_select">
                <option value="101">getWeather</option>
                <option value="102">getForecast</option>
            </select>
            <script>
            var someOtherVar = 'hello';
            </script>
            </body></html>
        "#;

        let analysis = analyze_page(html);
        assert!(!analysis.has_swagger_json);
        assert!(!analysis.has_swagger_url);
        assert_eq!(analysis.swagger_ops_count, None);
        assert!(analysis.has_html_pk);
        assert_eq!(analysis.html_ops_count, 2);
        assert!(!analysis.anomalies.contains(&"swagger_json_var_but_unparsed".to_string()));
    }

    #[test]
    fn test_analyze_page_swagger_skeleton() {
        let html = r#"
            <html><body>
            <script>
            var swaggerJson = `{"swagger":"2.0","host":"api.test.kr","basePath":"/","schemes":["https"],"paths":{}}`
            </script>
            </body></html>
        "#;

        let analysis = analyze_page(html);
        assert!(analysis.has_swagger_json);
        assert_eq!(analysis.swagger_ops_count, Some(0));
        assert!(analysis.anomalies.contains(&"swagger_empty_paths".to_string()));
    }

    #[test]
    fn test_detect_anomalies_terminated_service() {
        let html = "<html><body>이 API는 서비스 종료되었습니다.</body></html>";
        let anomalies = detect_anomalies(html, &None, &None, false);
        assert!(anomalies.contains(&"service_terminated".to_string()));
        assert!(anomalies.contains(&"no_swagger_no_html_pk".to_string()));
    }

    #[test]
    fn test_detect_anomalies_soap() {
        let html = "<html><body>WSDL endpoint: http://example.kr/service?wsdl</body></html>";
        let anomalies = detect_anomalies(html, &None, &None, false);
        assert!(anomalies.contains(&"soap_wsdl_detected".to_string()));
    }

    #[test]
    fn test_detect_anomalies_unparsed_swagger_var() {
        let html =
            r#"<html><body><script>var swaggerJson = JSON.parse(data);</script></body></html>"#;
        let anomalies = detect_anomalies(html, &None, &None, false);
        assert!(anomalies.contains(&"swagger_json_var_but_unparsed".to_string()));
    }

    #[test]
    fn test_survey_report_json_roundtrip() {
        let report = SurveyReport {
            started_at: "2026-04-02T00:00:00".into(),
            completed_at: "2026-04-02T01:00:00".into(),
            total_apis: 12000,
            surveyed_count: 11900,
            failed_count: 100,
            entries: vec![],
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let decoded: SurveyReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_apis, 12000);
        assert_eq!(decoded.surveyed_count, 11900);
    }

    #[test]
    fn test_load_existing_survey_empty() {
        let result = load_existing_survey("/nonexistent/path.json");
        assert!(result.is_empty());
    }

    #[test]
    fn test_load_existing_survey_valid() {
        let report = SurveyReport {
            started_at: "2026-04-02T00:00:00".into(),
            completed_at: "2026-04-02T01:00:00".into(),
            total_apis: 1,
            surveyed_count: 1,
            failed_count: 0,
            entries: vec![ApiSurveyEntry {
                list_id: "12345".into(),
                title: "Test".into(),
                endpoint_url: "".into(),
                http_status: Some(200),
                fetch_error: None,
                has_swagger_json: true,
                has_swagger_url: false,
                swagger_url_value: None,
                swagger_ops_count: Some(1),
                swagger_error: None,
                has_html_pk: false,
                html_pk_value: None,
                html_ops_count: 0,
                endpoint_domain: "".into(),
                current_classification: "Available".into(),
                anomalies: vec![],
            }],
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("survey.json");
        let json = serde_json::to_string(&report).unwrap();
        std::fs::write(&path, &json).unwrap();

        let loaded = load_existing_survey(path.to_str().unwrap());
        assert_eq!(loaded.len(), 1);
        assert!(loaded.contains_key("12345"));
    }
}
