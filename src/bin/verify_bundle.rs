//! 번들의 schema_version이 바이너리의 CURRENT_SCHEMA_VERSION과 일치하는지 검증한다.
//! release.yml workflow에서 번들 다운로드 후 바이너리 빌드 전에 실행.
//!
//! 동작: Bundle.metadata (첫 필드)만 postcard::take_from_bytes로 peek하여
//! schema_version을 비교. struct 전체 호환성에 의존하지 않음.
//!
//! # 호환성 전제 (중요)
//! 1. `Bundle` struct의 **첫 필드**가 `metadata: BundleMetadata`여야 한다 (types.rs).
//! 2. `BundleMetadata` struct 자체의 **바이너리 레이아웃**이 버전 간 호환되어야 한다.
//!    필드 순서 변경/삽입 금지. 필드 추가 시 반드시 **맨 마지막**에 추가하고
//!    schema_version도 동시에 bump해야 이 gate가 정확한 메시지를 출력한다.
//!    그렇지 않으면 "BundleMetadata 역직렬화 실패"로 먼저 터져서
//!    schema 불일치 메시지에 도달하지 못한다 (회귀).
//!
//! postcard는 struct 필드 추가 시 trailing bytes 부족으로 역직렬화 실패하므로,
//! schema_version 비교는 반드시 `!=` (정확 일치)여야 하며 `<=`는 올바르지 않다.

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
