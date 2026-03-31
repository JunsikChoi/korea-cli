use anyhow::{Context, Result};

/// GitHub repository for bundle downloads.
const BUNDLE_DOWNLOAD_URL: &str =
    "https://github.com/JunsikChoi/korea-cli/releases/latest/download/bundle.zstd";

pub async fn run() -> Result<()> {
    eprintln!("최신 번들 다운로드 중...");

    let client = reqwest::Client::builder()
        .user_agent("korea-cli/0.1.0")
        .build()?;

    let response = client
        .get(BUNDLE_DOWNLOAD_URL)
        .send()
        .await
        .context("GitHub Releases 연결 실패")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "번들 다운로드 실패: HTTP {}. 릴리스가 존재하는지 확인하세요.",
            response.status()
        );
    }

    let bytes = response
        .bytes()
        .await
        .context("번들 데이터 수신 실패")?;

    // Verify the downloaded bundle is valid
    let bundle = crate::core::bundle::decompress_and_deserialize(&bytes)
        .context("다운로드된 번들이 유효하지 않습니다")?;

    // Save to local override path
    let path = crate::config::paths::bundle_override_file()?;
    std::fs::write(&path, &bytes)?;

    let output = serde_json::json!({
        "success": true,
        "version": bundle.metadata.version,
        "api_count": bundle.metadata.api_count,
        "spec_count": bundle.metadata.spec_count,
        "size_bytes": bytes.len(),
        "saved_to": path.display().to_string(),
        "message": format!(
            "번들 업데이트 완료: v{} ({} APIs, {} specs)",
            bundle.metadata.version, bundle.metadata.api_count, bundle.metadata.spec_count
        ),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
