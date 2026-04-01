//! HTML table parser for data.go.kr Gateway API pages.
//!
//! Parses the openapi.do page and selectApiDetailFunction.do AJAX responses
//! to extract API spec information from HTML tables.

use anyhow::{Context, Result};
use scraper::{Html, Selector};

use crate::core::types::*;

/// Extracted operation metadata from openapi.do page.
#[derive(Debug, Clone)]
pub struct PageInfo {
    pub public_data_detail_pk: String,
    pub operations: Vec<OperationOption>,
}

/// A single operation option from the <select> dropdown.
#[derive(Debug, Clone)]
pub struct OperationOption {
    pub seq_no: String,
    pub name: String,
}

/// Parsed operation detail from selectApiDetailFunction.do response.
#[derive(Debug, Clone)]
pub struct ParsedOperation {
    pub request_url: String,
    pub service_url: String,
    pub parameters: Vec<Parameter>,
    pub response_fields: Vec<ResponseField>,
}

/// Extract publicDataDetailPk and operation list from openapi.do HTML.
pub fn parse_openapi_page(html: &str) -> Result<PageInfo> {
    let document = Html::parse_document(html);

    // Extract publicDataDetailPk from hidden input or JavaScript
    let pk = extract_public_data_detail_pk(&document, html)
        .context("publicDataDetailPk를 찾을 수 없습니다")?;

    // Extract operation list from <select id="open_api_detail_select">
    let operations = extract_operation_options(&document);

    Ok(PageInfo {
        public_data_detail_pk: pk,
        operations,
    })
}

/// Parse an operation detail HTML (from selectApiDetailFunction.do response).
pub fn parse_operation_detail(html: &str) -> Result<ParsedOperation> {
    let document = Html::parse_fragment(html);

    let request_url = extract_labeled_url(&document, "요청주소").unwrap_or_default();
    let service_url = extract_labeled_url(&document, "서비스URL").unwrap_or_default();
    let parameters = extract_request_params(&document);
    let response_fields = extract_response_fields(&document);

    Ok(ParsedOperation {
        request_url,
        service_url,
        parameters,
        response_fields,
    })
}

/// Build an ApiSpec from parsed operation details.
pub fn build_api_spec(list_id: &str, parsed_ops: &[ParsedOperation]) -> Option<ApiSpec> {
    if parsed_ops.is_empty() {
        return None;
    }

    // Use the first operation's service_url as base_url
    let base_url = parsed_ops
        .iter()
        .find(|op| !op.service_url.is_empty())
        .map(|op| op.service_url.clone())
        .unwrap_or_default();

    // Detect auth method from parameters
    let auth = parsed_ops
        .iter()
        .flat_map(|op| op.parameters.iter())
        .find(|p| {
            p.name.eq_ignore_ascii_case("serviceKey") || p.name.eq_ignore_ascii_case("ServiceKey")
        })
        .map(|_| AuthMethod::QueryParam {
            name: "serviceKey".to_string(),
        })
        .unwrap_or(AuthMethod::None);

    let operations: Vec<Operation> = parsed_ops
        .iter()
        .filter_map(|op| {
            let path = if !op.request_url.is_empty() && !base_url.is_empty() {
                op.request_url
                    .strip_prefix(&base_url)
                    .unwrap_or(&op.request_url)
                    .to_string()
            } else {
                op.request_url.clone()
            };

            if path.is_empty() && op.request_url.is_empty() {
                return None;
            }

            // Filter out serviceKey from user-visible parameters
            let params: Vec<Parameter> = op
                .parameters
                .iter()
                .filter(|p| !p.name.eq_ignore_ascii_case("serviceKey"))
                .cloned()
                .collect();

            Some(Operation {
                path: if path.is_empty() {
                    "/".to_string()
                } else {
                    path
                },
                method: HttpMethod::Get,
                summary: String::new(),
                content_type: ContentType::None,
                parameters: params,
                request_body: None,
                response_fields: op.response_fields.clone(),
            })
        })
        .collect();

    if operations.is_empty() {
        return None;
    }

    let fetched_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    Some(ApiSpec {
        list_id: list_id.to_string(),
        base_url,
        protocol: ApiProtocol::DataGoKrRest,
        auth,
        extractor: ResponseExtractor {
            data_path: vec![],
            error_check: ErrorCheck::HttpStatus,
            pagination: None,
            format: ResponseFormat::Xml,
        },
        operations,
        fetched_at,
    })
}

// ── Internal helpers ──

