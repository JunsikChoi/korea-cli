//! HTML 스펙 전수조사: data.go.kr 12K+ API의 openapi.do 페이지에서
//! pk/select/AJAX 경로를 전수조사하여 Swagger 없이도 추출 가능한 API를 식별한다.
//!
//! Usage: cargo run --bin html-survey -- --api-key YOUR_KEY [--output data/html_survey.json] [--concurrency 5] [--phase1-only]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use korea_cli::core::catalog::fetch_all_services;

// ── Data structures ──

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HtmlSurveyEntry {
    list_id: String,
    title: String,

    // 페이지 접근
    http_status: Option<u16>,
    fetch_error: Option<String>,
    page_size_bytes: usize,

    // Swagger 상태 (교차 비교용)
    has_swagger_json: bool,
    swagger_json_empty: bool,
    swagger_ops_count: Option<usize>,

    // HTML pk 탐지 (수정된 셀렉터)
    has_pk: bool,
    pk_value: Option<String>,
    pk_source: Option<String>,

    // select 옵션
    select_option_count: usize,
    select_options: Vec<SelectOption>,

    // AJAX 프로브 (첫 번째 operation만)
    ajax_attempted: bool,
    ajax_status: Option<u16>,
    ajax_error: Option<String>,
    ajax_response_bytes: Option<usize>,
    ajax_has_request_url: bool,
    ajax_has_service_url: bool,
    ajax_request_url: Option<String>,
    ajax_service_url: Option<String>,
    ajax_param_count: usize,
    ajax_param_style: Option<String>,
    ajax_response_field_count: usize,
    ajax_is_error_page: bool,

    // 분류
    page_pattern: String,
    anomalies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SelectOption {
    seq_no: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HtmlSurveyReport {
    started_at: String,
    completed_at: String,
    total_apis: usize,
    phase1_done: usize,
    phase2_done: usize,
    failed_count: usize,
    entries: Vec<HtmlSurveyEntry>,
}

// ── Page analysis ──

struct PageAnalysis {
    page_size_bytes: usize,

    // Swagger
    has_swagger_json: bool,
    swagger_json_empty: bool,
    swagger_ops_count: Option<usize>,

    // pk 탐지
    has_pk: bool,
    pk_value: Option<String>,
    pk_source: Option<String>,

    // select 옵션
    select_options: Vec<SelectOption>,

    anomalies: Vec<String>,
}

/// openapi.do 페이지 HTML에서 모든 신호를 추출한다.
fn analyze_page_html(html: &str) -> PageAnalysis {
    let page_size_bytes = html.len();
    let document = Html::parse_document(html);

    // ── Swagger 탐지 ──
    let swagger_json = korea_cli::core::swagger::extract_swagger_json(html);
    let swagger_json_empty = is_swagger_empty(html);

    let swagger_ops_count = swagger_json.as_ref().and_then(|json| {
        match korea_cli::core::swagger::parse_swagger("_html_survey", json) {
            Ok(spec) => Some(spec.operations.len()),
            Err(_) => None,
        }
    });

    // ── pk 탐지 (3단계) ──
    let (has_pk, pk_value, pk_source) = extract_pk(&document, html);

    // ── select 옵션 ──
    let select_options = extract_select_options(&document);

    // ── anomalies ──
    let anomalies = detect_anomalies(html, page_size_bytes);

    PageAnalysis {
        page_size_bytes,
        has_swagger_json: swagger_json.is_some(),
        swagger_json_empty,
        swagger_ops_count,
        has_pk,
        pk_value,
        pk_source,
        select_options,
        anomalies,
    }
}

/// `var swaggerJson = `` ` (빈 backtick) 여부 확인
fn is_swagger_empty(html: &str) -> bool {
    if let Ok(re) = Regex::new(r"var\s+swaggerJson\s*=\s*`\s*`") {
        re.is_match(html)
    } else {
        false
    }
}

/// pk를 3단계로 탐지한다. 어떤 방법으로 찾았는지 pk_source에 기록.
fn extract_pk(document: &Html, raw_html: &str) -> (bool, Option<String>, Option<String>) {
    // 1) id= 셀렉터
    if let Ok(sel) = Selector::parse(r#"input#publicDataDetailPk"#) {
        if let Some(el) = document.select(&sel).next() {
            if let Some(val) = el.value().attr("value") {
                if !val.is_empty() {
                    return (true, Some(val.to_string()), Some("id_attr".to_string()));
                }
            }
        }
    }

    // 2) name= 셀렉터
    if let Ok(sel) = Selector::parse(r#"input[name="publicDataDetailPk"]"#) {
        if let Some(el) = document.select(&sel).next() {
            if let Some(val) = el.value().attr("value") {
                if !val.is_empty() {
                    return (true, Some(val.to_string()), Some("name_attr".to_string()));
                }
            }
        }
    }

    // 3) regex fallback
    if let Ok(re) = Regex::new(
        r#"(?s)(?:name|id)\s*=\s*["']?publicDataDetailPk["']?\s+value\s*=\s*["']([^"']+)["']"#,
    ) {
        if let Some(caps) = re.captures(raw_html) {
            if let Some(m) = caps.get(1) {
                return (
                    true,
                    Some(m.as_str().to_string()),
                    Some("regex".to_string()),
                );
            }
        }
    }

    (false, None, None)
}

/// `#open_api_detail_select option` 파싱 (빈 value 옵션 제외)
fn extract_select_options(document: &Html) -> Vec<SelectOption> {
    let sel = match Selector::parse("#open_api_detail_select option") {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    document
        .select(&sel)
        .filter_map(|el| {
            let seq = el.value().attr("value")?.to_string();
            if seq.is_empty() {
                return None;
            }
            let name = el.text().collect::<String>().trim().to_string();
            Some(SelectOption { seq_no: seq, name })
        })
        .collect()
}

/// HTML 경로에 집중한 간소화된 anomaly 탐지
fn detect_anomalies(html: &str, page_size_bytes: usize) -> Vec<String> {
    let mut anomalies = Vec::new();

    if html.contains("폐기") {
        anomalies.push("deprecated_notice".into());
    }
    if html.contains("서비스 종료") || html.contains("서비스종료") {
        anomalies.push("service_terminated".into());
    }
    if (html.contains("location.href") || html.contains("location.replace"))
        && !html.contains("swaggerUrl")
    {
        anomalies.push("js_redirect".into());
    }
    if page_size_bytes < 1000 {
        anomalies.push("very_short_page".into());
    }

    anomalies
}

// ── AJAX probe ──

struct AjaxProbeResult {
    status: Option<u16>,
    error: Option<String>,
    response_bytes: Option<usize>,
    has_request_url: bool,
    has_service_url: bool,
    request_url: Option<String>,
    service_url: Option<String>,
    param_count: usize,
    param_style: Option<String>,
    response_field_count: usize,
    is_error_page: bool,
}

/// `selectApiDetailFunction.do` POST 호출로 operation 상세를 프로브한다.
async fn probe_ajax(
    client: &reqwest::Client,
    list_id: &str,
    pk: &str,
    first_seq_no: &str,
) -> AjaxProbeResult {
    let url = "https://www.data.go.kr/tcs/dss/selectApiDetailFunction.do";
    let referer = format!("https://www.data.go.kr/data/{list_id}/openapi.do");
    let body = format!("publicDataDetailPk={pk}&oprtinSeqNo={first_seq_no}&publicDataPk={list_id}");

    let resp = match client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Referer", &referer)
        .body(body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return AjaxProbeResult {
                status: None,
                error: Some(format!("request error: {e}")),
                response_bytes: None,
                has_request_url: false,
                has_service_url: false,
                request_url: None,
                service_url: None,
                param_count: 0,
                param_style: None,
                response_field_count: 0,
                is_error_page: false,
            };
        }
    };

    let status = resp.status().as_u16();
    let html = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            return AjaxProbeResult {
                status: Some(status),
                error: Some(format!("body read error: {e}")),
                response_bytes: None,
                has_request_url: false,
                has_service_url: false,
                request_url: None,
                service_url: None,
                param_count: 0,
                param_style: None,
                response_field_count: 0,
                is_error_page: false,
            };
        }
    };

    let response_bytes = html.len();

    // 에러 페이지 감지
    let is_error_page =
        html.contains("<title>에러") || html.contains("요청하신 페이지를 찾을 수 없습니다");

    // 요청주소 / 서비스URL 추출
    let request_url = extract_labeled_url_from_html(&html, "요청주소");
    let service_url = extract_labeled_url_from_html(&html, "서비스URL");

    // 파라미터 스타일 + 수 감지
    let (param_count, param_style) = detect_param_style(&html);

    // 응답 필드 수
    let response_field_count = count_response_fields(&html);

    AjaxProbeResult {
        status: Some(status),
        error: None,
        response_bytes: Some(response_bytes),
        has_request_url: request_url.is_some(),
        has_service_url: service_url.is_some(),
        request_url,
        service_url,
        param_count,
        param_style: Some(param_style),
        response_field_count,
        is_error_page,
    }
}

