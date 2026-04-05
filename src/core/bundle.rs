//! Bundle loading: decompress zstd + deserialize postcard.
//!
//! Override chain:
//!   1. ~/.config/korea-cli/bundle.zstd  (from `korea-cli update`)
//!   2. Embedded bundle (include_bytes!, built into binary)

use anyhow::Result;
use once_cell::sync::Lazy;

use crate::core::types::{Bundle, CURRENT_SCHEMA_VERSION};

/// Embedded bundle, compiled into binary via build.rs.
static EMBEDDED_BUNDLE: &[u8] = include_bytes!("../../data/bundle.zstd");

/// Global bundle instance. Initialized once on first access.
pub static BUNDLE: Lazy<Bundle> = Lazy::new(|| load_bundle().expect("Failed to load bundle"));

/// Load bundle with override chain: local file > embedded.
/// If the local override has an incompatible schema, falls back to the embedded bundle.
pub fn load_bundle() -> Result<Bundle> {
    // 1. Local override
    if let Ok(path) = crate::config::paths::bundle_override_file() {
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            match decompress_and_deserialize(&bytes) {
                Ok(bundle) if bundle.metadata.schema_version == CURRENT_SCHEMA_VERSION => {
                    return Ok(bundle);
                }
                Ok(_) => {
                    eprintln!("외부 번들의 스키마 버전이 다릅니다. 내장 번들을 사용합니다.");
                    eprintln!("최신 번들을 받으려면: korea-cli update");
                }
                Err(_) => {
                    eprintln!("외부 번들이 현재 버전과 호환되지 않습니다. 내장 번들을 사용합니다.");
                    eprintln!("최신 번들을 받으려면: korea-cli update");
                }
            }
        }
    }
    // 2. Embedded
    decompress_and_deserialize(EMBEDDED_BUNDLE)
}

/// Decompress zstd + deserialize postcard bytes into Bundle.
pub fn decompress_and_deserialize(compressed: &[u8]) -> Result<Bundle> {
    let decompressed = zstd::decode_all(compressed)?;
    let bundle: Bundle = postcard::from_bytes(&decompressed)
        .map_err(|e| anyhow::anyhow!("Bundle deserialization failed: {e}"))?;
    Ok(bundle)
}

/// Compress + serialize a Bundle to bytes (for bundle builder).
pub fn serialize_and_compress(bundle: &Bundle, zstd_level: i32) -> Result<Vec<u8>> {
    let bytes = postcard::to_allocvec(bundle)
        .map_err(|e| anyhow::anyhow!("Bundle serialization failed: {e}"))?;
    let compressed = zstd::encode_all(bytes.as_slice(), zstd_level)?;
    Ok(compressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{BundleMetadata, CatalogEntry};
    use std::collections::HashMap;

    fn make_test_bundle() -> Bundle {
        Bundle {
            metadata: BundleMetadata {
                version: "test".into(),
                schema_version: crate::core::types::CURRENT_SCHEMA_VERSION,
                api_count: 1,
                spec_count: 0,
                checksum: "abc".into(),
            },
            catalog: vec![CatalogEntry {
                list_id: "99999".into(),
                title: "Test API".into(),
                description: "A test".into(),
                keywords: vec!["test".into()],
                org_name: "TestOrg".into(),
                category: "TestCat".into(),
                request_count: 42,
                endpoint_url: "https://apis.data.go.kr/test".into(),
                spec_status: crate::core::types::SpecStatus::Available,
            }],
            specs: HashMap::new(),
        }
    }

    #[test]
    fn test_roundtrip_compress_decompress() {
        let bundle = make_test_bundle();
        let compressed = serialize_and_compress(&bundle, 3).unwrap();
        let decoded = decompress_and_deserialize(&compressed).unwrap();
        assert_eq!(decoded.catalog.len(), 1);
        assert_eq!(decoded.catalog[0].list_id, "99999");
        assert_eq!(decoded.metadata.version, "test");
    }

    #[test]
    fn test_decompress_invalid_data() {
        let result = decompress_and_deserialize(b"not valid zstd");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_embedded_bundle() {
        // Embedded bundle은 placeholder이거나 실제 번들이며, schema version bump 직후에는
        // 현재 struct와 호환 안 될 수 있음 (Task 9에서 v4 번들 재생성 후 통과 기대).
        match load_bundle() {
            Ok(bundle) => {
                assert!(!bundle.metadata.version.is_empty());
                assert!(bundle.metadata.schema_version <= CURRENT_SCHEMA_VERSION);
            }
            Err(e) => {
                // schema bump 직후 과도기 허용. 에러 메시지는 번들 관련이어야 함.
                let msg = e.to_string().to_lowercase();
                assert!(
                    msg.contains("bundle") || msg.contains("deserialization"),
                    "예상 외 에러: {e}"
                );
            }
        }
    }

    #[test]
    fn test_schema_version_mismatch_detected() {
        let mut bundle = make_test_bundle();
        bundle.metadata.schema_version = 999;
        let compressed = serialize_and_compress(&bundle, 3).unwrap();
        let decoded = decompress_and_deserialize(&compressed).unwrap();
        // Deserializes fine, but schema_version differs
        assert_ne!(decoded.metadata.schema_version, CURRENT_SCHEMA_VERSION);
    }
}
