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

    let url = format!("{}{}", spec.base_url, op.path);
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
    let body: serde_json::Value = response.json().await?;

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
