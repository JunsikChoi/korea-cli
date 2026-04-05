//! Gateway AJAX 추출 API의 실제 호출 가능성 E2E 스모크 테스트.
//!
//! 실행: cargo test --test e2e_gateway_smoke -- --ignored --nocapture
//!
//! 필수 환경변수: DATA_GO_KR_API_KEY
//! 각 list_id는 data.go.kr에서 이용신청 승인 필요.

use korea_cli::core::bundle;
use korea_cli::core::caller::call_api;
use korea_cli::core::types::{ApiProtocol, ApiSpec};

/// 테스트 대상 5개 API (list_id)
const TARGETS: &[(&str, &str)] = &[
    ("15059468", "기상청 중기예보"),
    ("15012690", "한국천문연구원 특일"),
    ("15073855", "한국환경공단 에어코리아"),
    ("15000415", "기상청 기상특보"),
    ("15134735", "국토교통부 건축HUB"),
];

/// data.go.kr 바디 에러 코드 중 test skip으로 분류할 것들
/// Round 1 B7: <returnAuthMsg> 태그에 들어가는 인증 관련 에러 포함
const SKIPPABLE_ERROR_CODES: &[&str] = &[
    "SERVICE_ACCESS_DENIED_ERROR",
    "SERVICE_KEY_IS_NOT_REGISTERED_ERROR",
    "TEMPORARILY_DISABLE_THE_SERVICEKEY_ERROR",
    "UNREGISTERED_IP_ERROR",
    "DEADLINE_HAS_EXPIRED_ERROR",
];

#[tokio::test]
#[ignore]
async fn e2e_gateway_smoke_available_operations() {
    let api_key = match std::env::var("DATA_GO_KR_API_KEY") {
        Ok(k) => k,
        Err(_) => {
            eprintln!("SKIP: DATA_GO_KR_API_KEY 환경변수 미설정");
            return;
        }
    };

    let bundle = bundle::load_bundle().expect("bundle load 실패");

    let mut pass = 0;
    let mut skip = 0;
    let mut fail = 0;

    for (list_id, name) in TARGETS {
        eprintln!("\n=== {} ({}) ===", list_id, name);
        let spec = match bundle.specs.get(*list_id) {
            Some(s) => s,
            None => {
                eprintln!("FAIL: bundle에 spec 없음");
                fail += 1;
                continue;
            }
        };

        // Gateway 경로 검증 (W-Back4)
        assert!(
            matches!(spec.protocol, ApiProtocol::DataGoKrRest),
            "{}의 protocol이 DataGoKrRest가 아님: {:?}",
            list_id,
            spec.protocol
        );

        // "호출 용이한" operation 선정: required 파라미터가 적은 것 우선
        let op = match pick_easy_operation(spec) {
            Some(op) => op,
            None => {
                eprintln!("SKIP: 호출 용이한 operation 없음");
                skip += 1;
                continue;
            }
        };
        eprintln!("선택 operation: {} ({})", op.path, op.summary);

        // 기본 파라미터 구성: 페이징 파라미터만 넣음
        let params = build_default_params(op);

        match call_api(spec, &op.path, &params, &api_key).await {
            Ok(resp) => {
                // resp.data는 이미 parse_xml_body로 파싱된 Value — Value에서 직접 key 탐색.
                // (Eval R1 W2: to_string() 후 substring 재파싱은 취약한 포맷 의존)
                let code = resp
                    .data
                    .as_ref()
                    .map(extract_result_code_from_value)
                    .unwrap_or_default();
                let body = resp
                    .data
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                if SKIPPABLE_ERROR_CODES.iter().any(|c| code.contains(c)) {
                    eprintln!("SKIP: {}", code);
                    skip += 1;
                } else if code.is_empty() || code == "00" || code == "0000" {
                    eprintln!("PASS: resultCode={}, body {} bytes", code, body.len());
                    pass += 1;
                } else {
                    eprintln!("=== FAIL: {}/{} ===", list_id, op.path);
                    eprintln!("URL: {}{}", spec.base_url, op.path);
                    eprintln!("Params: {:?}", params);
                    eprintln!("resultCode: {}", code);
                    eprintln!(
                        "Body (first 500): {}",
                        &body.chars().take(500).collect::<String>()
                    );
                    fail += 1;
                }
            }
            Err(e) => {
                eprintln!("=== FAIL: {}/{} ===", list_id, op.path);
                eprintln!("Error: {}", e);
                fail += 1;
            }
        }
    }

    eprintln!(
        "\n=== 결과: PASS {} / SKIP {} / FAIL {} ===",
        pass, skip, fail
    );
    assert_eq!(fail, 0, "{} API E2E 실패", fail);
}

fn pick_easy_operation(spec: &ApiSpec) -> Option<&korea_cli::core::types::Operation> {
    // required 파라미터가 가장 적은 operation 선택
    spec.operations
        .iter()
        .min_by_key(|op| op.parameters.iter().filter(|p| p.required).count())
}

