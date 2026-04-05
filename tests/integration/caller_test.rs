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
        missing_operations: vec![],
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

#[test]
fn test_parse_xml_flat_tags() {
    use korea_cli::core::caller::parse_xml_body;
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<response>
  <header>
    <resultCode>00</resultCode>
    <resultMsg>NORMAL SERVICE.</resultMsg>
  </header>
  <body>
    <items>
      <item><name>test</name></item>
    </items>
  </body>
</response>"#;
    let result = parse_xml_body(xml);
    assert!(result.is_ok(), "파싱 결과: {:?}", result);
    let value = result.unwrap();
    // resultCode는 단순 문자열로 나타나야 함 (quick-xml serde의 $text 래퍼 없이)
    let code = find_by_key(&value, "resultCode").expect("resultCode 없음");
    assert_eq!(
        code.as_str(),
        Some("00"),
        "resultCode 직접 매칭: {:?}",
        code
    );
}

#[test]
fn test_parse_xml_malformed() {
    use korea_cli::core::caller::parse_xml_body;
    let xml = "not xml at all";
    let result = parse_xml_body(xml);
    assert!(result.is_err());
}

#[test]
fn test_parse_xml_auth_error_tags() {
    // data.go.kr 인증 실패 응답의 returnAuthMsg 태그 탐색 가능
    use korea_cli::core::caller::parse_xml_body;
    let xml = r#"<OpenAPI_ServiceResponse><cmmMsgHeader>
        <errMsg>SERVICE ERROR</errMsg>
        <returnReasonCode>12</returnReasonCode>
        <returnAuthMsg>SERVICE_KEY_IS_NOT_REGISTERED_ERROR</returnAuthMsg>
    </cmmMsgHeader></OpenAPI_ServiceResponse>"#;
    let value = parse_xml_body(xml).unwrap();
    let msg = find_by_key(&value, "returnAuthMsg").expect("returnAuthMsg 없음");
    assert_eq!(msg.as_str(), Some("SERVICE_KEY_IS_NOT_REGISTERED_ERROR"));
}

/// Helper: serde_json::Value 안에서 key 이름으로 값 재귀 탐색
fn find_by_key<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(x) = m.get(key) {
                return Some(x);
            }
            m.values().find_map(|x| find_by_key(x, key))
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_by_key(x, key)),
        _ => None,
    }
}
