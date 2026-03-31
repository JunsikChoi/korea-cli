//! MCP tool handlers — search_api, get_api_spec, call_api.

use crate::config::AppConfig;
use crate::core::{caller, catalog, swagger};
use serde_json::json;

pub async fn handle_tool_call(params: serde_json::Value) -> serde_json::Value {
    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");

    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match tool_name {
        "search_api" => handle_search(arguments).await,
        "get_api_spec" => handle_get_spec(arguments).await,
        "call_api" => handle_call(arguments).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {tool_name}")),
    };

    match result {
        Ok(content) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&content).unwrap_or_default() }]
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": json!({ "success": false, "error": e.to_string() }).to_string() }],
            "isError": true
        }),
    }
}

async fn handle_search(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' parameter"))?;
    let category = args.get("category").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let catalog_data = catalog::load_catalog()?;
    if catalog_data.services.is_empty() {
        return Ok(json!({
            "success": false, "error": "CATALOG_EMPTY",
            "message": "카탈로그가 비어있습니다.",
            "action": "korea-cli update 를 먼저 실행하세요."
        }));
    }

    let results = catalog::search_catalog(&catalog_data, query, category, limit);
    Ok(serde_json::to_value(results)?)
}

async fn handle_get_spec(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let list_id = args
        .get("list_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'list_id' parameter"))?;

    let spec = swagger::fetch_and_cache_spec(list_id).await?;
    let has_key = AppConfig::load()?.resolve_api_key().is_some();

    let mut output = serde_json::to_value(&spec)?;
    if let Some(obj) = output.as_object_mut() {
        obj.insert("has_api_key".into(), json!(has_key));
        if !has_key {
            obj.insert(
                "key_guide".into(),
                json!(
                    "이 API를 호출하려면 API 키가 필요합니다. DATA_GO_KR_API_KEY 환경변수를 설정하세요."
                ),
            );
        }
    }

    Ok(output)
}

async fn handle_call(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let list_id = args
        .get("list_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'list_id' parameter"))?;
    let operation = args
        .get("operation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'operation' parameter"))?;

    let cfg = AppConfig::load()?;
    let api_key = match cfg.resolve_api_key() {
        Some(key) => key,
        None => {
            return Ok(json!({
                "success": false, "error": "NO_API_KEY",
                "message": "API 키가 설정되지 않았습니다.",
                "action": "DATA_GO_KR_API_KEY 환경변수를 설정하세요."
            }));
        }
    };

    let spec = swagger::fetch_and_cache_spec(list_id).await?;

    let params: Vec<(String, String)> = args
        .get("params")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| {
                    let value = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), value)
                })
                .collect()
        })
        .unwrap_or_default();

    let result = caller::call_api(&spec, operation, &params, &api_key).await?;
    Ok(serde_json::to_value(result)?)
}
