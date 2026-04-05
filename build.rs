//! Build script: ensures data/bundle.zstd exists for include_bytes!.
//!
//! Resolution order:
//!   1. data/bundle.zstd already present → use as-is (local dev, bundle CI artifact)
//!   2. BUNDLE_DOWNLOAD_URL env var set → download from that URL (binary release CI)
//!   3. Nothing → create a minimal placeholder bundle (CI compile checks, local dev)

use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

// Mirror types — build script cannot import crate types.
#[derive(Serialize)]
struct PlaceholderBundle {
    metadata: PlaceholderMetadata,
    catalog: Vec<PlaceholderEntry>,
    specs: HashMap<String, PlaceholderEntry>,
}

#[derive(Serialize)]
struct PlaceholderMetadata {
    version: String,
    schema_version: u32,
    api_count: usize,
    spec_count: usize,
    checksum: String,
}

#[derive(Serialize)]
struct PlaceholderEntry;

fn main() {
    println!("cargo:rerun-if-changed=data/bundle.zstd");
    // Re-run if the download URL changes (e.g. a new release tag is passed in CI).
    println!("cargo:rerun-if-env-changed=BUNDLE_DOWNLOAD_URL");

    let path = Path::new("data/bundle.zstd");

    if path.exists() {
        // Case 1: bundle already on disk — nothing to do.
        eprintln!("build.rs: data/bundle.zstd found, skipping download/placeholder.");
        return;
    }

    std::fs::create_dir_all("data").unwrap();

    // Case 2: CI provides a pre-built bundle URL.
    if let Ok(url) = std::env::var("BUNDLE_DOWNLOAD_URL") {
        eprintln!("build.rs: downloading bundle from {url}");
        download_bundle(&url, path);
        return;
    }

    // Case 3: Fallback placeholder — zero-entry bundle.
    eprintln!("build.rs: no bundle found, creating placeholder.");
    write_placeholder(path);
}

fn download_bundle(url: &str, dest: &Path) {
    // Use curl/wget — avoids adding reqwest to build-dependencies.
    // curl is available on all GitHub-hosted runners (ubuntu/macos/windows).
    let status = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location", // follow redirects (GitHub Release redirects to S3)
            "--fail",     // non-zero exit on HTTP 4xx/5xx
            "--output",
            dest.to_str().unwrap(),
            url,
        ])
        .status()
        .expect("build.rs: curl not found — cannot download bundle");

    if !status.success() {
        panic!("build.rs: bundle download failed (curl exit {})", status);
    }

    let size = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    eprintln!("build.rs: bundle downloaded ({size} bytes)");
}

fn write_placeholder(path: &Path) {
    let bundle = PlaceholderBundle {
        metadata: PlaceholderMetadata {
            // schema_version 0 → load_bundle() falls back to embedded immediately
            // when a real bundle exists at the override path.
            version: "placeholder".into(),
            schema_version: 0,
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
    eprintln!(
        "build.rs: placeholder created ({} bytes)",
        compressed.len()
    );
}
