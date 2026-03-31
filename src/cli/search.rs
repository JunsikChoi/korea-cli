use crate::core::catalog;
use anyhow::Result;

pub async fn run(query: &str, category: Option<&str>, limit: usize) -> Result<()> {
    let catalog_data = catalog::load_catalog()?;
    if catalog_data.services.is_empty() {
        let response = serde_json::json!({
            "success": false,
            "error": "CATALOG_EMPTY",
            "message": "카탈로그가 비어있습니다.",
            "action": "korea-cli update 를 먼저 실행하세요."
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    let results = catalog::search_catalog(&catalog_data, query, category, limit);
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
