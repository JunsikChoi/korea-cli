use crate::core::bundle::BUNDLE;
use anyhow::Result;

pub async fn run(list_id: &str) -> Result<()> {
    // Check if spec exists (Available status)
    if let Some(spec) = BUNDLE.specs.get(list_id) {
        let output = serde_json::json!({
            "success": true,
            "spec_status": crate::core::types::SpecStatus::Available,
            "list_id": spec.list_id,
            "base_url": spec.base_url,
            "auth": spec.auth,
            "has_api_key": crate::config::AppConfig::load()?.resolve_api_key().is_some(),
            "operations": spec.operations.iter().map(|op| {
                serde_json::json!({
                    "path": op.path,
                    "method": op.method,
                    "summary": op.summary,
                    "parameters": op.parameters,
                    "request_body": op.request_body,
                    "response_fields": op.response_fields,
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // No spec — look up catalog entry for status info
    let entry = BUNDLE.catalog.iter().find(|e| e.list_id == list_id);
    match entry {
        Some(entry) => {
            let output = serde_json::json!({
                "success": false,
                "list_id": list_id,
                "spec_status": entry.spec_status,
                "endpoint_url": entry.endpoint_url,
                "message": entry.spec_status.user_message(),
                "data_go_kr_url": format!("https://www.data.go.kr/data/{list_id}/openapi.do"),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        None => {
            let output = serde_json::json!({
                "success": false,
                "error": "NOT_FOUND",
                "message": format!("API를 찾을 수 없습니다: {list_id}"),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }
    Ok(())
}
