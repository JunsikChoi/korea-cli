use crate::config::AppConfig;
use crate::core::{bundle::BUNDLE, caller};
use anyhow::Result;

pub async fn run(list_id: &str, operation: &str, params: &[(String, String)]) -> Result<()> {
    // Check spec availability before attempting call
    if !BUNDLE.specs.contains_key(list_id) {
        let entry = BUNDLE.catalog.iter().find(|e| e.list_id == list_id);
        let response = match entry {
            Some(entry) => serde_json::json!({
                "success": false,
                "list_id": list_id,
                "spec_status": entry.spec_status,
                "message": entry.spec_status.user_message(),
                "endpoint_url": entry.endpoint_url,
                "data_go_kr_url": format!("https://www.data.go.kr/data/{list_id}/openapi.do"),
            }),
            None => serde_json::json!({
                "success": false,
                "error": "NOT_FOUND",
                "message": format!("API를 찾을 수 없습니다: {list_id}"),
            }),
        };
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    let cfg = AppConfig::load()?;
    let api_key = match cfg.resolve_api_key() {
        Some(key) => key,
        None => {
            let response = serde_json::json!({
                "success": false,
                "error": "NO_API_KEY",
                "message": "API 키가 설정되지 않았습니다.",
                "action": "korea-cli config set api-key YOUR_KEY 또는 환경변수 DATA_GO_KR_API_KEY 설정"
            });
            println!("{}", serde_json::to_string_pretty(&response)?);
            return Ok(());
        }
    };

    let spec = BUNDLE.specs.get(list_id).unwrap();
    let result = caller::call_api(spec, operation, params, &api_key).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
