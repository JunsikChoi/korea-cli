use crate::config::AppConfig;
use crate::core::{caller, swagger};
use anyhow::Result;

pub async fn run(list_id: &str, operation: &str, params: &[(String, String)]) -> Result<()> {
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

    let spec = swagger::fetch_and_cache_spec(list_id).await?;
    let result = caller::call_api(&spec, operation, params, &api_key).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