/// `<strong>label</strong>` 뒤의 `http`로 시작하는 URL 추출
fn extract_labeled_url_from_html(html: &str, label: &str) -> Option<String> {
    let document = Html::parse_fragment(html);
    let sel = Selector::parse("strong").ok()?;

    for el in document.select(&sel) {
        let text = el.text().collect::<String>();
        if text.contains(label) {
            if let Some(parent) = el.parent() {
                let parent_text: String = parent
                    .children()
                    .filter_map(|child| child.value().as_text().map(|t| t.text.trim().to_string()))
                    .collect::<Vec<_>>()
                    .join(" ");
                for word in parent_text.split_whitespace() {
                    if word.starts_with("http") {
                        return Some(word.to_string());
                    }
                }
            }
        }
    }
    None
}

/// 파라미터 스타일 감지: data_attr → td_fallback → none
fn detect_param_style(html: &str) -> (usize, String) {
    let document = Html::parse_fragment(html);

    // data_attr 방식
    if let Ok(sel) = Selector::parse("tr[data-paramtr-nm]") {
        let rows: Vec<_> = document.select(&sel).collect();
        if !rows.is_empty() {
            return (rows.len(), "data_attr".to_string());
        }
    }

    // td_fallback: 6+ 컬럼 <td> 테이블
    if let Ok(tr_sel) = Selector::parse("tr") {
        if let Ok(td_sel) = Selector::parse("td") {
            let mut param_rows = 0;
            for row in document.select(&tr_sel) {
                let cells: Vec<_> = row.select(&td_sel).collect();
                if cells.len() >= 6 {
                    // 헤더 행 제외
                    let first_cell = cells[1].text().collect::<String>();
                    let first_cell = first_cell.trim();
                    if !first_cell.is_empty()
                        && first_cell != "항목명(영문)"
                        && first_cell != "항목명"
                    {
                        param_rows += 1;
                    }
                }
            }
            if param_rows > 0 {
                return (param_rows, "td_fallback".to_string());
            }
        }
    }

    (0, "none".to_string())
}

