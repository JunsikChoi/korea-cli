use crate::core::bundle::BUNDLE;
use anyhow::Result;

pub async fn run(list_id: &str) -> Result<()> {
    let spec = BUNDLE
        .specs
        .get(list_id)
        .ok_or_else(|| anyhow::anyhow!("API spec을 찾을 수 없습니다: {list_id}"))?;

    let output = serde_json::json!({
        "success": true,
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
    Ok(())
}
