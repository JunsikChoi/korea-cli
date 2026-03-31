//! Swagger spec fetching, parsing, and caching.

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;

use crate::config::paths;
use crate::core::types::*;

/// Parse a Swagger 2.0 JSON spec into our normalized `ApiSpec`.
pub fn parse_swagger(list_id: &str, spec: &Value) -> Result<ApiSpec> {
    let host = spec["host"].as_str().unwrap_or_default();
    let base_path = spec["basePath"].as_str().unwrap_or("");
    let scheme = spec["schemes"]
        .as_array()
        .and_then(|s| s.first())
        .and_then(|v| v.as_str())
        .unwrap_or("https");

    let base_url = format!("{scheme}://{host}{base_path}");

    let protocol = if host.contains("api.odcloud.kr") {
        ApiProtocol::InfuserRest
    } else if host.contains("apis.data.go.kr") {
        ApiProtocol::DataGoKrRest
    } else {
        ApiProtocol::ExternalRest
    };

    let auth = parse_auth(spec);

    let mut operations = Vec::new();
    if let Some(paths) = spec["paths"].as_object() {
        for (path, methods) in paths {
            if let Some(methods_obj) = methods.as_object() {
                for (method_str, details) in methods_obj {
                    let method = match method_str.to_lowercase().as_str() {
                        "get" => HttpMethod::Get,
                        "post" => HttpMethod::Post,
                        "put" => HttpMethod::Put,
                        "delete" => HttpMethod::Delete,
                        _ => continue,
                    };

                    let summary = details["summary"].as_str().unwrap_or_default().to_string();

                    let content_type = details["consumes"]
                        .as_array()
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                        .map(|ct| match ct {
                            "application/json" => ContentType::Json,
                            "application/xml" | "text/xml" => ContentType::Xml,
                            "application/x-www-form-urlencoded" => ContentType::FormUrlEncoded,
                            _ => ContentType::Json,
                        })
                        .unwrap_or(ContentType::None);

                    let (parameters, request_body) = parse_parameters(details);
                    let response_fields = parse_response_fields(details);

                    operations.push(Operation {
                        path: path.clone(),
                        method,
                        summary,
                        content_type,
                        parameters,
                        request_body,
                        response_fields,
                    });
                }
            }
        }
    }

    // Determine response format from first operation's produces, or default JSON
    let format = spec["paths"]
        .as_object()
        .and_then(|paths| paths.values().next())
        .and_then(|m| m.as_object())
        .and_then(|methods| methods.values().next())
        .and_then(|details| details["produces"].as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|p| {
            if p.contains("xml") {
                ResponseFormat::Xml
            } else {
                ResponseFormat::Json
            }
        })
        .unwrap_or(ResponseFormat::Json);

    let extractor = ResponseExtractor {
        data_path: vec![],
        error_check: ErrorCheck::HttpStatus,
        pagination: None,
        format,
    };

    let fetched_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    Ok(ApiSpec {
        list_id: list_id.to_string(),
        base_url,
        protocol,
        auth,
        extractor,
        operations,
        fetched_at,
    })
}

/// Extract auth method from securityDefinitions.
fn parse_auth(spec: &Value) -> AuthMethod {
    let sec_defs = &spec["securityDefinitions"];
    if sec_defs.is_null() || !sec_defs.is_object() {
        return AuthMethod::None;
    }

    let defs = match sec_defs.as_object() {
        Some(d) => d,
        None => return AuthMethod::None,
    };

    let mut has_query = false;
    let mut query_name = String::new();
    let mut has_header = false;
    let mut header_name = String::new();

    for (_key, def) in defs {
        let location = def["in"].as_str().unwrap_or_default();
        let name = def["name"].as_str().unwrap_or_default();
        match location {
            "query" => {
                has_query = true;
                query_name = name.to_string();
            }
            "header" => {
                has_header = true;
                header_name = name.to_string();
            }
            _ => {}
        }
    }

    match (has_query, has_header) {
        (true, true) => AuthMethod::Both {
            query: query_name,
            header_name,
            header_prefix: "Infuser ".to_string(),
            prefer: AuthPreference::Query,
        },
        (true, false) => AuthMethod::QueryParam { name: query_name },
        (false, true) => AuthMethod::Header {
            name: header_name,
            prefix: "Infuser ".to_string(),
        },
        (false, false) => AuthMethod::None,
    }
}