fn build_default_params(op: &korea_cli::core::types::Operation) -> Vec<(String, String)> {
    // 페이징 파라미터 기본값 주입
    // Round 1 W8: _type=xml 파라미터는 일부 API에서 INVALID_REQUEST_PARAMETER_ERROR 유발 → 제거
    // Gateway API는 기본 XML 응답이므로 명시 불필요.
    let mut params = vec![
        ("pageNo".to_string(), "1".to_string()),
        ("numOfRows".to_string(), "1".to_string()),
    ];
    // required 파라미터에 default가 있으면 사용, 없으면 더미값 "20250101"
    for p in op.parameters.iter().filter(|p| p.required) {
        if params.iter().any(|(k, _)| k == &p.name) {
            continue;
        }
        let val = p.default.clone().unwrap_or_else(|| "20250101".to_string());
        params.push((p.name.clone(), val));
    }
    params
}

/// 응답 body에서 에러 코드 추출. data.go.kr의 두 가지 응답 구조 모두 커버:
/// 1. 정상: `<response><header><resultCode>XX</resultCode></header></response>`
/// 2. 인증오류: `<OpenAPI_ServiceResponse><cmmMsgHeader><returnAuthMsg>XXX_ERROR</returnAuthMsg></cmmMsgHeader></OpenAPI_ServiceResponse>`
///
/// Round 1 B7: `<returnAuthMsg>`를 우선순위 높게 탐색 — SKIPPABLE 매칭을 위해.
fn extract_result_code(body: &str) -> String {
    // 1. XML: <returnAuthMsg> (인증 에러 시 data.go.kr 표준)
    if let Some(code) = extract_tag(body, "returnAuthMsg") {
        if !code.is_empty() {
            return code;
        }
    }
    // 2. XML: <resultCode>
    if let Some(code) = extract_tag(body, "resultCode") {
        if !code.is_empty() {
            return code;
        }
    }
    // 3. JSON: "resultCode":"XX"
    if let Some(start) = body.find("\"resultCode\"") {
        let rest = &body[start..];
        if let Some(colon) = rest.find(':') {
            let after = &rest[colon + 1..];
            let trimmed = after.trim_start_matches(['"', ' ', '\t']);
            let end = trimmed.find(['"', ',', '}']).unwrap_or(trimmed.len());
            return trimmed[..end].trim().to_string();
        }
    }
    // 4. errMsg fallback
    extract_tag(body, "errMsg").unwrap_or_default()
}

fn extract_tag(body: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = body.find(&open)?;
    let rest = &body[start + open.len()..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim().to_string())
}

/// Value에서 직접 resultCode/returnAuthMsg 찾기 (Eval R1 W2).
/// parse_xml_body가 이미 Value로 변환한 결과에서 재귀 탐색.
/// 우선순위: returnAuthMsg → resultCode → errMsg.
fn extract_result_code_from_value(v: &serde_json::Value) -> String {
    if let Some(s) = find_key_recursive(v, "returnAuthMsg") {
        if !s.is_empty() {
            return s;
        }
    }
    if let Some(s) = find_key_recursive(v, "resultCode") {
        if !s.is_empty() {
            return s;
        }
    }
    find_key_recursive(v, "errMsg").unwrap_or_default()
}

fn find_key_recursive(v: &serde_json::Value, key: &str) -> Option<String> {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(x) = m.get(key) {
                return match x {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                };
            }
            m.values().find_map(|x| find_key_recursive(x, key))
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_key_recursive(x, key)),
        _ => None,
    }
}

#[test]
fn test_extract_result_code_xml() {
    let xml = "<response><header><resultCode>00</resultCode></header></response>";
    assert_eq!(extract_result_code(xml), "00");
}

#[test]
fn test_extract_result_code_return_auth_msg() {
    // Round 1 B7: returnAuthMsg가 우선순위 높음
    let xml = "<OpenAPI_ServiceResponse><cmmMsgHeader><errMsg>SERVICE ERROR</errMsg><returnReasonCode>12</returnReasonCode><returnAuthMsg>SERVICE_KEY_IS_NOT_REGISTERED_ERROR</returnAuthMsg></cmmMsgHeader></OpenAPI_ServiceResponse>";
    let code = extract_result_code(xml);
    assert_eq!(code, "SERVICE_KEY_IS_NOT_REGISTERED_ERROR");
    // SKIPPABLE 매칭 확인
    assert!(SKIPPABLE_ERROR_CODES.iter().any(|c| code.contains(c)));
}

#[test]
fn test_extract_result_code_errmsg_fallback() {
    let xml = "<root><errMsg>fallback error</errMsg></root>";
    assert_eq!(extract_result_code(xml), "fallback error");
}

#[test]
fn test_extract_result_code_from_value_nested() {
    // parse_xml_body 결과 Value에서 중첩된 resultCode 재귀 탐색
    let v = serde_json::json!({
        "response": {
            "header": {
                "resultCode": "00",
                "resultMsg": "NORMAL SERVICE."
            }
        }
    });
    assert_eq!(extract_result_code_from_value(&v), "00");
}

#[test]
fn test_extract_result_code_from_value_auth_error_priority() {
    // returnAuthMsg가 resultCode보다 우선순위 높음
    let v = serde_json::json!({
        "OpenAPI_ServiceResponse": {
            "cmmMsgHeader": {
                "errMsg": "SERVICE ERROR",
                "returnReasonCode": "12",
                "returnAuthMsg": "SERVICE_KEY_IS_NOT_REGISTERED_ERROR"
            }
        }
    });
    assert_eq!(
        extract_result_code_from_value(&v),
        "SERVICE_KEY_IS_NOT_REGISTERED_ERROR"
    );
}
