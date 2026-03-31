use korea_cli::core::caller;
use korea_cli::core::types::*;

fn make_test_spec() -> ApiSpec {
    ApiSpec {
        list_id: "test".into(),
        base_url: "https://api.odcloud.kr/api/test/v1".into(),
        protocol: ApiProtocol::InfuserRest,
        auth: AuthMethod::QueryParam {
            name: "serviceKey".into(),
        },
        extractor: ResponseExtractor {
            data_path: vec!["data".into()],
            error_check: ErrorCheck::HttpStatus,
            pagination: None,
            format: ResponseFormat::Json,
        },
        operations: vec![Operation {
            path: "/items".into(),
            method: HttpMethod::Get,
            summary: "아이템 조회".into(),
            content_type: ContentType::None,
            parameters: vec![Parameter {
                name: "page".into(),
                description: "페이지".into(),
                location: ParamLocation::Query,
                param_type: "integer".into(),
                required: false,
                default: Some("1".into()),
            }],
            request_body: None,
            response_fields: vec![],
        }],
        fetched_at: "2024-01-01".into(),
    }
}

#[test]
fn test_find_operation() {
    let spec = make_test_spec();
    let op = caller::find_operation(&spec, "/items").unwrap();
    assert_eq!(op.summary, "아이템 조회");
}

#[test]
fn test_find_operation_by_name() {
    let spec = make_test_spec();
    let op = caller::find_operation(&spec, "아이템 조회").unwrap();
    assert_eq!(op.path, "/items");
}

#[test]
fn test_find_operation_not_found() {
    let spec = make_test_spec();
    assert!(caller::find_operation(&spec, "/unknown").is_none());
}
