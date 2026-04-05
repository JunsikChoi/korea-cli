//! 번들의 schema_version이 바이너리의 CURRENT_SCHEMA_VERSION과 일치하는지 검증한다.
//! release.yml workflow에서 번들 다운로드 후 바이너리 빌드 전에 실행.
//!
//! 동작: Bundle.metadata (첫 필드)만 postcard::take_from_bytes로 peek하여
//! schema_version을 비교. struct 전체 호환성에 의존하지 않음.

use korea_cli::core::types::{BundleMetadata, CURRENT_SCHEMA_VERSION};

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: verify-bundle <path>"))?;
    let bytes = std::fs::read(&path)?;
    let decompressed =
        zstd::decode_all(bytes.as_slice()).map_err(|e| anyhow::anyhow!("zstd 해제 실패: {e}"))?;

    // metadata만 peek (Bundle의 첫 필드 = BundleMetadata)
    let (metadata, _rest): (BundleMetadata, _) = postcard::take_from_bytes(&decompressed)
        .map_err(|e| anyhow::anyhow!("BundleMetadata 역직렬화 실패: {e}"))?;

    if metadata.schema_version != CURRENT_SCHEMA_VERSION {
        anyhow::bail!(
            "schema_version 불일치: 번들={}, 바이너리={}. 올바른 번들 태그 사용 필요",
            metadata.schema_version,
            CURRENT_SCHEMA_VERSION
        );
    }
    println!("OK: schema_version = {}", metadata.schema_version);
    Ok(())
}
