use crate::config::AppConfig;
use crate::core::catalog;
use crate::core::types::Catalog;
use anyhow::Result;

pub async fn run() -> Result<()> {
    let cfg = AppConfig::load()?;
    let api_key = cfg.resolve_api_key().ok_or_else(|| {
        anyhow::anyhow!("API 키가 설정되지 않았습니다. korea-cli config set api-key YOUR_KEY")
    })?;

    eprintln!("메타 API에서 카탈로그 수집 중...");
    let services = catalog::fetch_all_services(&api_key).await?;
    let count = services.len();

    let catalog_data = Catalog {
        services,
        updated_at: chrono::Utc::now().format("%Y-%m-%d").to_string(),
    };
    catalog::save_catalog(&catalog_data)?;

    let response = serde_json::json!({
        "success": true,
        "services_count": count,
        "updated_at": catalog_data.updated_at,
    });
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
