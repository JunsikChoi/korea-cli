//! Bundle loading: decompress zstd + deserialize postcard.
//!
//! Override chain:
//!   1. ~/.config/korea-cli/bundle.zstd  (from `korea-cli update`)
//!   2. Embedded bundle (include_bytes!, built into binary)

use anyhow::Result;
use once_cell::sync::Lazy;

use crate::core::types::Bundle;

/// Embedded bundle, compiled into binary via build.rs.
static EMBEDDED_BUNDLE: &[u8] = include_bytes!("../../data/bundle.zstd");

/// Global bundle instance. Initialized once on first access.
pub static BUNDLE: Lazy<Bundle> = Lazy::new(|| load_bundle().expect("Failed to load bundle"));

/// Load bundle with override chain: local file > embedded.
pub fn load_bundle() -> Result<Bundle> {
    // 1. Local override
    if let Ok(path) = crate::config::paths::bundle_override_file() {
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            return decompress_and_deserialize(&bytes);
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
        // Embedded bundle is either placeholder (dev) or real (with data/bundle.zstd)
        let bundle = load_bundle().unwrap();
        assert!(!bundle.metadata.version.is_empty());
    }
}
