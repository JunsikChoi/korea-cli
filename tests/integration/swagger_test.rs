#[test]
fn test_parse_swagger_spec() {
    let spec_json = serde_json::json!({
        "swagger": "2.0",
        "info": { "title": "사업자등록정보 서비스" },
        "host": "api.odcloud.kr",
        "basePath": "/api/nts-businessman/v1",
        "schemes": ["https"],
        "securityDefinitions": {
            "api_key": { "type": "apiKey", "in": "query", "name": "serviceKey" },
            "api_key_header": { "type": "apiKey", "in": "header", "name": "Authorization" }
        },
        "paths": {
            "/status": {
                "post": {
                    "summary": "상태조회",
                    "consumes": ["application/json"],
                    "produces": ["application/json"],
                    "parameters": [
                        {
                            "name": "body",
                            "in": "body",
                            "required": true,
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "b_no": {
                                        "type": "array",
                                        "items": { "type": "string" },
                                        "description": "사업자등록번호 배열"
                                    }
                                }
                            }
                        }
                    ]
                }
            }
        }
    });

    let api_spec = korea_cli::core::swagger::parse_swagger("15081808", &spec_json).unwrap();
    assert_eq!(api_spec.list_id, "15081808");
    assert_eq!(
        api_spec.base_url,
        "https://api.odcloud.kr/api/nts-businessman/v1"
    );
    assert_eq!(api_spec.operations.len(), 1);
    let op = &api_spec.operations[0];
    assert_eq!(op.path, "/status");
    assert!(matches!(
        op.method,
        korea_cli::core::types::HttpMethod::Post
    ));
    assert_eq!(op.summary, "상태조회");
    let body = op.request_body.as_ref().unwrap();
    assert_eq!(body.fields.len(), 1);
    assert_eq!(body.fields[0].name, "b_no");
}
