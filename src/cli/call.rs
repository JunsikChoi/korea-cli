use crate::config::AppConfig;
use crate::core::{bundle::BUNDLE, caller};
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

    let spec = BUNDLE
        .specs
        .get(list_id)
        .ok_or_else(|| anyhow::anyhow!("API spec을 찾을 수 없습니다: {list_id}"))?;

    let result = caller::call_api(spec, operation, params, &api_key).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
