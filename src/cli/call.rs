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

    // PartialStub 안내: spec이 있지만 요청한 operation이 없을 수 있음
    let entry = BUNDLE.catalog.iter().find(|e| e.list_id == list_id);
    let is_partial =
        entry.is_some_and(|e| e.spec_status == crate::core::types::SpecStatus::PartialStub);

    let spec = BUNDLE.specs.get(list_id).unwrap();

    // 요청한 operation이 spec에 없고 PartialStub이면 안내
    let has_operation = spec
        .operations
        .iter()
        .any(|op| op.path == operation || op.summary == operation);
    if !has_operation && is_partial {
        let response = serde_json::json!({
            "success": false,
            "list_id": list_id,
            "spec_status": "PartialStub",
            "message": "이 API는 일부 operation만 수집됨 — `korea-cli update`로 최신 번들을 받으면 추가 operation이 포함될 수 있습니다",
            "available_operations": spec.operations.iter().map(|op| &op.path).collect::<Vec<_>>(),
        });
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

    let result = caller::call_api(spec, operation, params, &api_key).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