fn extract_public_data_detail_pk(document: &Html, raw_html: &str) -> Option<String> {
    // 1) id= 셀렉터 (실제 data.go.kr 구조)
    if let Ok(sel) = Selector::parse(r#"input#publicDataDetailPk"#) {
        if let Some(el) = document.select(&sel).next() {
            if let Some(val) = el.value().attr("value") {
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }

    // 2) name= 셀렉터 (하위 호환)
    if let Ok(sel) = Selector::parse(r#"input[name="publicDataDetailPk"]"#) {
        if let Some(el) = document.select(&sel).next() {
            if let Some(val) = el.value().attr("value") {
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }

    // 3) regex fallback (id= 또는 name= 모두 매칭)
    let re = regex::Regex::new(
        r#"(?s)(?:name|id)\s*=\s*["']?publicDataDetailPk["']?\s+value\s*=\s*["']([^"']+)["']"#,
    )
    .ok()?;
    re.captures(raw_html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_operation_options(document: &Html) -> Vec<OperationOption> {
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
            Some(OperationOption { seq_no: seq, name })
        })
        .collect()
}

fn extract_labeled_url(document: &Html, label: &str) -> Option<String> {
    let sel = Selector::parse("strong").ok()?;
    for el in document.select(&sel) {
        let text = el.text().collect::<String>();
        if text.contains(label) {
            // The URL is typically in a sibling or nearby text node
            if let Some(parent) = el.parent() {
                let parent_text: String = parent
                    .children()
                    .filter_map(|child| child.value().as_text().map(|t| t.text.trim().to_string()))
                    .collect::<Vec<_>>()
                    .join("");
                // Find URL pattern in parent text
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

fn extract_request_params(document: &Html) -> Vec<Parameter> {
    // Look for request parameter table rows
    let row_sel = match Selector::parse("tr[data-paramtr-nm]") {
        Ok(s) => s,
        Err(_) => return extract_request_params_fallback(document),
    };

    let rows: Vec<_> = document.select(&row_sel).collect();
    if rows.is_empty() {
        return extract_request_params_fallback(document);
    }

    rows.iter()
        .filter_map(|row| {
            let name = row
                .value()
                .attr("data-paramtr-nm")
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                return None;
            }

            let required = row
                .value()
                .attr("data-paramtr-division")
                .map(|d| d.starts_with("필"))
                .unwrap_or(false);

            let description = row
                .value()
                .attr("data-paramtr-dc")
                .unwrap_or_default()
                .to_string();

            Some(Parameter {
                name,
                description,
                location: ParamLocation::Query,
                param_type: "string".to_string(),
                required,
                default: None,
            })
        })
        .collect()
}

fn extract_request_params_fallback(document: &Html) -> Vec<Parameter> {
    // Fallback: parse <td> cells in request parameter table
    let td_sel = match Selector::parse("td") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let tr_sel = match Selector::parse("tr") {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut params = Vec::new();
    for row in document.select(&tr_sel) {
        let cells: Vec<String> = row
            .select(&td_sel)
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();

        // Expected columns: 순번, 항목명, 타입, 항목크기, 항목구분, 항목설명
        if cells.len() >= 6 {
            let name = &cells[1];
            if name.is_empty() || name == "항목명(영문)" || name == "항목명" {
                continue;
            }
            let required = cells[4].starts_with("필");
            let description = cells[5].clone();

            params.push(Parameter {
                name: name.clone(),
                description,
                location: ParamLocation::Query,
                param_type: "string".to_string(),
                required,
                default: None,
            });
        }
    }
    params
}

fn extract_response_fields(document: &Html) -> Vec<ResponseField> {
    // Look for output result table — typically the second table or one after "출력결과"
    let td_sel = match Selector::parse("td") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let tr_sel = match Selector::parse("tr") {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut fields = Vec::new();
    let mut in_response_section = false;

    for row in document.select(&tr_sel) {
        let cells: Vec<String> = row
            .select(&td_sel)
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();

        // Detect response section by header row or th content
        let row_text = row.text().collect::<String>();
        if row_text.contains("출력결과") || row_text.contains("응답메시지") {
            in_response_section = true;
            continue;
        }

        if in_response_section && cells.len() >= 3 {
            let name = &cells[1];
            if name.is_empty() || name == "항목명(영문)" || name == "항목명" {
                continue;
            }
            let description = if cells.len() >= 6 {
                cells[5].clone()
            } else {
                cells.last().cloned().unwrap_or_default()
            };

            fields.push(ResponseField {
                name: name.clone(),
                description,
                field_type: "string".to_string(),
            });
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openapi_page_extracts_pk() {
        let html = r#"
        <html>
        <body>
            <input type="hidden" name="publicDataDetailPk" value="uddi:12345-abcde">
            <select id="open_api_detail_select">
                <option value="">선택</option>
                <option value="1001">getWeather</option>
                <option value="1002">getForecast</option>
            </select>
        </body>
        </html>
        "#;

        let info = parse_openapi_page(html).unwrap();
        assert_eq!(info.public_data_detail_pk, "uddi:12345-abcde");
        assert_eq!(info.operations.len(), 2);
        assert_eq!(info.operations[0].seq_no, "1001");
        assert_eq!(info.operations[0].name, "getWeather");
        assert_eq!(info.operations[1].seq_no, "1002");
    }

    #[test]
    fn test_parse_openapi_page_regex_fallback() {
        // Some pages have multiline hidden inputs
        let html = r#"
        <html><body>
            <input type="hidden"
                name="publicDataDetailPk"
                value="uddi:multiline-pk">
        </body></html>
        "#;

        let info = parse_openapi_page(html).unwrap();
        assert_eq!(info.public_data_detail_pk, "uddi:multiline-pk");
    }

    #[test]
    fn test_parse_operation_detail_data_attributes() {
        let html = r#"
        <div>
            <p><strong>요청주소</strong> https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0/getUltraSrtNcst</p>
            <p><strong>서비스URL</strong> https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0</p>
            <table>
                <tr data-paramtr-nm="serviceKey" data-paramtr-division="필수" data-paramtr-dc="인증키">
                    <td>1</td><td>serviceKey</td><td>string</td><td>100</td><td>필수</td><td>인증키</td>
                </tr>
                <tr data-paramtr-nm="numOfRows" data-paramtr-division="옵션" data-paramtr-dc="한 페이지 결과 수">
                    <td>2</td><td>numOfRows</td><td>string</td><td>10</td><td>옵션</td><td>한 페이지 결과 수</td>
                </tr>
                <tr data-paramtr-nm="base_date" data-paramtr-division="필수" data-paramtr-dc="발표일자">
                    <td>3</td><td>base_date</td><td>string</td><td>8</td><td>필수</td><td>발표일자</td>
                </tr>
            </table>
        </div>
        "#;

        let op = parse_operation_detail(html).unwrap();
        assert_eq!(
            op.request_url,
            "https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0/getUltraSrtNcst"
        );
        assert_eq!(
            op.service_url,
            "https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0"
        );
        assert_eq!(op.parameters.len(), 3);
        assert_eq!(op.parameters[0].name, "serviceKey");
        assert!(op.parameters[0].required);
        assert_eq!(op.parameters[1].name, "numOfRows");
        assert!(!op.parameters[1].required);
        assert_eq!(op.parameters[2].name, "base_date");
        assert!(op.parameters[2].required);
    }

    #[test]
    fn test_parse_operation_detail_fallback_td() {
        let html = r#"
        <div>
            <table>
                <tr><td>순번</td><td>항목명(영문)</td><td>타입</td><td>크기</td><td>항목구분</td><td>설명</td></tr>
                <tr><td>1</td><td>serviceKey</td><td>string</td><td>100</td><td>필수</td><td>인증키</td></tr>
                <tr><td>2</td><td>pageNo</td><td>string</td><td>10</td><td>옵션</td><td>페이지번호</td></tr>
            </table>
        </div>
        "#;

        let op = parse_operation_detail(html).unwrap();
        assert_eq!(op.parameters.len(), 2);
        assert_eq!(op.parameters[0].name, "serviceKey");
        assert!(op.parameters[0].required);
        assert_eq!(op.parameters[1].name, "pageNo");
        assert!(!op.parameters[1].required);
    }

    #[test]
    fn test_build_api_spec_filters_service_key() {
        let ops = vec![ParsedOperation {
            request_url: "https://apis.data.go.kr/test/getItems".into(),
            service_url: "https://apis.data.go.kr/test".into(),
            parameters: vec![
                Parameter {
                    name: "serviceKey".into(),
                    description: "인증키".into(),
                    location: ParamLocation::Query,
                    param_type: "string".into(),
                    required: true,
                    default: None,
                },
                Parameter {
                    name: "pageNo".into(),
                    description: "페이지번호".into(),
                    location: ParamLocation::Query,
                    param_type: "string".into(),
                    required: false,
                    default: None,
                },
            ],
            response_fields: vec![ResponseField {
                name: "resultCode".into(),
                description: "결과코드".into(),
                field_type: "string".into(),
            }],
        }];

        let spec = build_api_spec("15084084", &ops).unwrap();
        assert_eq!(spec.list_id, "15084084");
        assert_eq!(spec.base_url, "https://apis.data.go.kr/test");
        assert!(matches!(spec.auth, AuthMethod::QueryParam { .. }));
        assert_eq!(spec.operations.len(), 1);
        assert_eq!(spec.operations[0].path, "/getItems");
        // serviceKey should be filtered from user-visible params
        assert_eq!(spec.operations[0].parameters.len(), 1);
        assert_eq!(spec.operations[0].parameters[0].name, "pageNo");
        assert_eq!(spec.operations[0].response_fields.len(), 1);
    }

    #[test]
    fn test_parse_openapi_page_id_attribute() {
        // 실제 data.go.kr HTML은 id= 사용 (name= 아님)
        let html = r#"
        <html><body>
            <input type="hidden" id="publicDataDetailPk"
                   value="uddi:b295d381-f52d-4318-9191-96fe1fafff1f"/>
            <input type="hidden" id="publicDataPk" value="15061357"/>
            <select id="open_api_detail_select">
                <option value="25356">선물사일반현황조회</option>
                <option value="25357">선물사재무현황조회</option>
            </select>
        </body></html>
        "#;

        let info = parse_openapi_page(html).unwrap();
        assert_eq!(
            info.public_data_detail_pk,
            "uddi:b295d381-f52d-4318-9191-96fe1fafff1f"
        );
        assert_eq!(info.operations.len(), 2);
        assert_eq!(info.operations[0].seq_no, "25356");
        assert_eq!(info.operations[0].name, "선물사일반현황조회");
    }

    #[test]
    fn test_build_api_spec_empty_returns_none() {
        assert!(build_api_spec("12345", &[]).is_none());
    }
}
