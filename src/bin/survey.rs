//! API 전수조사: data.go.kr 12K+ API의 openapi.do 페이지를 스캔하여
//! Swagger/HTML/외부URL 등 모든 패턴을 분류한다.
//!
//! Usage: cargo run --bin survey -- --api-key YOUR_KEY [--output data/survey.json] [--concurrency 5]

use serde::{Deserialize, Serialize};

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

fn main() {
    // TODO: Task 5에서 구현
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