/// `출력결과` 또는 `응답메시지` 섹션 이후 `<tr>` 행 수 (응답 필드 수)
fn count_response_fields(html: &str) -> usize {
    let document = Html::parse_fragment(html);
    let tr_sel = match Selector::parse("tr") {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let td_sel = match Selector::parse("td") {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let mut in_response_section = false;
    let mut count = 0;

    for row in document.select(&tr_sel) {
        let row_text = row.text().collect::<String>();
        if row_text.contains("출력결과") || row_text.contains("응답메시지") {
            in_response_section = true;
            continue;
        }
        if in_response_section {
            let cells: Vec<_> = row.select(&td_sel).collect();
            if cells.len() >= 3 {
                let name_cell = cells[1].text().collect::<String>();
                let name_cell = name_cell.trim();
                if !name_cell.is_empty() && name_cell != "항목명(영문)" && name_cell != "항목명"
                {
                    count += 1;
                }
            }
        }
    }

    count
}

// ── Classification ──

fn classify_page_pattern(analysis: &PageAnalysis, ajax: Option<&AjaxProbeResult>) -> String {
    // 폐기/서비스 종료
    if analysis
        .anomalies
        .iter()
        .any(|a| a == "deprecated_notice" || a == "service_terminated")
    {
        return "deprecated".to_string();
    }

    // 페이지 접근 실패는 caller에서 처리 (fetch_error가 있으면 이 함수에 오지 않음)

    // Swagger full
    if analysis.swagger_ops_count.unwrap_or(0) > 0 {
        return "swagger_full".to_string();
    }

    if !analysis.has_pk {
        return "no_pk".to_string();
    }

    if analysis.select_options.is_empty() {
        return "pk_no_options".to_string();
    }

    // pk + options → AJAX 결과 확인
    if let Some(ajax) = ajax {
        if ajax.is_error_page || ajax.error.is_some() {
            return "pk_no_ajax".to_string();
        }
        if analysis.swagger_json_empty {
            return "swagger_empty_html_ok".to_string();
        }
        return "html_only".to_string();
    }

    // AJAX 미시도 (phase1-only 등)
    if analysis.swagger_json_empty {
        return "swagger_empty_html_ok".to_string();
    }
    "html_only".to_string()
}

// ── Resume support ──

fn load_existing_survey(path: &str) -> HashMap<String, HtmlSurveyEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let report: HtmlSurveyReport = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };
    report
        .entries
        .into_iter()
        .map(|e| (e.list_id.clone(), e))
        .collect()
}

// ── CLI args ──

#[derive(Clone)]
struct SurveyConfig {
    api_key: String,
    output: String,
    concurrency: usize,
    delay_ms: u64,
    resume: bool,
    phase1_only: bool,
}

