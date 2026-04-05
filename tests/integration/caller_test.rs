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
    // 실제 XML 구조 오류 — 태그 미스매치 (파서 에러 경로)
    let xml = "<a><b></a>";
    let result = parse_xml_body(xml);
    assert!(result.is_err(), "태그 미스매치는 에러여야 함");
    // 순수 텍스트 — 루트 없음 에러 경로
    let result2 = parse_xml_body("not xml at all");
    assert!(result2.is_err());
}

#[test]
fn test_parse_xml_cdata_preserved() {
    // Eval R1 B1: CDATA 이벤트가 text로 보존되는지 검증
    use korea_cli::core::caller::parse_xml_body;
    let xml = r#"<root><msg><![CDATA[hello world]]></msg></root>"#;
    let value = parse_xml_body(xml).unwrap();
    let msg = find_by_key(&value, "msg").expect("msg 없음");
    assert_eq!(msg.as_str(), Some("hello world"));
}

#[test]
fn test_parse_xml_mixed_content() {
    // Eval R2 W-R2-1: text와 children이 공존하는 mixed content → $text 키로 보존
    use korea_cli::core::caller::parse_xml_body;
    // trim_text(true)가 whitespace-only text를 제거하므로, 실제 text가 있어야 보존됨
    let xml = r#"<root>direct text<child>val</child></root>"#;
    let value = parse_xml_body(xml).unwrap();
    let root = value.get("root").expect("root 없음");
    let root_obj = root.as_object().expect("root는 Object여야 함");
    assert_eq!(
        root_obj.get("$text").and_then(|v| v.as_str()),
        Some("direct text"),
        "$text 키에 direct text 보존"
    );
    assert_eq!(
        root_obj.get("child").and_then(|v| v.as_str()),
        Some("val"),
        "child element도 함께 보존"
    );
}

#[test]
fn test_parse_xml_self_closing_root() {
    // Eval R1 W1: 루트 레벨 self-closing 태그를 에러로 처리하지 않음
    use korea_cli::core::caller::parse_xml_body;
    let xml = r#"<response/>"#;
    let value = parse_xml_body(xml).expect("self-closing root는 파싱 성공해야 함");
    // {"response": null} 형태
    assert!(value.get("response").is_some());
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
