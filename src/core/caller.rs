//! API call engine — builds requests and extracts responses per ApiProtocol.

use crate::core::types::*;
use anyhow::Result;

/// Find an operation by path or by summary name.
pub fn find_operation<'a>(spec: &'a ApiSpec, operation: &str) -> Option<&'a Operation> {
    spec.operations
        .iter()
        .find(|op| op.path == operation || op.summary == operation)
}

/// Call an API using the spec and parameters.
pub async fn call_api(
    spec: &ApiSpec,
    operation_id: &str,
    params: &[(String, String)],
    api_key: &str,
) -> Result<ApiResponse> {
    let op = find_operation(spec, operation_id).ok_or_else(|| {
        anyhow::anyhow!(
            "Operation '{operation_id}' not found. Available: {}",
            spec.operations
                .iter()
                .map(|o| format!("{} ({})", o.path, o.summary))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    // Round 1 W7: base_url trailing slash + path leading slash 충돌 방어
    let base = spec.base_url.trim_end_matches('/');
    let path = if op.path.starts_with('/') {
        op.path.clone()
    } else {
        format!("/{}", op.path)
    };
    let url = format!("{}{}", base, path);
    let client = reqwest::Client::new();

    let mut request = match op.method {
        HttpMethod::Get => client.get(&url),
        HttpMethod::Post => client.post(&url),
        HttpMethod::Put => client.put(&url),
        HttpMethod::Delete => client.delete(&url),
    };

    // Add auth
    request = apply_auth(request, &spec.auth, api_key);

    // Add parameters
    match op.method {
        HttpMethod::Get | HttpMethod::Delete => {
            for (key, value) in params {
                request = request.query(&[(key, value)]);
            }
        }
        HttpMethod::Post | HttpMethod::Put => {
            let body = build_json_body(params);
            request = request.json(&body);
        }
    }

    let response = request.send().await?;
    let status = response.status().as_u16();

    // Parse body based on ResponseFormat (XML or JSON)
    let body: serde_json::Value = match spec.extractor.format {
        ResponseFormat::Json => response.json().await?,
        ResponseFormat::Xml => {
            let text = response.text().await?;
            parse_xml_body(&text)?
        }
    };

    let data = extract_data(&body, &spec.extractor);

    if (200..300).contains(&status) {
        Ok(ApiResponse {
            success: true,
            data: Some(data),
            error: None,
            message: None,
            action: None,
            raw_status: Some(status),
            metadata: Some(serde_json::json!({
                "api_title": spec.list_id,
                "operation": operation_id,
            })),
        })
    } else {
        Ok(ApiResponse {
            success: false,
            data: None,
            error: Some(format!("HTTP_{status}")),
            message: Some(body.to_string()),
            action: Some("Check API key and parameters".to_string()),
            raw_status: Some(status),
            metadata: None,
        })
    }
}

fn apply_auth(
    request: reqwest::RequestBuilder,
    auth: &AuthMethod,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match auth {
        AuthMethod::QueryParam { name } => request.query(&[(name.as_str(), api_key)]),
        AuthMethod::Header { name, prefix } => {
            request.header(name.as_str(), format!("{prefix}{api_key}"))
        }
        AuthMethod::Both {
            query,
            header_name: _,
            header_prefix: _,
            prefer,
        } => match prefer {
            AuthPreference::Query => request.query(&[(query.as_str(), api_key)]),
            AuthPreference::Header => request,
        },
        AuthMethod::None => request,
    }
}

fn build_json_body(params: &[(String, String)]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, value) in params {
        let json_value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.clone()));
        map.insert(key.clone(), json_value);
    }
    serde_json::Value::Object(map)
}

fn extract_data(body: &serde_json::Value, extractor: &ResponseExtractor) -> serde_json::Value {
    let mut current = body;
    for key in &extractor.data_path {
        match current.get(key) {
            Some(v) => current = v,
            None => return body.clone(),
        }
    }
    current.clone()
}

/// XML 응답 본문을 serde_json::Value로 변환한다.
/// data.go.kr Gateway API의 XML 응답을 flat tag→value 또는 중첩 object로 매핑.
///
/// 규칙:
/// - text 노드만 있는 요소: `{tag: "text"}`
/// - 자식 요소가 있는 요소: `{tag: {...}}`
/// - 같은 tag가 반복되면 `{tag: [...]}`로 배열 승격
/// - attribute는 무시 (data.go.kr 응답에 attribute 거의 없음)
/// - $text 래퍼 사용 안 함 (quick-xml serde feature의 구조 변경 방지)
pub fn parse_xml_body(xml: &str) -> Result<serde_json::Value> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    // 스택: (tag_name, children_map, text_buf)
    type Frame = (String, serde_json::Map<String, serde_json::Value>, String);
    let mut stack: Vec<Frame> = vec![];
    let mut root: Option<(String, serde_json::Value)> = None;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                stack.push((name, serde_json::Map::new(), String::new()));
            }
            Ok(Event::End(_)) => {
                let (name, mut children, text) = stack
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("XML 스택 언더플로우"))?;
                // text와 children이 공존하면 $text 키로 보존 (mixed content 대응)
                let value = match (children.is_empty(), text.is_empty()) {
                    (true, _) => serde_json::Value::String(text),
                    (false, true) => serde_json::Value::Object(children),
                    (false, false) => {
                        children.insert("$text".to_string(), serde_json::Value::String(text));
                        serde_json::Value::Object(children)
                    }
                };
                if let Some(parent) = stack.last_mut() {
                    // 반복 태그 → 배열 승격
                    match parent.1.remove(&name) {
                        Some(serde_json::Value::Array(mut arr)) => {
                            arr.push(value);
                            parent.1.insert(name, serde_json::Value::Array(arr));
                        }
                        Some(existing) => {
                            parent
                                .1
                                .insert(name, serde_json::Value::Array(vec![existing, value]));
                        }
                        None => {
                            parent.1.insert(name, value);
                        }
                    }
                } else {
                    root = Some((name, value));
                }
            }
            Ok(Event::Text(e)) => {
                // trim_text(true)는 whitespace-only 노드만 제거.
                // 실제 값의 leading/trailing padding은 여기서 trim (Eval R2 Codex S1).
                let txt = e
                    .unescape()
                    .map_err(|err| anyhow::anyhow!("XML text unescape 실패: {err}"))?
                    .trim()
                    .to_string();
                if !txt.is_empty() {
                    if let Some(frame) = stack.last_mut() {
                        frame.2.push_str(&txt);
                    }
                }
            }
            Ok(Event::CData(e)) => {
                // CDATA는 raw 바이트로 유지 (unescape 불필요)
                let txt = String::from_utf8_lossy(e.as_ref()).to_string();
                if let Some(frame) = stack.last_mut() {
                    frame.2.push_str(&txt);
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if let Some(parent) = stack.last_mut() {
                    parent.1.insert(name, serde_json::Value::Null);
                } else {
                    // 루트 레벨 self-closing 태그 (예: <response/>)
                    root = Some((name, serde_json::Value::Null));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {} // comments, declarations, etc. — skip
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "XML 파싱 실패 at pos {}: {}",
                    reader.buffer_position(),
                    e
                ));
            }
        }
        buf.clear();
    }

    let (root_name, root_value) = root.ok_or_else(|| anyhow::anyhow!("XML 루트 노드 없음"))?;
    let mut out = serde_json::Map::new();
    out.insert(root_name, root_value);
    Ok(serde_json::Value::Object(out))
}
