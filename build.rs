//! Build script: ensures data/bundle.zstd exists for include_bytes!.
//! If missing, creates a minimal placeholder bundle.

use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

// Mirror types (build script can't use crate types).
// Empty collections encode identically regardless of element type in postcard.
#[derive(Serialize)]
struct PlaceholderBundle {
    metadata: PlaceholderMetadata,
    catalog: Vec<PlaceholderEntry>,
    specs: HashMap<String, PlaceholderEntry>,
}

#[derive(Serialize)]
struct PlaceholderMetadata {
    version: String,
    api_count: usize,
    spec_count: usize,
    checksum: String,
}

#[derive(Serialize)]
struct PlaceholderEntry;

fn main() {
    println!("cargo:rerun-if-changed=data/bundle.zstd");

    let path = Path::new("data/bundle.zstd");
    if !path.exists() {
        eprintln!("Creating placeholder bundle at data/bundle.zstd...");
        std::fs::create_dir_all("data").unwrap();

        let bundle = PlaceholderBundle {
            metadata: PlaceholderMetadata {
                version: "placeholder".into(),
                api_count: 0,
                spec_count: 0,
                checksum: "".into(),
            },
            catalog: vec![],
            specs: HashMap::new(),
        };

        let bytes = postcard::to_allocvec(&bundle).unwrap();
        let compressed = zstd::encode_all(bytes.as_slice(), 3).unwrap();
        std::fs::write(path, &compressed).unwrap();
        eprintln!("Placeholder bundle created ({} bytes)", compressed.len());
    }
}
