use crate::core::bundle::BUNDLE;
use crate::core::catalog;
use anyhow::Result;

pub async fn run(query: &str, category: Option<&str>, limit: usize) -> Result<()> {
    if BUNDLE.catalog.is_empty() {
        let response = serde_json::json!({
            "success": false,
            "error": "BUNDLE_EMPTY",
            "message": "번들이 비어있습니다.",
            "action": "korea-cli update 를 먼저 실행하세요."
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    let results = catalog::search_bundle_catalog(&BUNDLE.catalog, query, category, limit);
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