/// Parse operation parameters. Skip serviceKey/Authorization params.
/// "in":"body" params are extracted into RequestBody fields.
fn parse_parameters(details: &Value) -> (Vec<Parameter>, Option<RequestBody>) {
    let params_array = match details["parameters"].as_array() {
        Some(arr) => arr,
        None => return (vec![], None),
    };

    let mut parameters = Vec::new();
    let mut request_body: Option<RequestBody> = None;

    for param in params_array {
        let name = param["name"].as_str().unwrap_or_default();
        let location = param["in"].as_str().unwrap_or_default();

        // Skip auth-related parameters
        if name == "serviceKey" || name == "Authorization" {
            continue;
        }

        if location == "body" {
            let fields = extract_body_fields(param);
            let content_type = ContentType::Json;
            request_body = Some(RequestBody {
                content_type,
                fields,
            });
            continue;
        }

        let param_location = match location {
            "query" => ParamLocation::Query,
            "path" => ParamLocation::Path,
            "header" => ParamLocation::Header,
            _ => ParamLocation::Query,
        };

        let param_type = format_param_type(param);

        parameters.push(Parameter {
            name: name.to_string(),
            description: param["description"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            location: param_location,
            param_type,
            required: param["required"].as_bool().unwrap_or(false),
            default: param["default"].as_str().map(|s| s.to_string()),
        });
    }

    (parameters, request_body)
}

/// Format parameter type string, handling arrays as "array(item_type)".
fn format_param_type(param: &Value) -> String {
    let base_type = param["type"].as_str().unwrap_or("string");
    if base_type == "array" {
        let item_type = param["items"]["type"].as_str().unwrap_or("string");
        format!("array({item_type})")
    } else {
        base_type.to_string()
    }
}

/// Extract body schema properties as a list of Parameters.
fn extract_body_fields(param: &Value) -> Vec<Parameter> {
    let properties = &param["schema"]["properties"];
    let required_fields = param["schema"]["required"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let props = match properties.as_object() {
        Some(p) => p,
        None => return vec![],
    };

    props
        .iter()
        .map(|(name, prop)| {
            let param_type = if prop["type"].as_str() == Some("array") {
                let item_type = prop["items"]["type"].as_str().unwrap_or("string");
                format!("array({item_type})")
            } else {
                prop["type"].as_str().unwrap_or("string").to_string()
            };

            Parameter {
                name: name.clone(),
                description: prop["description"].as_str().unwrap_or_default().to_string(),
                location: ParamLocation::Body,
                param_type,
                required: required_fields.contains(name),
                default: None,
            }
        })
        .collect()
}

/// Extract 200 response schema properties as ResponseField list.
fn parse_response_fields(details: &Value) -> Vec<ResponseField> {
    let response_200 = &details["responses"]["200"];
    if response_200.is_null() {
        return vec![];
    }

    let properties = &response_200["schema"]["properties"];
    let props = match properties.as_object() {
        Some(p) => p,
        None => return vec![],
    };

    props
        .iter()
        .map(|(name, prop)| {
            let field_type = if prop["type"].as_str() == Some("array") {
                let item_type = prop["items"]["type"].as_str().unwrap_or("object");
                format!("array({item_type})")
            } else {
                prop["type"].as_str().unwrap_or("object").to_string()
            };

            ResponseField {
                name: name.clone(),
                description: prop["description"].as_str().unwrap_or_default().to_string(),
                field_type,
            }
        })
        .collect()
}

/// Extract swaggerUrl from the data.go.kr openapi page HTML.
fn extract_swagger_url(html: &str) -> Option<String> {
    let re = Regex::new(r"var\s+swaggerUrl\s*=\s*'([^']+)'").ok()?;
    re.captures(html)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Fetch Swagger spec from data.go.kr, parse, and cache locally.
/// If a cached spec exists, returns that instead of re-fetching.
pub async fn fetch_and_cache_spec(list_id: &str) -> Result<ApiSpec> {
    // Check cache first
    if let Some(cached) = load_cached_spec(list_id)? {
        return Ok(cached);
    }

    // Scrape the openapi page for swaggerUrl
    let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");
    let client = reqwest::Client::builder()
        .user_agent("korea-cli/0.1.0")
        .build()?;
    let html = client
        .get(&page_url)
        .send()
        .await
        .context("Failed to fetch openapi page")?
        .text()
        .await
        .context("Failed to read openapi page body")?;

    let swagger_url =
        extract_swagger_url(&html).context("Could not find swaggerUrl in openapi page")?;

    // Fetch the actual Swagger JSON
    let spec_json: Value = client
        .get(&swagger_url)
        .send()
        .await
        .context("Failed to fetch Swagger spec")?
        .json()
        .await
        .context("Failed to parse Swagger spec JSON")?;

    // Parse into ApiSpec
    let api_spec = parse_swagger(list_id, &spec_json)?;

    // Cache to disk
    let cache_path = paths::spec_cache_file(list_id)?;
    let serialized = serde_json::to_string_pretty(&api_spec)?;
    std::fs::write(&cache_path, serialized)?;

    Ok(api_spec)
}

/// Load a cached spec from disk, if it exists.
pub fn load_cached_spec(list_id: &str) -> Result<Option<ApiSpec>> {
    let cache_path = paths::spec_cache_file(list_id)?;
    if cache_path.exists() {
        let content = std::fs::read_to_string(&cache_path)?;
        let spec: ApiSpec = serde_json::from_str(&content)?;
        Ok(Some(spec))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_swagger_url() {
        let html = r#"
            var swaggerUrl = 'https://api.odcloud.kr/spec/15081808';
            some other content
        "#;
        assert_eq!(
            extract_swagger_url(html),
            Some("https://api.odcloud.kr/spec/15081808".to_string())
        );
    }

    #[test]
    fn test_extract_swagger_url_no_match() {
        let html = "no swagger url here";
        assert_eq!(extract_swagger_url(html), None);
    }

    #[test]
    fn test_parse_auth_both() {
        let spec = serde_json::json!({
            "securityDefinitions": {
                "key1": { "type": "apiKey", "in": "query", "name": "serviceKey" },
                "key2": { "type": "apiKey", "in": "header", "name": "Authorization" }
            }
        });
        let auth = parse_auth(&spec);
        assert!(matches!(auth, AuthMethod::Both { .. }));
    }

    #[test]
    fn test_parse_auth_query_only() {
        let spec = serde_json::json!({
            "securityDefinitions": {
                "key1": { "type": "apiKey", "in": "query", "name": "serviceKey" }
            }
        });
        let auth = parse_auth(&spec);
        assert!(matches!(auth, AuthMethod::QueryParam { .. }));
    }

    #[test]
    fn test_parse_auth_header_only() {
        let spec = serde_json::json!({
            "securityDefinitions": {
                "key1": { "type": "apiKey", "in": "header", "name": "Authorization" }
            }
        });
        let auth = parse_auth(&spec);
        assert!(matches!(auth, AuthMethod::Header { .. }));
    }

    #[test]
    fn test_parse_auth_none() {
        let spec = serde_json::json!({});
        let auth = parse_auth(&spec);
        assert!(matches!(auth, AuthMethod::None));
    }

    #[test]
    fn test_parse_parameters_skips_service_key() {
        let details = serde_json::json!({
            "parameters": [
                { "name": "serviceKey", "in": "query", "type": "string" },
                { "name": "Authorization", "in": "header", "type": "string" },
                { "name": "pageNo", "in": "query", "type": "integer", "required": false, "description": "페이지번호" }
            ]
        });
        let (params, body) = parse_parameters(&details);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "pageNo");
        assert!(body.is_none());
    }

    #[test]
    fn test_parse_parameters_body() {
        let details = serde_json::json!({
            "parameters": [
                {
                    "name": "body",
                    "in": "body",
                    "schema": {
                        "type": "object",
                        "properties": {
                            "b_no": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "사업자번호"
                            }
                        }
                    }
                }
            ]
        });
        let (params, body) = parse_parameters(&details);
        assert!(params.is_empty());
        let body = body.unwrap();
        assert_eq!(body.fields.len(), 1);
        assert_eq!(body.fields[0].name, "b_no");
        assert_eq!(body.fields[0].param_type, "array(string)");
    }

    #[test]
    fn test_parse_response_fields() {
        let details = serde_json::json!({
            "responses": {
                "200": {
                    "schema": {
                        "type": "object",
                        "properties": {
                            "status_code": { "type": "string", "description": "상태코드" },
                            "data": { "type": "array", "items": { "type": "object" }, "description": "결과" }
                        }
                    }
                }
            }
        });
        let fields = parse_response_fields(&details);
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn test_parse_swagger_full() {
        let spec_json = serde_json::json!({
            "swagger": "2.0",
            "info": { "title": "Test API" },
            "host": "api.odcloud.kr",
            "basePath": "/api/test/v1",
            "schemes": ["https"],
            "securityDefinitions": {
                "api_key": { "type": "apiKey", "in": "query", "name": "serviceKey" }
            },
            "paths": {
                "/items": {
                    "get": {
                        "summary": "목록 조회",
                        "produces": ["application/json"],
                        "parameters": [
                            { "name": "page", "in": "query", "type": "integer", "required": false, "description": "페이지" }
                        ],
                        "responses": {
                            "200": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "data": { "type": "array", "items": { "type": "object" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let api_spec = parse_swagger("12345", &spec_json).unwrap();
        assert_eq!(api_spec.list_id, "12345");
        assert_eq!(api_spec.base_url, "https://api.odcloud.kr/api/test/v1");
        assert!(matches!(api_spec.protocol, ApiProtocol::InfuserRest));
        assert!(matches!(api_spec.auth, AuthMethod::QueryParam { .. }));
        assert_eq!(api_spec.operations.len(), 1);
        assert_eq!(api_spec.operations[0].path, "/items");
        assert!(matches!(api_spec.operations[0].method, HttpMethod::Get));
        assert_eq!(api_spec.operations[0].parameters.len(), 1);
        assert_eq!(api_spec.operations[0].response_fields.len(), 1);
    }
}
