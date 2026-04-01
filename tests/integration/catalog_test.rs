#[test]
fn test_parse_meta_api_response() {
    let json = serde_json::json!({
        "currentCount": 2,
        "totalCount": 17174,
        "data": [
            {
                "list_id": "15081808",
                "list_title": "국세청_사업자등록정보 진위확인 및 상태조회 서비스",
                "id": "uddi:abc",
                "title": "사업자등록정보 서비스",
                "desc": "사업자 등록 정보 조회",
                "keywords": "사업자,국세청",
                "org_nm": "국세청",
                "new_category_nm": "공공행정",
                "end_point_url": "https://api.odcloud.kr/api/nts-businessman/v1",
                "data_format": "JSON",
                "is_confirmed_for_dev": "Y",
                "is_charged": "무료",
                "request_cnt": 1234,
                "updated_at": "2024-10-01",
                "operation_nm": "상태조회",
                "operation_seq": "12345",
                "request_param_nm": "\"사업자등록번호\"",
                "request_param_nm_en": "b_no",
                "response_param_nm": "\"납세자상태명\"",
                "response_param_nm_en": "b_stt"
            },
            {
                "list_id": "15081808",
                "list_title": "국세청_사업자등록정보 진위확인 및 상태조회 서비스",
                "id": "uddi:def",
                "title": "사업자등록정보 서비스",
                "desc": "사업자 등록 정보 조회",
                "keywords": "사업자,국세청",
                "org_nm": "국세청",
                "new_category_nm": "공공행정",
                "end_point_url": "https://api.odcloud.kr/api/nts-businessman/v1",
                "data_format": "JSON",
                "is_confirmed_for_dev": "Y",
                "is_charged": "무료",
                "request_cnt": 1234,
                "updated_at": "2024-10-01",
                "operation_nm": "진위확인",
                "operation_seq": "12346",
                "request_param_nm": "\"사업자등록번호\",\"개업일자\"",
                "request_param_nm_en": "b_no,start_dt",
                "response_param_nm": "\"진위확인결과\"",
                "response_param_nm_en": "valid"
            }
        ]
    });

    let services = korea_cli::core::catalog::parse_meta_response(&json).unwrap();

    // Two operations should be grouped into one service
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].list_id, "15081808");
    assert_eq!(services[0].operations.len(), 2);
    assert_eq!(services[0].operations[0].name, "상태조회");
    assert_eq!(services[0].operations[1].name, "진위확인");
    assert_eq!(services[0].keywords, vec!["사업자", "국세청"]);
    assert!(services[0].auto_approve);
}

#[test]
fn test_search_bundle_catalog() {
    use korea_cli::core::types::{CatalogEntry, SpecStatus};

    let catalog = vec![
        CatalogEntry {
            list_id: "15081808".into(),
            title: "국세청_사업자등록정보 서비스".into(),
            description: "사업자 등록 정보 조회".into(),
            keywords: vec!["사업자".into(), "국세청".into()],
            org_name: "국세청".into(),
            category: "공공행정".into(),
            request_count: 1234,
            endpoint_url: "https://api.odcloud.kr/api/nts-businessman/v1".into(),
            spec_status: SpecStatus::Available,
        },
        CatalogEntry {
            list_id: "15095478".into(),
            title: "한국공항공사_공항 소요시간 정보".into(),
            description: "공항 내 구간별 소요시간".into(),
            keywords: vec!["공항".into(), "소요시간".into()],
            org_name: "한국공항공사".into(),
            category: "교통".into(),
            request_count: 500,
            endpoint_url: "https://apis.data.go.kr/airport".into(),
            spec_status: SpecStatus::HtmlOnly,
        },
    ];

    let results = korea_cli::core::catalog::search_bundle_catalog(&catalog, "사업자", None, 10);
    assert_eq!(results.total, 1);
    assert_eq!(results.results[0].list_id, "15081808");

    let results = korea_cli::core::catalog::search_bundle_catalog(&catalog, "공항", None, 10);
    assert_eq!(results.total, 1);
    assert_eq!(results.results[0].list_id, "15095478");

    let results =
        korea_cli::core::catalog::search_bundle_catalog(&catalog, "공항", Some("교통"), 10);
    assert_eq!(results.total, 1);

    let results =
        korea_cli::core::catalog::search_bundle_catalog(&catalog, "공항", Some("공공행정"), 10);
    assert_eq!(results.total, 0);
}
