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
    pub public_data_pk: Option<String>,
    pub ty_detail_code: Option<String>,
    pub operations: Vec<OperationOption>,
    /// External portal URL extracted from `a.link-api-btn[href]` (LINK API only).
    pub external_url: Option<String>,
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
    pub summary: String,
    pub parameters: Vec<Parameter>,
    pub response_fields: Vec<ResponseField>,
}

/// Extract publicDataDetailPk and operation list from openapi.do HTML.
pub fn parse_openapi_page(html: &str) -> Result<PageInfo> {
    let document = Html::parse_document(html);

    // Extract publicDataDetailPk from hidden input or JavaScript
    let pk = extract_public_data_detail_pk(&document, html)
        .context("publicDataDetailPk를 찾을 수 없습니다")?;
    let public_data_pk = extract_hidden_input_value(&document, "publicDataPk");
    let ty_detail_code = extract_ty_detail_code(html);

    // Extract operation list from <select id="open_api_detail_select">
    let operations = extract_operation_options(&document);

    // Extract external portal URL from a.link-api-btn[href]
    let external_url = extract_external_url(&document);

    Ok(PageInfo {
        public_data_detail_pk: pk,
        public_data_pk,
        ty_detail_code,
        operations,
        external_url,
    })
}

/// Parse an operation detail HTML (from selectApiDetailFunction.do response).
pub fn parse_operation_detail(html: &str) -> Result<ParsedOperation> {
    let document = Html::parse_fragment(html);

    let request_url = extract_labeled_url(&document, "요청주소").unwrap_or_default();
    let service_url = extract_labeled_url(&document, "서비스URL").unwrap_or_default();
    let summary = extract_summary(&document);
    let parameters = extract_request_params(&document);
    let response_fields = extract_response_fields(&document);

    Ok(ParsedOperation {
        request_url,
        service_url,
        summary,
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
            // request_url이 있으면 base_url 기준으로 path 추출
            // 없으면 service_url을 사용하고 path는 "/"
            let path = if !op.request_url.is_empty() && !base_url.is_empty() {
                op.request_url
                    .strip_prefix(&base_url)
                    .unwrap_or(&op.request_url)
                    .to_string()
            } else if !op.request_url.is_empty() {
                op.request_url.clone()
            } else if !op.service_url.is_empty() {
                // 빈 요청주소 + 서비스URL만 있는 경우 → path "/"
                "/".to_string()
            } else {
                return None; // 둘 다 비어있으면 skip
            };

            let final_path = if path.is_empty() {
                "/".to_string()
            } else {
                path
            };

            // Filter out serviceKey from user-visible parameters
            let params: Vec<Parameter> = op
                .parameters
                .iter()
                .filter(|p| !p.name.eq_ignore_ascii_case("serviceKey"))
                .cloned()
                .collect();

            Some(Operation {
                path: final_path,
                method: HttpMethod::Get,
                summary: op.summary.clone(),
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
            format: ResponseFormat::Xml, // TODO: Gateway API 중 JSON 응답도 존재 — _type 파라미터 감지로 개선
        },
        operations,
        fetched_at,
        missing_operations: vec![],
    })
}

// ── Internal helpers ──

fn extract_external_url(document: &Html) -> Option<String> {
    let sel = Selector::parse("a.link-api-btn").ok()?;
    let href = document.select(&sel).next()?.value().attr("href")?.trim();
    if href.starts_with("http") {
        Some(href.to_string())
    } else {
        None
    }
}

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
    use std::sync::LazyLock;
    static RE_PK: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"(?s)(?:name|id)\s*=\s*["']?publicDataDetailPk["']?\s+value\s*=\s*["']([^"']+)["']"#,
        )
        .unwrap()
    });
    RE_PK
        .captures(raw_html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_ty_detail_code(raw_html: &str) -> Option<String> {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"var\s+tyDetailCode\s*=\s*["']([^"']+)["']"#).unwrap()
    });
    RE.captures(raw_html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_summary(document: &Html) -> String {
    let sel = match Selector::parse("h4.tit") {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    document
        .select(&sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default()
}

fn extract_hidden_input_value(document: &Html, id: &str) -> Option<String> {
    let selector = Selector::parse(&format!("input#{id}")).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("value"))
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
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
    // Strategy 1: h4 기반 — "출력결과" h4 다음의 table에서 추출
    if let Some(fields) = extract_response_fields_by_h4(document) {
        if !fields.is_empty() {
            return fields;
        }
    }

    // Strategy 2 (fallback): 기존 tr 스캔 방식 — 레거시 호환
    extract_response_fields_by_tr_scan(document)
}

fn extract_response_fields_by_h4(document: &Html) -> Option<Vec<ResponseField>> {
    use scraper::node::Node;

    let h4_sel = Selector::parse("h4").ok()?;

    for h4 in document.select(&h4_sel) {
        let text = h4.text().collect::<String>();
        if !text.contains("출력결과") {
            continue;
        }

        // h4 다음 형제 노드에서 첫 번째 <table> 찾기
        let mut sibling = h4.next_sibling();
        while let Some(node) = sibling {
            if let Node::Element(ref el) = node.value() {
                if el.name() == "table" {
                    if let Some(table_el) = scraper::ElementRef::wrap(node) {
                        return Some(parse_table_rows_as_response_fields(table_el));
                    }
                }
            }
            sibling = node.next_sibling();
        }
    }
    None
}

fn parse_table_rows_as_response_fields(table: scraper::ElementRef) -> Vec<ResponseField> {
    let tr_sel = match Selector::parse("tr") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let td_sel = match Selector::parse("td") {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut fields = Vec::new();
    for row in table.select(&tr_sel) {
        let cells: Vec<String> = row
            .select(&td_sel)
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();

        // 최소 3컬럼: 순번, 항목명, ... , 설명
        if cells.len() >= 3 {
            let name = &cells[1];
            if name.is_empty() || name == "항목명(영문)" || name == "항목명" {
                continue;
            }
            let description = cells.last().cloned().unwrap_or_default();
            fields.push(ResponseField {
                name: name.clone(),
                description,
                field_type: "string".to_string(),
            });
        }
    }
    fields
}

fn extract_response_fields_by_tr_scan(document: &Html) -> Vec<ResponseField> {
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
            summary: String::new(),
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

    #[test]
    fn test_build_api_spec_empty_request_url_uses_service_url() {
        let ops = vec![ParsedOperation {
            request_url: "".into(),
            service_url: "https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0".into(),
            summary: "초단기실황조회".into(),
            parameters: vec![Parameter {
                name: "serviceKey".into(),
                description: "인증키".into(),
                location: ParamLocation::Query,
                param_type: "string".into(),
                required: true,
                default: None,
            }],
            response_fields: vec![],
        }];

        let spec = build_api_spec("15084084", &ops);
        assert!(
            spec.is_some(),
            "빈 request_url이어도 service_url이 있으면 Operation 생성"
        );
        let spec = spec.unwrap();
        assert_eq!(spec.operations.len(), 1);
        assert_eq!(spec.operations[0].path, "/");
        assert_eq!(spec.operations[0].summary, "초단기실황조회");
    }

    #[test]
    fn test_extract_response_fields_h4_based() {
        // 실제 data.go.kr AJAX 응답 구조: h4 + table 분리
        let html = r#"
        <div id="open-api-detail-result">
            <h4>요청변수(Request Parameter)</h4>
            <table>
                <tr><th>순번</th><th>항목명(영문)</th><th>타입</th><th>크기</th><th>항목구분</th><th>항목설명</th></tr>
                <tr><td>1</td><td>serviceKey</td><td>string</td><td>100</td><td>필수</td><td>인증키</td></tr>
            </table>
            <h4>출력결과(Response Element)</h4>
            <table>
                <tr><th>순번</th><th>항목명(영문)</th><th>타입</th><th>크기</th><th>항목설명</th></tr>
                <tr><td>1</td><td>resultCode</td><td>string</td><td>2</td><td>결과코드</td></tr>
                <tr><td>2</td><td>resultMsg</td><td>string</td><td>50</td><td>결과메시지</td></tr>
                <tr><td>3</td><td>baseDate</td><td>string</td><td>8</td><td>발표일자</td></tr>
            </table>
        </div>
        "#;

        let document = Html::parse_fragment(html);
        let fields = extract_response_fields(&document);
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].name, "resultCode");
        assert_eq!(fields[0].description, "결과코드");
        assert_eq!(fields[1].name, "resultMsg");
        assert_eq!(fields[2].name, "baseDate");
    }

    #[test]
    fn test_parse_operation_detail_extracts_summary() {
        let html = r#"
        <div id="open-api-detail-result">
            <h4 class="tit">초단기실황조회</h4>
            <p><strong>요청주소</strong> https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0/getUltraSrtNcst</p>
            <p><strong>서비스URL</strong> https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0</p>
            <table>
                <tr data-paramtr-nm="serviceKey" data-paramtr-division="필수" data-paramtr-dc="인증키">
                    <td>1</td><td>serviceKey</td><td>string</td><td>100</td><td>필수</td><td>인증키</td>
                </tr>
            </table>
        </div>
        "#;

        let op = parse_operation_detail(html).unwrap();
        assert_eq!(op.summary, "초단기실황조회");
    }

    #[test]
    fn test_parse_operation_detail_no_summary() {
        let html = r#"
        <div>
            <p><strong>요청주소</strong> https://apis.data.go.kr/test/getItems</p>
            <p><strong>서비스URL</strong> https://apis.data.go.kr/test</p>
        </div>
        "#;
        let op = parse_operation_detail(html).unwrap();
        assert!(op.summary.is_empty());
    }

    #[test]
    fn test_parse_openapi_page_extracts_ty_detail_code() {
        let html = r#"
        <html><body>
            <script>
                var tyDetailCode = 'PRDE04';
            </script>
            <input type="hidden" id="publicDataDetailPk" value="uddi:abc-123">
            <input type="hidden" id="publicDataPk" value="15061357">
        </body></html>
        "#;
        let info = parse_openapi_page(html).unwrap();
        assert_eq!(info.ty_detail_code.as_deref(), Some("PRDE04"));
        assert_eq!(info.public_data_pk.as_deref(), Some("15061357"));
    }

    #[test]
    fn test_parse_openapi_page_no_ty_detail_code() {
        let html = r#"
        <html><body>
            <input type="hidden" name="publicDataDetailPk" value="uddi:12345-abcde">
            <select id="open_api_detail_select">
                <option value="1001">getWeather</option>
            </select>
        </body></html>
        "#;
        let info = parse_openapi_page(html).unwrap();
        assert!(info.ty_detail_code.is_none());
        assert!(info.public_data_pk.is_none());
    }

    #[test]
    fn test_parse_openapi_page_extracts_ty_detail_code_double_quotes() {
        let html = r#"
        <html><body>
            <script>
                var tyDetailCode = "PRDE04";
            </script>
            <input type="hidden" id="publicDataDetailPk" value="uddi:abc-123">
        </body></html>
        "#;
        let info = parse_openapi_page(html).unwrap();
        assert_eq!(info.ty_detail_code.as_deref(), Some("PRDE04"));
    }

    #[test]
    fn test_external_url_valid_href() {
        let html = r#"
        <html><body>
            <input type="hidden" id="publicDataDetailPk" value="uddi:ext-123">
            <a class="link-api-btn" href="https://www.kma.go.kr/api/forecast">외부 링크</a>
        </body></html>
        "#;
        let info = parse_openapi_page(html).unwrap();
        assert_eq!(
            info.external_url.as_deref(),
            Some("https://www.kma.go.kr/api/forecast")
        );
    }

    #[test]
    fn test_external_url_no_link_api_btn() {
        let html = r#"
        <html><body>
            <input type="hidden" id="publicDataDetailPk" value="uddi:no-ext">
            <select id="open_api_detail_select">
                <option value="1001">getWeather</option>
            </select>
        </body></html>
        "#;
        let info = parse_openapi_page(html).unwrap();
        assert!(info.external_url.is_none());
    }

    #[test]
    fn test_external_url_javascript_void_ignored() {
        let html = r#"
        <html><body>
            <input type="hidden" id="publicDataDetailPk" value="uddi:js-void">
            <a class="link-api-btn" href="javascript:void(0)">비활성 링크</a>
        </body></html>
        "#;
        let info = parse_openapi_page(html).unwrap();
        assert!(info.external_url.is_none());
    }

    #[test]
    fn test_external_url_ampersand_decoded() {
        // scraper (html5ever) auto-decodes &amp; → &
        let html = r#"
        <html><body>
            <input type="hidden" id="publicDataDetailPk" value="uddi:amp-test">
            <a class="link-api-btn" href="https://example.kr/api?a=1&amp;b=2">링크</a>
        </body></html>
        "#;
        let info = parse_openapi_page(html).unwrap();
        assert_eq!(
            info.external_url.as_deref(),
            Some("https://example.kr/api?a=1&b=2")
        );
    }
}
