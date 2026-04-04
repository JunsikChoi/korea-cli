use anyhow::{Context, Result};
use korea_cli::core::types::CURRENT_SCHEMA_VERSION;

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

    let bytes = response.bytes().await.context("번들 데이터 수신 실패")?;

    // Verify the downloaded bundle is valid
    let bundle = crate::core::bundle::decompress_and_deserialize(&bytes)
        .context("다운로드된 번들이 유효하지 않습니다")?;

    // Schema version 검증
    let remote_version = bundle.metadata.schema_version;
    if remote_version != CURRENT_SCHEMA_VERSION {
        let msg = if remote_version > CURRENT_SCHEMA_VERSION {
            format!(
                "새 번들(v{})은 최신 CLI가 필요합니다. `cargo install korea-cli`로 업데이트하세요 (현재 CLI: v{})",
                remote_version, CURRENT_SCHEMA_VERSION
            )
        } else {
            format!(
                "구버전 번들(v{})입니다. 최신 Release가 아직 생성되지 않았습니다 (현재 CLI: v{})",
                remote_version, CURRENT_SCHEMA_VERSION
            )
        };
        let output = serde_json::json!({
            "success": false,
            "error": "SCHEMA_MISMATCH",
            "message": msg,
            "remote_schema_version": remote_version,
            "local_schema_version": CURRENT_SCHEMA_VERSION,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Atomic 저장: tmp → rename
    let path = crate::config::paths::bundle_override_file()?;
    let tmp_path = path.with_file_name("bundle.zstd.tmp");
    std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")))?;
    std::fs::write(&tmp_path, &bytes)?;
    std::fs::rename(&tmp_path, &path)?;

    let output = serde_json::json!({
        "success": true,
        "version": bundle.metadata.version,
        "schema_version": bundle.metadata.schema_version,
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