fn parse_args() -> Result<SurveyConfig> {
    let args: Vec<String> = std::env::args().collect();

    let api_key = get_arg(&args, "--api-key")
        .or_else(|| std::env::var("DATA_GO_KR_API_KEY").ok())
        .ok_or_else(|| anyhow::anyhow!("--api-key 또는 DATA_GO_KR_API_KEY 환경변수 필요"))?;

    let output = get_arg(&args, "--output").unwrap_or_else(|| "data/html_survey.json".into());
    let concurrency: usize = get_arg(&args, "--concurrency")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let delay_ms: u64 = get_arg(&args, "--delay")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let resume = args.iter().any(|a| a == "--resume");
    let phase1_only = args.iter().any(|a| a == "--phase1-only");

    Ok(SurveyConfig {
        api_key,
        output,
        concurrency,
        delay_ms,
        resume,
        phase1_only,
    })
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

// ── Main ──

#[tokio::main]
async fn main() -> Result<()> {
    let config = parse_args()?;
    let start = Instant::now();
    let started_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    // Step 1: 카탈로그 수집
    eprintln!("=== Step 1/4: 카탈로그 수집 ===");
    let services = fetch_all_services(&config.api_key).await?;
    eprintln!("  {} 서비스 수집 완료", services.len());

    // Resume: 기존 결과 로드
    let mut existing = if config.resume {
        let loaded = load_existing_survey(&config.output);
        if !loaded.is_empty() {
            eprintln!("  기존 조사 결과 {} 건 로드 (이어하기)", loaded.len());
        }
        loaded
    } else {
        HashMap::new()
    };

    let client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-html-survey/0.1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .context("HTTP 클라이언트 생성 실패")?,
    );

    // ── Phase 1: openapi.do 페이지 fetch + analyze ──
    let phase1_targets: Vec<_> = services
        .iter()
        .filter(|svc| !existing.contains_key(&svc.list_id))
        .collect();

    eprintln!(
        "\n=== Step 2/4: Phase 1 — openapi.do 페이지 분석 (동시 {}건) ===",
        config.concurrency
    );
    eprintln!(
        "  신규 조사 대상: {} / 기존: {}",
        phase1_targets.len(),
        existing.len()
    );

    let done_count = Arc::new(AtomicUsize::new(0));
    let total_phase1 = phase1_targets.len();

    let phase1_entries: Vec<HtmlSurveyEntry> = stream::iter(phase1_targets.into_iter())
        .map(|svc| {
            let client = client.clone();
            let list_id = svc.list_id.clone();
            let title = svc.title.clone();
            let delay_ms = config.delay_ms;
            let done = done_count.clone();

            async move {
                let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");

                let (http_status, fetch_error, analysis) = match client.get(&page_url).send().await
                {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        match resp.text().await {
                            Ok(html) => (Some(status), None, Some(analyze_page_html(&html))),
                            Err(e) => (Some(status), Some(format!("body read error: {e}")), None),
                        }
                    }
                    Err(e) => (None, Some(format!("request error: {e}")), None),
                };

                let count = done.fetch_add(1, Ordering::Relaxed) + 1;
                if total_phase1 > 0 && count.is_multiple_of(500) {
                    eprintln!("  Phase 1 진행: {count}/{total_phase1}");
                }

                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;

                match analysis {
                    Some(a) => {
                        let page_pattern = classify_page_pattern(&a, None);
                        HtmlSurveyEntry {
                            list_id,
                            title,
                            http_status,
                            fetch_error,
                            page_size_bytes: a.page_size_bytes,
                            has_swagger_json: a.has_swagger_json,
                            swagger_json_empty: a.swagger_json_empty,
                            swagger_ops_count: a.swagger_ops_count,
                            has_pk: a.has_pk,
                            pk_value: a.pk_value,
                            pk_source: a.pk_source,
                            select_option_count: a.select_options.len(),
                            select_options: a.select_options,
                            ajax_attempted: false,
                            ajax_status: None,
                            ajax_error: None,
                            ajax_response_bytes: None,
                            ajax_has_request_url: false,
                            ajax_has_service_url: false,
                            ajax_request_url: None,
                            ajax_service_url: None,
                            ajax_param_count: 0,
                            ajax_param_style: None,
                            ajax_response_field_count: 0,
                            ajax_is_error_page: false,
                            page_pattern,
                            anomalies: a.anomalies,
                        }
                    }
                    None => HtmlSurveyEntry {
                        list_id,
                        title,
                        http_status,
                        fetch_error,
                        page_size_bytes: 0,
                        has_swagger_json: false,
                        swagger_json_empty: false,
                        swagger_ops_count: None,
                        has_pk: false,
                        pk_value: None,
                        pk_source: None,
                        select_option_count: 0,
                        select_options: vec![],
                        ajax_attempted: false,
                        ajax_status: None,
                        ajax_error: None,
                        ajax_response_bytes: None,
                        ajax_has_request_url: false,
                        ajax_has_service_url: false,
                        ajax_request_url: None,
                        ajax_service_url: None,
                        ajax_param_count: 0,
                        ajax_param_style: None,
                        ajax_response_field_count: 0,
                        ajax_is_error_page: false,
                        page_pattern: "fetch_failed".to_string(),
                        anomalies: vec!["page_fetch_failed".into()],
                    },
                }
            }
        })
        .buffer_unordered(config.concurrency)
        .collect()
        .await;

    // Phase 1 결과 병합
    for entry in phase1_entries {
        existing.insert(entry.list_id.clone(), entry);
    }

    let phase1_done = existing.len();
    eprintln!("  Phase 1 완료: {phase1_done} 건");

    // 중간 저장 (Phase 2 전)
    save_report(&config.output, &started_at, &existing)?;

    // ── Phase 2: AJAX 프로브 ──
    if config.phase1_only {
        eprintln!("\n  --phase1-only: AJAX 프로브 생략");
    } else {
        // pk + select 옵션 1+ 있고 아직 AJAX 시도 안 한 항목
        let phase2_targets: Vec<_> = existing
            .values()
            .filter(|e| e.has_pk && e.select_option_count > 0 && !e.ajax_attempted)
            .map(|e| {
                (
                    e.list_id.clone(),
                    e.pk_value.clone().unwrap_or_default(),
                    e.select_options
                        .first()
                        .map(|o| o.seq_no.clone())
                        .unwrap_or_default(),
                )
            })
            .collect();

        eprintln!(
            "\n=== Step 3/4: Phase 2 — AJAX 프로브 (동시 {}건) ===",
            config.concurrency
        );
        eprintln!("  AJAX 대상: {} 건", phase2_targets.len());

        let done_count = Arc::new(AtomicUsize::new(0));
        let total_phase2 = phase2_targets.len();

        let ajax_results: Vec<(String, AjaxProbeResult)> = stream::iter(phase2_targets.into_iter())
            .map(|(list_id, pk, seq_no)| {
                let client = client.clone();
                let delay_ms = config.delay_ms;
                let done = done_count.clone();

                async move {
                    let result = probe_ajax(&client, &list_id, &pk, &seq_no).await;

                    let count = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if total_phase2 > 0 && count.is_multiple_of(500) {
                        eprintln!("  Phase 2 진행: {count}/{total_phase2}");
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    (list_id, result)
                }
            })
            .buffer_unordered(config.concurrency)
            .collect()
            .await;

        // Phase 2 결과 병합
        for (list_id, ajax) in &ajax_results {
            if let Some(entry) = existing.get_mut(list_id) {
                entry.ajax_attempted = true;
                entry.ajax_status = ajax.status;
                entry.ajax_error = ajax.error.clone();
                entry.ajax_response_bytes = ajax.response_bytes;
                entry.ajax_has_request_url = ajax.has_request_url;
                entry.ajax_has_service_url = ajax.has_service_url;
                entry.ajax_request_url = ajax.request_url.clone();
                entry.ajax_service_url = ajax.service_url.clone();
                entry.ajax_param_count = ajax.param_count;
                entry.ajax_param_style = ajax.param_style.clone();
                entry.ajax_response_field_count = ajax.response_field_count;
                entry.ajax_is_error_page = ajax.is_error_page;

                // 분류 재계산 (AJAX 결과 반영)
                let analysis = PageAnalysis {
                    page_size_bytes: entry.page_size_bytes,
                    has_swagger_json: entry.has_swagger_json,
                    swagger_json_empty: entry.swagger_json_empty,
                    swagger_ops_count: entry.swagger_ops_count,
                    has_pk: entry.has_pk,
                    pk_value: entry.pk_value.clone(),
                    pk_source: entry.pk_source.clone(),
                    select_options: entry.select_options.clone(),
                    anomalies: entry.anomalies.clone(),
                };
                entry.page_pattern = classify_page_pattern(&analysis, Some(ajax));
            }
        }

        eprintln!("  Phase 2 완료: {} 건", ajax_results.len());
    }

    let completed_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    // ── 보고서 생성 + 저장 ──
    eprintln!("\n=== Step 4/4: 보고서 생성 ===");

    let all_entries: Vec<HtmlSurveyEntry> = existing.into_values().collect();
    let phase2_done = all_entries.iter().filter(|e| e.ajax_attempted).count();
    let failed_count = all_entries
        .iter()
        .filter(|e| e.fetch_error.is_some())
        .count();

    let report = HtmlSurveyReport {
        started_at,
        completed_at,
        total_apis: all_entries.len(),
        phase1_done,
        phase2_done,
        failed_count,
        entries: all_entries,
    };

    print_summary(&report);

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

/// 중간 저장: Phase 2 전에 Phase 1 결과를 안전하게 저장
fn save_report(
    output: &str,
    started_at: &str,
    entries: &HashMap<String, HtmlSurveyEntry>,
) -> Result<()> {
    let all: Vec<HtmlSurveyEntry> = entries.values().cloned().collect();
    let phase2_done = all.iter().filter(|e| e.ajax_attempted).count();
    let failed_count = all.iter().filter(|e| e.fetch_error.is_some()).count();

    let report = HtmlSurveyReport {
        started_at: started_at.to_string(),
        completed_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        total_apis: all.len(),
        phase1_done: all.len(),
        phase2_done,
        failed_count,
        entries: all,
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::create_dir_all(
        std::path::Path::new(output)
            .parent()
            .unwrap_or(std::path::Path::new(".")),
    )?;
    std::fs::write(output, &json)?;
    eprintln!("  중간 저장 완료: {output}");
    Ok(())
}

// ── Summary ──

fn print_summary(report: &HtmlSurveyReport) {
    eprintln!("\n--- HTML 전수조사 요약 ---");
    eprintln!("전체: {}", report.total_apis);
    eprintln!(
        "Phase 1: {} / Phase 2: {} / 실패: {}",
        report.phase1_done, report.phase2_done, report.failed_count
    );

    // pk 탐지
    let pk_id = report
        .entries
        .iter()
        .filter(|e| e.pk_source.as_deref() == Some("id_attr"))
        .count();
    let pk_name = report
        .entries
        .iter()
        .filter(|e| e.pk_source.as_deref() == Some("name_attr"))
        .count();
    let pk_regex = report
        .entries
        .iter()
        .filter(|e| e.pk_source.as_deref() == Some("regex"))
        .count();
    let pk_none = report.entries.iter().filter(|e| !e.has_pk).count();

    eprintln!("\n[pk 탐지]");
    eprintln!("  id= 발견: {pk_id}");
    eprintln!("  name= 발견: {pk_name}");
    eprintln!("  regex 발견: {pk_regex}");
    eprintln!("  미발견: {pk_none}");

    // select 옵션
    let has_options = report
        .entries
        .iter()
        .filter(|e| e.select_option_count > 0)
        .count();
    let no_options = report
        .entries
        .iter()
        .filter(|e| e.select_option_count == 0)
        .count();

    eprintln!("\n[select 옵션]");
    eprintln!("  1+ 옵션: {has_options}");
    eprintln!("  0 옵션: {no_options}");

    // AJAX 프로브
    let ajax_success_full = report
        .entries
        .iter()
        .filter(|e| e.ajax_attempted && e.ajax_has_request_url && e.ajax_param_count > 0)
        .count();
    let ajax_success_partial = report
        .entries
        .iter()
        .filter(|e| {
            e.ajax_attempted
                && e.ajax_error.is_none()
                && !e.ajax_is_error_page
                && !(e.ajax_has_request_url && e.ajax_param_count > 0)
        })
        .count();
    let ajax_error_page = report
        .entries
        .iter()
        .filter(|e| e.ajax_attempted && e.ajax_is_error_page)
        .count();
    let ajax_http_error = report
        .entries
        .iter()
        .filter(|e| e.ajax_attempted && e.ajax_error.is_some())
        .count();
    let ajax_not_tried = report.entries.iter().filter(|e| !e.ajax_attempted).count();

    eprintln!("\n[AJAX 프로브]");
    eprintln!("  성공 (요청주소+파라미터): {ajax_success_full}");
    eprintln!("  성공 (부분): {ajax_success_partial}");
    eprintln!("  에러 페이지: {ajax_error_page}");
    eprintln!("  HTTP 에러: {ajax_http_error}");
    eprintln!("  미시도: {ajax_not_tried}");

    // 교차 분석
    let swagger_and_pk = report
        .entries
        .iter()
        .filter(|e| e.swagger_ops_count.unwrap_or(0) > 0 && e.has_pk)
        .count();
    let no_swagger_pk_ajax_ok = report
        .entries
        .iter()
        .filter(|e| {
            e.swagger_ops_count.unwrap_or(0) == 0
                && e.has_pk
                && e.ajax_attempted
                && e.ajax_has_request_url
                && !e.ajax_is_error_page
        })
        .count();
    let no_swagger_no_pk = report
        .entries
        .iter()
        .filter(|e| e.swagger_ops_count.unwrap_or(0) == 0 && !e.has_pk)
        .count();

    eprintln!("\n[교차 분석]");
    eprintln!("  Swagger 있음 + pk 있음: {swagger_and_pk}");
    eprintln!("  Swagger 없음 + pk+AJAX 성공: {no_swagger_pk_ajax_ok}  <- 신규 추출 가능");
    eprintln!("  Swagger 없음 + pk 없음: {no_swagger_no_pk}");

    // page_pattern 분포
    let mut pattern_counts: HashMap<String, usize> = HashMap::new();
    for entry in &report.entries {
        *pattern_counts
            .entry(entry.page_pattern.clone())
            .or_default() += 1;
    }
    let mut patterns: Vec<_> = pattern_counts.into_iter().collect();
    patterns.sort_by(|a, b| b.1.cmp(&a.1));

    eprintln!("\n[page_pattern 분포]");
    for (pattern, count) in &patterns {
        eprintln!("  {count:>5}  {pattern}");
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_page_html_pk_id() {
        let padding = " ".repeat(800);
        let html = format!(
            r#"<html><body>
            <input type="hidden" id="publicDataDetailPk" value="uddi:abc-123"/>
            <select id="open_api_detail_select">
                <option value="101">getItems</option>
            </select>
            <!-- {padding} -->
            </body></html>"#
        );

        let a = analyze_page_html(&html);
        assert!(a.has_pk);
        assert_eq!(a.pk_value.as_deref(), Some("uddi:abc-123"));
        assert_eq!(a.pk_source.as_deref(), Some("id_attr"));
        assert_eq!(a.select_options.len(), 1);
        assert_eq!(a.select_options[0].seq_no, "101");
        assert_eq!(a.select_options[0].name, "getItems");
    }

    #[test]
    fn test_analyze_page_html_pk_name() {
        let padding = " ".repeat(800);
        let html = format!(
            r#"<html><body>
            <input type="hidden" name="publicDataDetailPk" value="uddi:xyz-789"/>
            <!-- {padding} -->
            </body></html>"#
        );

        let a = analyze_page_html(&html);
        assert!(a.has_pk);
        assert_eq!(a.pk_value.as_deref(), Some("uddi:xyz-789"));
        assert_eq!(a.pk_source.as_deref(), Some("name_attr"));
    }

    #[test]
    fn test_analyze_page_html_no_pk() {
        let padding = " ".repeat(800);
        let html = format!(r#"<html><body><p>nothing here</p><!-- {padding} --></body></html>"#);

        let a = analyze_page_html(&html);
        assert!(!a.has_pk);
        assert!(a.pk_value.is_none());
        assert!(a.pk_source.is_none());
    }

    #[test]
    fn test_analyze_page_html_swagger_empty() {
        let padding = " ".repeat(800);
        let html = format!(
            r#"<html><body>
            <script>var swaggerJson = ``</script>
            <!-- {padding} -->
            </body></html>"#
        );

        let a = analyze_page_html(&html);
        assert!(a.swagger_json_empty);
        assert!(!a.has_swagger_json);
    }

    #[test]
    fn test_analyze_page_html_swagger_with_ops() {
        let padding = " ".repeat(800);
        let html = format!(
            r#"<html><body>
            <script>
            var swaggerJson = `{{"swagger":"2.0","host":"api.test.kr","basePath":"/","schemes":["https"],"paths":{{"/items":{{"get":{{"summary":"test","parameters":[],"responses":{{"200":{{}}}}}}}}}}}}`
            </script>
            <!-- {padding} -->
            </body></html>"#
        );

        let a = analyze_page_html(&html);
        assert!(a.has_swagger_json);
        assert!(!a.swagger_json_empty);
        assert_eq!(a.swagger_ops_count, Some(1));
    }

    #[test]
    fn test_detect_anomalies_deprecated() {
        let html = "<html><body>이 API는 폐기되었습니다.</body></html>";
        let anomalies = detect_anomalies(html, html.len());
        assert!(anomalies.contains(&"deprecated_notice".to_string()));
        assert!(anomalies.contains(&"very_short_page".to_string()));
    }

    #[test]
    fn test_detect_anomalies_service_terminated() {
        let html = "<html><body>서비스 종료</body></html>";
        let anomalies = detect_anomalies(html, html.len());
        assert!(anomalies.contains(&"service_terminated".to_string()));
    }

    #[test]
    fn test_detect_anomalies_js_redirect() {
        let html = "<html><body><script>location.href='http://other.kr';</script></body></html>";
        let anomalies = detect_anomalies(html, html.len());
        assert!(anomalies.contains(&"js_redirect".to_string()));
    }

    #[test]
    fn test_classify_swagger_full() {
        let a = PageAnalysis {
            page_size_bytes: 5000,
            has_swagger_json: true,
            swagger_json_empty: false,
            swagger_ops_count: Some(3),
            has_pk: true,
            pk_value: Some("uddi:test".into()),
            pk_source: Some("id_attr".into()),
            select_options: vec![SelectOption {
                seq_no: "1".into(),
                name: "op".into(),
            }],
            anomalies: vec![],
        };
        assert_eq!(classify_page_pattern(&a, None), "swagger_full");
    }

    #[test]
    fn test_classify_no_pk() {
        let a = PageAnalysis {
            page_size_bytes: 5000,
            has_swagger_json: false,
            swagger_json_empty: false,
            swagger_ops_count: None,
            has_pk: false,
            pk_value: None,
            pk_source: None,
            select_options: vec![],
            anomalies: vec![],
        };
        assert_eq!(classify_page_pattern(&a, None), "no_pk");
    }

    #[test]
    fn test_classify_deprecated() {
        let a = PageAnalysis {
            page_size_bytes: 5000,
            has_swagger_json: false,
            swagger_json_empty: false,
            swagger_ops_count: None,
            has_pk: true,
            pk_value: Some("uddi:test".into()),
            pk_source: Some("id_attr".into()),
            select_options: vec![],
            anomalies: vec!["deprecated_notice".into()],
        };
        assert_eq!(classify_page_pattern(&a, None), "deprecated");
    }

    #[test]
    fn test_classify_html_only_with_ajax() {
        let a = PageAnalysis {
            page_size_bytes: 5000,
            has_swagger_json: false,
            swagger_json_empty: false,
            swagger_ops_count: None,
            has_pk: true,
            pk_value: Some("uddi:test".into()),
            pk_source: Some("id_attr".into()),
            select_options: vec![SelectOption {
                seq_no: "1".into(),
                name: "op".into(),
            }],
            anomalies: vec![],
        };
        let ajax = AjaxProbeResult {
            status: Some(200),
            error: None,
            response_bytes: Some(5000),
            has_request_url: true,
            has_service_url: true,
            request_url: Some("http://test.kr/api/items".into()),
            service_url: Some("http://test.kr/api".into()),
            param_count: 3,
            param_style: Some("data_attr".into()),
            response_field_count: 5,
            is_error_page: false,
        };
        assert_eq!(classify_page_pattern(&a, Some(&ajax)), "html_only");
    }

    #[test]
    fn test_classify_swagger_empty_html_ok() {
        let a = PageAnalysis {
            page_size_bytes: 5000,
            has_swagger_json: false,
            swagger_json_empty: true,
            swagger_ops_count: None,
            has_pk: true,
            pk_value: Some("uddi:test".into()),
            pk_source: Some("id_attr".into()),
            select_options: vec![SelectOption {
                seq_no: "1".into(),
                name: "op".into(),
            }],
            anomalies: vec![],
        };
        let ajax = AjaxProbeResult {
            status: Some(200),
            error: None,
            response_bytes: Some(5000),
            has_request_url: true,
            has_service_url: true,
            request_url: Some("http://test.kr/api/items".into()),
            service_url: Some("http://test.kr/api".into()),
            param_count: 3,
            param_style: Some("data_attr".into()),
            response_field_count: 5,
            is_error_page: false,
        };
        assert_eq!(
            classify_page_pattern(&a, Some(&ajax)),
            "swagger_empty_html_ok"
        );
    }

    #[test]
    fn test_classify_pk_no_ajax() {
        let a = PageAnalysis {
            page_size_bytes: 5000,
            has_swagger_json: false,
            swagger_json_empty: false,
            swagger_ops_count: None,
            has_pk: true,
            pk_value: Some("uddi:test".into()),
            pk_source: Some("id_attr".into()),
            select_options: vec![SelectOption {
                seq_no: "1".into(),
                name: "op".into(),
            }],
            anomalies: vec![],
        };
        let ajax = AjaxProbeResult {
            status: Some(200),
            error: None,
            response_bytes: Some(500),
            has_request_url: false,
            has_service_url: false,
            request_url: None,
            service_url: None,
            param_count: 0,
            param_style: None,
            response_field_count: 0,
            is_error_page: true,
        };
        assert_eq!(classify_page_pattern(&a, Some(&ajax)), "pk_no_ajax");
    }

    #[test]
    fn test_classify_pk_no_options() {
        let a = PageAnalysis {
            page_size_bytes: 5000,
            has_swagger_json: false,
            swagger_json_empty: false,
            swagger_ops_count: None,
            has_pk: true,
            pk_value: Some("uddi:test".into()),
            pk_source: Some("id_attr".into()),
            select_options: vec![],
            anomalies: vec![],
        };
        assert_eq!(classify_page_pattern(&a, None), "pk_no_options");
    }

    #[test]
    fn test_select_options_extraction() {
        let html = r#"
            <html><body>
            <select id="open_api_detail_select">
                <option value="">선택</option>
                <option value="101">getWeather</option>
                <option value="102">getForecast</option>
            </select>
            </body></html>
        "#;
        let doc = Html::parse_document(html);
        let opts = extract_select_options(&doc);
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0].seq_no, "101");
        assert_eq!(opts[0].name, "getWeather");
        assert_eq!(opts[1].seq_no, "102");
        assert_eq!(opts[1].name, "getForecast");
    }

    #[test]
    fn test_is_swagger_empty() {
        assert!(is_swagger_empty("var swaggerJson = ``"));
        assert!(is_swagger_empty("var swaggerJson = ` `"));
        assert!(!is_swagger_empty(
            r#"var swaggerJson = `{"swagger":"2.0"}`"#
        ));
        assert!(!is_swagger_empty("no swagger here"));
    }

    #[test]
    fn test_detect_param_style_data_attr() {
        let html = r#"
            <table>
                <tr data-paramtr-nm="serviceKey"><td>1</td></tr>
                <tr data-paramtr-nm="pageNo"><td>2</td></tr>
            </table>
        "#;
        let (count, style) = detect_param_style(html);
        assert_eq!(count, 2);
        assert_eq!(style, "data_attr");
    }

    #[test]
    fn test_detect_param_style_td_fallback() {
        let html = r#"
            <table>
                <tr><td>순번</td><td>항목명(영문)</td><td>타입</td><td>크기</td><td>구분</td><td>설명</td></tr>
                <tr><td>1</td><td>serviceKey</td><td>string</td><td>100</td><td>필수</td><td>인증키</td></tr>
                <tr><td>2</td><td>pageNo</td><td>string</td><td>10</td><td>옵션</td><td>번호</td></tr>
            </table>
        "#;
        let (count, style) = detect_param_style(html);
        assert_eq!(count, 2);
        assert_eq!(style, "td_fallback");
    }

    #[test]
    fn test_detect_param_style_none() {
        let html = "<div>no params</div>";
        let (count, style) = detect_param_style(html);
        assert_eq!(count, 0);
        assert_eq!(style, "none");
    }

    #[test]
    fn test_count_response_fields() {
        let html = r#"
            <table>
                <tr><td colspan="6">출력결과</td></tr>
                <tr><td>1</td><td>resultCode</td><td>string</td><td>10</td><td>필수</td><td>결과코드</td></tr>
                <tr><td>2</td><td>resultMsg</td><td>string</td><td>100</td><td>필수</td><td>결과메시지</td></tr>
            </table>
        "#;
        assert_eq!(count_response_fields(html), 2);
    }

    #[test]
    fn test_count_response_fields_none() {
        let html = "<div>no response section</div>";
        assert_eq!(count_response_fields(html), 0);
    }

    #[test]
    fn test_survey_entry_json_roundtrip() {
        let entry = HtmlSurveyEntry {
            list_id: "15084084".into(),
            title: "Test API".into(),
            http_status: Some(200),
            fetch_error: None,
            page_size_bytes: 50000,
            has_swagger_json: false,
            swagger_json_empty: true,
            swagger_ops_count: None,
            has_pk: true,
            pk_value: Some("uddi:abc-123".into()),
            pk_source: Some("id_attr".into()),
            select_option_count: 2,
            select_options: vec![
                SelectOption {
                    seq_no: "101".into(),
                    name: "getItems".into(),
                },
                SelectOption {
                    seq_no: "102".into(),
                    name: "getDetail".into(),
                },
            ],
            ajax_attempted: true,
            ajax_status: Some(200),
            ajax_error: None,
            ajax_response_bytes: Some(8000),
            ajax_has_request_url: true,
            ajax_has_service_url: true,
            ajax_request_url: Some("https://api.test.kr/items".into()),
            ajax_service_url: Some("https://api.test.kr".into()),
            ajax_param_count: 3,
            ajax_param_style: Some("data_attr".into()),
            ajax_response_field_count: 5,
            ajax_is_error_page: false,
            page_pattern: "swagger_empty_html_ok".into(),
            anomalies: vec![],
        };

        let json = serde_json::to_string(&entry).unwrap();
        let decoded: HtmlSurveyEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.list_id, "15084084");
        assert_eq!(decoded.pk_source.as_deref(), Some("id_attr"));
        assert_eq!(decoded.select_option_count, 2);
        assert_eq!(decoded.ajax_param_count, 3);
        assert_eq!(decoded.page_pattern, "swagger_empty_html_ok");
    }

    #[test]
    fn test_report_json_roundtrip() {
        let report = HtmlSurveyReport {
            started_at: "2026-04-02T00:00:00".into(),
            completed_at: "2026-04-02T01:00:00".into(),
            total_apis: 12000,
            phase1_done: 12000,
            phase2_done: 8000,
            failed_count: 100,
            entries: vec![],
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let decoded: HtmlSurveyReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_apis, 12000);
        assert_eq!(decoded.phase1_done, 12000);
        assert_eq!(decoded.phase2_done, 8000);
    }

    #[test]
    fn test_load_existing_survey_empty() {
        let result = load_existing_survey("/nonexistent/path.json");
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_labeled_url_from_html() {
        let html = r#"<p><strong>요청주소</strong> https://apis.data.go.kr/test/getItems</p>"#;
        let url = extract_labeled_url_from_html(html, "요청주소");
        assert_eq!(
            url.as_deref(),
            Some("https://apis.data.go.kr/test/getItems")
        );
    }

    #[test]
    fn test_extract_labeled_url_from_html_missing() {
        let html = "<p>no labeled url</p>";
        let url = extract_labeled_url_from_html(html, "요청주소");
        assert!(url.is_none());
    }
}
