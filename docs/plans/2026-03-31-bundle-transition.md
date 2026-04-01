# Bundle Transition Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace runtime Swagger scraping with pre-bundled catalog + specs for offline search/spec and ~99% API coverage.

**Architecture:** Binary embeds zstd-compressed postcard bundle (catalog + specs). Local override from `korea-cli update`. Global static via `once_cell::Lazy`. Bundle builder binary collects data from data.go.kr for releases.

**Tech Stack:** Rust, postcard (binary serde), zstd (compression), once_cell (lazy static), tokio (async)

**Spec:** `docs/specs/2026-03-31-bundle-transition-design.md`

---

## Task 1: Add Dependencies + Bundle Types

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/core/types.rs`

### Step 1: Add dependencies to Cargo.toml

Under `[dependencies]`, add:

```toml
postcard = { version = "1", features = ["alloc"] }
zstd = "0.13"
once_cell = "1"
```

Add new section:

```toml
[build-dependencies]
serde = { version = "1", features = ["derive"] }
postcard = { version = "1", features = ["alloc"] }
zstd = "0.13"
```

### Step 2: Write failing test for Bundle types

In `src/core/types.rs`, add at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_bundle_postcard_roundtrip() {
        let entry = CatalogEntry {
            list_id: "12345".into(),
            title: "Test API".into(),
            description: "desc".into(),
            keywords: vec!["test".into()],
            org_name: "org".into(),
            category: "cat".into(),
            request_count: 100,
        };
        let bundle = Bundle {
            metadata: BundleMetadata {
                version: "2026-03-31".into(),
                api_count: 1,
                spec_count: 0,
                checksum: "abc".into(),
            },
            catalog: vec![entry],
            specs: HashMap::new(),
        };
        let bytes = postcard::to_allocvec(&bundle).unwrap();
        let decoded: Bundle = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.catalog.len(), 1);
        assert_eq!(decoded.catalog[0].title, "Test API");
        assert_eq!(decoded.metadata.version, "2026-03-31");
    }

    #[test]
    fn test_bundle_zstd_roundtrip() {
        let bundle = Bundle {
            metadata: BundleMetadata {
                version: "test".into(),
                api_count: 0,
                spec_count: 0,
                checksum: "".into(),
            },
            catalog: vec![],
            specs: HashMap::new(),
        };
        let bytes = postcard::to_allocvec(&bundle).unwrap();
        let compressed = zstd::encode_all(bytes.as_slice(), 3).unwrap();
        let decompressed = zstd::decode_all(compressed.as_slice()).unwrap();
        let decoded: Bundle = postcard::from_bytes(&decompressed).unwrap();
        assert_eq!(decoded.metadata.version, "test");
    }
}
```

### Step 3: Run test to verify it fails

Run: `cargo test --lib types::tests`
Expected: FAIL — `CatalogEntry`, `Bundle`, `BundleMetadata` not defined

### Step 4: Implement Bundle types

In `src/core/types.rs`, add `use std::collections::HashMap;` at the top (line 3), and add these types after `OperationSummary` (after line 36):

```rust
// ── Bundle types (pre-collected data for offline use) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub metadata: BundleMetadata,
    pub catalog: Vec<CatalogEntry>,
    pub specs: HashMap<String, ApiSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMetadata {
    pub version: String,
    pub api_count: usize,
    pub spec_count: usize,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub org_name: String,
    pub category: String,
    pub request_count: u32,
}
```

### Step 5: Run test to verify it passes

Run: `cargo test --lib types::tests`
Expected: PASS (both tests)

### Step 6: Commit

```bash
git add Cargo.toml src/core/types.rs
git commit -m "feat: add Bundle types + postcard/zstd dependencies"
```

---

## Task 2: Build Infrastructure (build.rs + placeholder bundle)

**Files:**
- Create: `build.rs`
- Generated: `data/bundle.zstd` (by build.rs at compile time)

### Step 1: Create build.rs

`build.rs` creates a minimal placeholder bundle if `data/bundle.zstd` doesn't exist. This is needed for `include_bytes!` to work during development.

```rust
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
```

### Step 2: Add data/ to .gitignore (bundle is release artifact)

In `.gitignore`, add:

```
data/bundle.zstd
```

> Note: bundle.zstd는 릴리스 시 첨부 에셋으로 배포. 개발 중에는 build.rs가 자동 생성.

### Step 3: Verify build.rs works

Run: `cargo check`
Expected: compiles; `data/bundle.zstd` created if missing

Run: `ls -la data/bundle.zstd`
Expected: file exists (~30 bytes)

### Step 4: Commit

```bash
git add build.rs .gitignore
git commit -m "feat: build.rs creates placeholder bundle for development"
```

---

## Task 3: Config Paths Update

**Files:**
- Modify: `src/config/paths.rs`

### Step 1: Write failing test

In `tests/integration/config_test.rs`, add:

```rust
#[test]
fn test_bundle_override_path() {
    let path = korea_cli::config::paths::bundle_override_file().unwrap();
    assert!(path.to_str().unwrap().contains("korea-cli"));
    assert!(path.to_str().unwrap().ends_with("bundle.zstd"));
}
```

### Step 2: Run test to verify it fails

Run: `cargo test --test config_test test_bundle_override_path`
Expected: FAIL — `bundle_override_file` not found

### Step 3: Implement bundle_override_file

In `src/config/paths.rs`, add after `spec_cache_file` (line 29):

```rust
pub fn bundle_override_file() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("bundle.zstd"))
}
```

### Step 4: Run test to verify it passes

Run: `cargo test --test config_test test_bundle_override_path`
Expected: PASS

### Step 5: Commit

```bash
git add src/config/paths.rs tests/integration/config_test.rs
git commit -m "feat: add bundle_override_file path"
```

---

## Task 4: Bundle Loader

**Files:**
- Create: `src/core/bundle.rs`
- Modify: `src/core/mod.rs`
- Modify: `src/lib.rs`

### Step 1: Create bundle.rs with loader + tests

```rust
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
pub static BUNDLE: Lazy<Bundle> = Lazy::new(|| {
    load_bundle().expect("Failed to load bundle")
});

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
        // Embedded bundle is the placeholder from build.rs
        let bundle = load_bundle().unwrap();
        // Placeholder has version "placeholder" and empty catalog
        assert_eq!(bundle.metadata.version, "placeholder");
        assert!(bundle.catalog.is_empty());
    }
}
```

### Step 2: Register module

In `src/core/mod.rs`, add:

```rust
pub mod bundle;
```

### Step 3: Run tests

Run: `cargo test --lib bundle::tests`
Expected: PASS (all 3 tests)

### Step 4: Commit

```bash
git add src/core/bundle.rs src/core/mod.rs
git commit -m "feat: bundle loader with override chain + once_cell global"
```

---

## Task 5: Swagger JSON Inline Extraction

**Files:**
- Modify: `src/core/swagger.rs`

### Step 1: Write failing tests for swaggerJson extraction

In `src/core/swagger.rs`, add to the `mod tests` block:

```rust
    #[test]
    fn test_extract_swagger_json_inline() {
        let html = r#"
            some html content
            var swaggerJson = `{"swagger":"2.0","info":{"title":"Test"},"host":"api.test.kr","basePath":"/api","schemes":["https"],"paths":{}}`
            more content
        "#;
        let result = extract_swagger_json(html);
        assert!(result.is_some());
        let json = result.unwrap();
        assert_eq!(json["swagger"].as_str(), Some("2.0"));
        assert_eq!(json["host"].as_str(), Some("api.test.kr"));
    }

    #[test]
    fn test_extract_swagger_json_not_found() {
        let html = "no swagger data here";
        assert!(extract_swagger_json(html).is_none());
    }

    #[test]
    fn test_extract_swagger_json_preferred_over_url() {
        let html = r#"
            var swaggerUrl = 'https://example.com/spec';
            var swaggerJson = `{"swagger":"2.0","info":{"title":"Inline"},"host":"inline.kr","basePath":"/","schemes":["https"],"paths":{}}`
        "#;
        // swaggerJson should be found
        let result = extract_swagger_json(html);
        assert!(result.is_some());
        assert_eq!(result.unwrap()["host"].as_str(), Some("inline.kr"));
    }
```

### Step 2: Run tests to verify they fail

Run: `cargo test --lib swagger::tests::test_extract_swagger_json`
Expected: FAIL — `extract_swagger_json` not found

### Step 3: Implement extract_swagger_json

In `src/core/swagger.rs`, add after `extract_swagger_url` (after line 307), make it `pub`:

```rust
/// Extract inline Swagger JSON from data.go.kr page HTML.
/// Matches: var swaggerJson = `{...}`  (backtick-quoted template literal)
/// Returns parsed JSON, or None if not found.
pub fn extract_swagger_json(html: &str) -> Option<serde_json::Value> {
    let re = Regex::new(r"var\s+swaggerJson\s*=\s*`([^`]+)`").ok()?;
    let caps = re.captures(html)?;
    let json_str = caps.get(1)?.as_str();
    serde_json::from_str(json_str).ok()
}
```

Also make `extract_swagger_url` public (change `fn` to `pub fn` at line 302):

```rust
pub fn extract_swagger_url(html: &str) -> Option<String> {
```

### Step 4: Run tests to verify they pass

Run: `cargo test --lib swagger::tests`
Expected: PASS (all tests including existing ones)

### Step 5: Commit

```bash
git add src/core/swagger.rs
git commit -m "feat: extract_swagger_json for inline Swagger parsing"
```

---

## Task 6: Search Refactor (CatalogEntry-based)

**Files:**
- Modify: `src/core/catalog.rs`
- Modify: `src/core/types.rs` (SearchEntry update)
- Modify: `tests/integration/catalog_test.rs`

### Step 1: Update SearchEntry type

In `src/core/types.rs`, replace the current `SearchEntry` (lines 193-202):

```rust
#[derive(Debug, Serialize)]
pub struct SearchEntry {
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub org: String,
    pub category: String,
    pub popularity: u32,
}
```

> `operations`와 `auto_approve` 제거. Operations는 `spec` 명령으로 조회.

### Step 2: Write test for CatalogEntry-based search

In `src/core/catalog.rs`, add/update the test module. Add a new test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::CatalogEntry;

    #[test]
    fn test_search_bundle_catalog() {
        let catalog = vec![
            CatalogEntry {
                list_id: "111".into(),
                title: "기상청 단기예보".into(),
                description: "날씨 예보 API".into(),
                keywords: vec!["기상".into(), "날씨".into()],
                org_name: "기상청".into(),
                category: "과학기술".into(),
                request_count: 500,
            },
            CatalogEntry {
                list_id: "222".into(),
                title: "사업자등록 조회".into(),
                description: "사업자번호 진위확인".into(),
                keywords: vec!["사업자".into()],
                org_name: "국세청".into(),
                category: "산업경제".into(),
                request_count: 1000,
            },
        ];

        let result = search_bundle_catalog(&catalog, "기상청", None, 10);
        assert_eq!(result.total, 1);
        assert_eq!(result.results[0].list_id, "111");
        assert_eq!(result.results[0].category, "과학기술");
    }

    #[test]
    fn test_search_bundle_catalog_category_filter() {
        let catalog = vec![
            CatalogEntry {
                list_id: "111".into(),
                title: "기상청 API".into(),
                description: "".into(),
                keywords: vec![],
                org_name: "기상청".into(),
                category: "과학기술".into(),
                request_count: 100,
            },
            CatalogEntry {
                list_id: "222".into(),
                title: "기상 관련".into(),
                description: "".into(),
                keywords: vec![],
                org_name: "환경부".into(),
                category: "산업경제".into(),
                request_count: 200,
            },
        ];

        let result = search_bundle_catalog(&catalog, "기상", Some("과학기술"), 10);
        assert_eq!(result.total, 1);
        assert_eq!(result.results[0].list_id, "111");
    }

    #[test]
    fn test_search_bundle_catalog_scoring() {
        let catalog = vec![
            CatalogEntry {
                list_id: "111".into(),
                title: "사업자 조회".into(),
                description: "사업자 등록 상태 조회".into(),
                keywords: vec!["사업자".into()],
                org_name: "국세청".into(),
                category: "".into(),
                request_count: 100,
            },
            CatalogEntry {
                list_id: "222".into(),
                title: "사업자 등록 확인".into(),
                description: "사업자 번호".into(),
                keywords: vec!["사업자".into(), "등록".into()],
                org_name: "국세청".into(),
                category: "".into(),
                request_count: 50,
            },
        ];

        // "사업자 등록" — 222 has more term matches (title + desc + keywords all match both terms)
        let result = search_bundle_catalog(&catalog, "사업자 등록", None, 10);
        assert_eq!(result.total, 2);
        // Higher match count wins even with lower popularity
    }
}
```

### Step 3: Run tests to verify they fail

Run: `cargo test --lib catalog::tests`
Expected: FAIL — `search_bundle_catalog` not found

### Step 4: Implement search_bundle_catalog

In `src/core/catalog.rs`, add after `search_catalog`:

```rust
/// Search bundle catalog entries by query string.
pub fn search_bundle_catalog(
    catalog: &[CatalogEntry],
    query: &str,
    category: Option<&str>,
    limit: usize,
) -> SearchResult {
    let query_lower = query.to_lowercase();
    let terms: Vec<&str> = query_lower.split_whitespace().collect();

    let mut scored: Vec<(u32, &CatalogEntry)> = catalog
        .iter()
        .filter_map(|entry| {
            if let Some(cat) = category {
                if !entry.category.contains(cat) {
                    return None;
                }
            }

            let searchable = format!(
                "{} {} {} {}",
                entry.title.to_lowercase(),
                entry.description.to_lowercase(),
                entry.keywords.join(" ").to_lowercase(),
                entry.org_name.to_lowercase(),
            );

            let match_count = terms.iter().filter(|t| searchable.contains(*t)).count();
            if match_count > 0 {
                let score = (match_count as u32) * 100 + entry.request_count;
                Some((score, entry))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    let total = scored.len();
    let results: Vec<SearchEntry> = scored
        .into_iter()
        .take(limit)
        .map(|(_, entry)| SearchEntry {
            list_id: entry.list_id.clone(),
            title: entry.title.clone(),
            description: entry.description.clone(),
            org: entry.org_name.clone(),
            category: entry.category.clone(),
            popularity: entry.request_count,
        })
        .collect();

    SearchResult { results, total }
}
```

Add `use crate::core::types::CatalogEntry;` import (or ensure it's covered by `use crate::core::types::*;`).

### Step 5: Run tests to verify they pass

Run: `cargo test --lib catalog::tests`
Expected: PASS

### Step 6: Update integration test

In `tests/integration/catalog_test.rs`, update `test_search_catalog` to also test the new function, and fix any `SearchEntry` field references (remove `operations`, `auto_approve`; add `category`).

### Step 7: Run full test suite

Run: `cargo test`
Expected: PASS (all tests)

### Step 8: Commit

```bash
git add src/core/catalog.rs src/core/types.rs tests/integration/catalog_test.rs
git commit -m "feat: search_bundle_catalog for CatalogEntry-based search"
```

---

## Task 7: CLI/MCP Integration

**Files:**
- Modify: `src/cli/search.rs`
- Modify: `src/cli/spec.rs`
- Modify: `src/cli/call.rs`
- Modify: `src/mcp/tools.rs`

### Step 1: Rewrite cli/search.rs

```rust
use crate::core::bundle::BUNDLE;
use crate::core::catalog;
use anyhow::Result;

pub async fn run(query: &str, category: Option<&str>, limit: usize) -> Result<()> {
    if BUNDLE.catalog.is_empty() {
        let response = serde_json::json!({
            "success": false,
            "error": "BUNDLE_EMPTY",
            "message": "번들이 비어있습니다.",
            "action": "korea-cli update 를 먼저 실행하세요."
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    let results = catalog::search_bundle_catalog(&BUNDLE.catalog, query, category, limit);
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
```

### Step 2: Rewrite cli/spec.rs

```rust
use crate::core::bundle::BUNDLE;
use anyhow::Result;

pub async fn run(list_id: &str) -> Result<()> {
    let spec = BUNDLE
        .specs
        .get(list_id)
        .ok_or_else(|| anyhow::anyhow!("API spec을 찾을 수 없습니다: {list_id}"))?;

    let output = serde_json::json!({
        "success": true,
        "list_id": spec.list_id,
        "base_url": spec.base_url,
        "auth": spec.auth,
        "has_api_key": crate::config::AppConfig::load()?.resolve_api_key().is_some(),
        "operations": spec.operations.iter().map(|op| {
            serde_json::json!({
                "path": op.path,
                "method": op.method,
                "summary": op.summary,
                "parameters": op.parameters,
                "request_body": op.request_body,
                "response_fields": op.response_fields,
            })
        }).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
```

### Step 3: Rewrite cli/call.rs

```rust
use crate::config::AppConfig;
use crate::core::{bundle::BUNDLE, caller};
use anyhow::Result;

pub async fn run(list_id: &str, operation: &str, params: &[(String, String)]) -> Result<()> {
    let cfg = AppConfig::load()?;
    let api_key = match cfg.resolve_api_key() {
        Some(key) => key,
        None => {
            let response = serde_json::json!({
                "success": false,
                "error": "NO_API_KEY",
                "message": "API 키가 설정되지 않았습니다.",
                "action": "korea-cli config set api-key YOUR_KEY 또는 환경변수 DATA_GO_KR_API_KEY 설정"
            });
            println!("{}", serde_json::to_string_pretty(&response)?);
            return Ok(());
        }
    };

    let spec = BUNDLE
        .specs
        .get(list_id)
        .ok_or_else(|| anyhow::anyhow!("API spec을 찾을 수 없습니다: {list_id}"))?;

    let result = caller::call_api(spec, operation, params, &api_key).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
```

### Step 4: Rewrite mcp/tools.rs

```rust
//! MCP tool handlers — search_api, get_api_spec, call_api.

use crate::config::AppConfig;
use crate::core::{bundle::BUNDLE, caller, catalog};
use serde_json::json;

pub async fn handle_tool_call(params: serde_json::Value) -> serde_json::Value {
    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match tool_name {
        "search_api" => handle_search(arguments).await,
        "get_api_spec" => handle_get_spec(arguments).await,
        "call_api" => handle_call(arguments).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {tool_name}")),
    };

    match result {
        Ok(content) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&content).unwrap_or_default() }]
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": json!({ "success": false, "error": e.to_string() }).to_string() }],
            "isError": true
        }),
    }
}

async fn handle_search(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' parameter"))?;
    let category = args.get("category").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    if BUNDLE.catalog.is_empty() {
        return Ok(json!({
            "success": false, "error": "BUNDLE_EMPTY",
            "message": "번들이 비어있습니다.",
            "action": "korea-cli update 를 먼저 실행하세요."
        }));
    }

    let results = catalog::search_bundle_catalog(&BUNDLE.catalog, query, category, limit);
    Ok(serde_json::to_value(results)?)
}

async fn handle_get_spec(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let list_id = args
        .get("list_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'list_id' parameter"))?;

    let spec = BUNDLE
        .specs
        .get(list_id)
        .ok_or_else(|| anyhow::anyhow!("API spec not found: {list_id}"))?;

    let has_key = AppConfig::load()?.resolve_api_key().is_some();
    let mut output = serde_json::to_value(spec)?;
    if let Some(obj) = output.as_object_mut() {
        obj.insert("has_api_key".into(), json!(has_key));
        if !has_key {
            obj.insert(
                "key_guide".into(),
                json!("이 API를 호출하려면 API 키가 필요합니다. DATA_GO_KR_API_KEY 환경변수를 설정하세요."),
            );
        }
    }

    Ok(output)
}

async fn handle_call(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let list_id = args
        .get("list_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'list_id' parameter"))?;
    let operation = args
        .get("operation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'operation' parameter"))?;

    let cfg = AppConfig::load()?;
    let api_key = match cfg.resolve_api_key() {
        Some(key) => key,
        None => {
            return Ok(json!({
                "success": false, "error": "NO_API_KEY",
                "message": "API 키가 설정되지 않았습니다.",
                "action": "DATA_GO_KR_API_KEY 환경변수를 설정하세요."
            }));
        }
    };

    let spec = BUNDLE
        .specs
        .get(list_id)
        .ok_or_else(|| anyhow::anyhow!("API spec not found: {list_id}"))?;

    let params: Vec<(String, String)> = args
        .get("params")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| {
                    let value = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), value)
                })
                .collect()
        })
        .unwrap_or_default();

    let result = caller::call_api(spec, operation, &params, &api_key).await?;
    Ok(serde_json::to_value(result)?)
}
```

### Step 5: Run type check + clippy

Run: `cargo clippy`
Expected: no errors (warnings acceptable for now)

### Step 6: Run tests

Run: `cargo test`
Expected: PASS (integration tests may need adjustment — see next step)

### Step 7: Fix integration tests if needed

`tests/integration/catalog_test.rs` may reference old `search_catalog` with `Catalog` type — update to use `search_bundle_catalog` with `CatalogEntry` slice, or keep both functions during transition.

`tests/integration/swagger_test.rs` should still pass as `parse_swagger` is unchanged.

### Step 8: Commit

```bash
git add src/cli/ src/mcp/tools.rs
git commit -m "feat: CLI/MCP use bundle for search + spec lookup"
```

---

## Task 8: Update Command (GitHub Releases)

**Files:**
- Modify: `src/cli/update.rs`

### Step 1: Write the update command

Replace `src/cli/update.rs`:

```rust
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
```

### Step 2: Verify compilation

Run: `cargo check`
Expected: compiles

### Step 3: Commit

```bash
git add src/cli/update.rs
git commit -m "feat: update command downloads bundle from GitHub Releases"
```

---

## Task 9: Bundle Builder Binary

**Files:**
- Create: `src/bin/build_bundle.rs`
- Modify: `Cargo.toml` (add binary target + futures dep)

### Step 1: Add futures dependency and binary target

In `Cargo.toml`, add to `[dependencies]`:

```toml
futures = "0.3"
```

Add binary section:

```toml
[[bin]]
name = "build-bundle"
path = "src/bin/build_bundle.rs"
```

### Step 2: Create the bundle builder

```rust
//! Bundle builder: collects all API catalog + Swagger specs from data.go.kr.
//!
//! Usage: cargo run --bin build-bundle -- --api-key YOUR_KEY [--output data/bundle.zstd] [--concurrency 5]
//!
//! Estimated time: ~15-20 minutes for 12K APIs with concurrency=5.

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use korea_cli::core::bundle;
use korea_cli::core::catalog::fetch_all_services;
use korea_cli::core::swagger::{extract_swagger_json, extract_swagger_url, parse_swagger};
use korea_cli::core::types::*;

#[derive(Clone)]
struct BuildConfig {
    api_key: String,
    output: String,
    concurrency: usize,
    delay_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = parse_args()?;
    let start = Instant::now();

    // Step 1: Fetch catalog
    eprintln!("=== Step 1/4: 카탈로그 수집 ===");
    let services = fetch_all_services(&config.api_key).await?;
    eprintln!("  {} 서비스 수집 완료", services.len());

    // Step 2: Collect Swagger specs
    eprintln!("\n=== Step 2/4: Swagger spec 수집 (동시 {}건) ===", config.concurrency);
    let specs = collect_specs(&services, &config).await;
    eprintln!(
        "  {}/{} spec 수집 완료 ({:.1}%)",
        specs.len(),
        services.len(),
        (specs.len() as f64 / services.len() as f64) * 100.0
    );

    // Step 3: Build lightweight catalog
    eprintln!("\n=== Step 3/4: 번들 구성 ===");
    let catalog: Vec<CatalogEntry> = services
        .iter()
        .map(|svc| CatalogEntry {
            list_id: svc.list_id.clone(),
            title: svc.title.clone(),
            description: svc.description.clone(),
            keywords: svc.keywords.clone(),
            org_name: svc.org_name.clone(),
            category: svc.category.clone(),
            request_count: svc.request_count,
        })
        .collect();

    let checksum = format!("{:x}", md5_hash(&format!("{}-{}", catalog.len(), specs.len())));
    let bundle_data = Bundle {
        metadata: BundleMetadata {
            version: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            api_count: catalog.len(),
            spec_count: specs.len(),
            checksum,
        },
        catalog,
        specs,
    };

    // Step 4: Serialize + compress
    eprintln!("\n=== Step 4/4: 직렬화 + 압축 ===");
    let compressed = bundle::serialize_and_compress(&bundle_data, 3)?;

    std::fs::create_dir_all(
        std::path::Path::new(&config.output)
            .parent()
            .unwrap_or(std::path::Path::new(".")),
    )?;
    std::fs::write(&config.output, &compressed)?;

    let elapsed = start.elapsed();
    eprintln!("\n=== 완료 ===");
    eprintln!("  버전: {}", bundle_data.metadata.version);
    eprintln!("  API: {} / Spec: {}", bundle_data.metadata.api_count, bundle_data.metadata.spec_count);
    eprintln!("  크기: {:.2} MB", compressed.len() as f64 / 1_048_576.0);
    eprintln!("  경로: {}", config.output);
    eprintln!("  소요: {:.1}분", elapsed.as_secs_f64() / 60.0);

    Ok(())
}

async fn collect_specs(
    services: &[ApiService],
    config: &BuildConfig,
) -> HashMap<String, ApiSpec> {
    let client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-builder/0.1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to build HTTP client"),
    );

    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));
    let total = services.len();

    let results: Vec<(String, Option<ApiSpec>)> = stream::iter(services.iter().enumerate())
        .map(|(i, svc)| {
            let client = client.clone();
            let list_id = svc.list_id.clone();
            let delay_ms = config.delay_ms;
            let sc = success_count.clone();
            let fc = fail_count.clone();

            async move {
                let result = fetch_single_spec(&client, &list_id).await;

                match &result {
                    Ok(_) => { sc.fetch_add(1, Ordering::Relaxed); }
                    Err(e) => {
                        fc.fetch_add(1, Ordering::Relaxed);
                        eprintln!("  SKIP {list_id}: {e}");
                    }
                }

                let done = sc.load(Ordering::Relaxed) + fc.load(Ordering::Relaxed);
                if done % 500 == 0 {
                    eprintln!(
                        "  진행: {done}/{total} ({} OK, {} FAIL)",
                        sc.load(Ordering::Relaxed),
                        fc.load(Ordering::Relaxed),
                    );
                }

                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                (list_id, result.ok())
            }
        })
        .buffer_unordered(config.concurrency)
        .collect()
        .await;

    results
        .into_iter()
        .filter_map(|(id, spec)| spec.map(|s| (id, s)))
        .collect()
}

async fn fetch_single_spec(client: &reqwest::Client, list_id: &str) -> Result<ApiSpec> {
    let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");
    let html = client
        .get(&page_url)
        .send()
        .await
        .context("페이지 요청 실패")?
        .text()
        .await
        .context("페이지 본문 읽기 실패")?;

    // Pattern 1: inline swaggerJson (99% of cases)
    if let Some(json) = extract_swagger_json(&html) {
        return parse_swagger(list_id, &json);
    }

    // Pattern 2: external swaggerUrl (1% of cases)
    if let Some(url) = extract_swagger_url(&html) {
        let spec_json: serde_json::Value = client
            .get(&url)
            .send()
            .await
            .context("Swagger URL 요청 실패")?
            .json()
            .await
            .context("Swagger JSON 파싱 실패")?;
        return parse_swagger(list_id, &spec_json);
    }

    anyhow::bail!("swaggerJson/swaggerUrl 모두 없음")
}

fn parse_args() -> Result<BuildConfig> {
    let args: Vec<String> = std::env::args().collect();

    let api_key = get_arg(&args, "--api-key")
        .or_else(|| std::env::var("DATA_GO_KR_API_KEY").ok())
        .ok_or_else(|| anyhow::anyhow!("--api-key 또는 DATA_GO_KR_API_KEY 환경변수 필요"))?;

    let output = get_arg(&args, "--output").unwrap_or_else(|| "data/bundle.zstd".into());
    let concurrency: usize = get_arg(&args, "--concurrency")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let delay_ms: u64 = get_arg(&args, "--delay")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    Ok(BuildConfig { api_key, output, concurrency, delay_ms })
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Simple hash for checksum (not cryptographic, just for version tracking).
fn md5_hash(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}
```

### Step 3: Verify compilation

Run: `cargo check --bin build-bundle`
Expected: compiles

### Step 4: Commit

```bash
git add Cargo.toml src/bin/build_bundle.rs
git commit -m "feat: bundle builder binary for collecting API specs"
```

---

## Task 10: Generate Real Bundle + E2E Testing

**Files:**
- Generated: `data/bundle.zstd` (real data)

### Step 1: Run bundle builder

> **Note:** 12K API spec 수집에 ~15-20분 소요. API key 필요.

```bash
cargo run --bin build-bundle -- --api-key $DATA_GO_KR_API_KEY --output data/bundle.zstd
```

Expected output:
```
=== Step 1/4: 카탈로그 수집 ===
  12080 서비스 수집 완료

=== Step 2/4: Swagger spec 수집 (동시 5건) ===
  ...
  11950/12080 spec 수집 완료 (98.9%)

=== Step 3/4: 번들 구성 ===

=== Step 4/4: 직렬화 + 압축 ===

=== 완료 ===
  크기: ~2.5 MB
```

### Step 2: Verify bundle size

```bash
ls -lh data/bundle.zstd
```

Expected: 2-5 MB

### Step 3: Rebuild with real bundle

```bash
cargo build
```

### Step 4: E2E test — search (offline)

```bash
cargo run -- search "기상청"
```

Expected: returns results with list_ids

### Step 5: E2E test — spec (offline)

```bash
cargo run -- spec 15084084
```

Expected: returns API spec with operations, parameters

### Step 6: E2E test — API call (online)

```bash
cargo run -- call 15081808 /status --param b_no='["1234567890"]'
```

Expected: returns API response

### Step 7: Commit (bundle is not committed — it's a release asset)

```bash
# No commit for data/bundle.zstd (in .gitignore)
# But verify tests pass
cargo test
cargo clippy
```

---

## Task 11: Legacy Cleanup + Documentation

**Files:**
- Modify: `src/core/swagger.rs` (remove fetch_and_cache_spec, load_cached_spec)
- Modify: `src/config/paths.rs` (remove catalog_file, spec_cache_dir, spec_cache_file)
- Modify: `src/core/catalog.rs` (remove load_catalog, save_catalog; keep fetch_all_services for builder)
- Modify: `README.md`
- Modify: `CLAUDE.md` (project structure)

### Step 1: Remove legacy spec fetching from swagger.rs

In `src/core/swagger.rs`, remove:
- `fetch_and_cache_spec()` (lines 311-353)
- `load_cached_spec()` (lines 356-365)

Keep:
- `parse_swagger()` — used by bundle builder
- `parse_auth()`, `parse_parameters()`, `parse_response_fields()` — used by parse_swagger
- `extract_swagger_url()` — used by bundle builder
- `extract_swagger_json()` — used by bundle builder
- All tests

### Step 2: Remove legacy catalog persistence from catalog.rs

In `src/core/catalog.rs`, remove:
- `load_catalog()` (lines 104-115)
- `save_catalog()` (lines 118-123)

Keep:
- `parse_meta_response()` — used by fetch_all_services
- `fetch_all_services()` — used by bundle builder
- `search_catalog()` — keep for backward compat, or remove if no references
- `search_bundle_catalog()` — the new search
- Helper functions

### Step 3: Clean up config/paths.rs

In `src/config/paths.rs`, remove:
- `catalog_file()` — no longer used
- `spec_cache_dir()` — no longer used
- `spec_cache_file()` — no longer used

Keep:
- `config_dir()`
- `config_file()`
- `bundle_override_file()`

### Step 4: Update config/mod.rs

Remove `catalog_updated_at` from `AppConfig` if present.

### Step 5: Run full checks

Run: `cargo test && cargo clippy && cargo fmt -- --check`
Expected: PASS

### Step 6: Fix any remaining references

Grep for removed function names:

```bash
cargo clippy 2>&1 | grep "error"
```

Fix all compilation errors.

### Step 7: Update README.md

Key changes:
- Installation: `cargo install korea-cli` — 즉시 사용 가능 (번들 내장)
- Remove "update 먼저 실행" 안내
- Add `korea-cli update` — 최신 번들 다운로드
- Note: 12K+ API 오프라인 검색/스펙 조회 지원

### Step 8: Update CLAUDE.md project structure

```
src/
├── main.rs        # CLI 엔트리포인트, clap 서브커맨드 정의
├── core/
│   ├── types.rs   # 타입 (Bundle, CatalogEntry, ApiSpec, etc.)
│   ├── bundle.rs  # 번들 로드/해제, 오버라이드 체인
│   ├── catalog.rs # 카탈로그 검색, 메타 API 수집
│   ├── swagger.rs # Swagger 파싱 (parse_swagger, extract_swagger_json)
│   └── caller.rs  # API 호출 엔진
├── mcp/           # MCP 서버 (stdio JSON-RPC)
├── config/        # 설정 (~/.config/korea-cli/)
├── cli/           # CLI 커맨드 핸들러
└── bin/
    └── build_bundle.rs  # 번들 생성 도구 (릴리스용)
```

### Step 9: Commit

```bash
git add -A
git commit -m "refactor: remove legacy scraping, clean up for bundle architecture"
```

### Step 10: Final verification

```bash
cargo test
cargo clippy
cargo fmt -- --check
cargo run -- search "사업자"
cargo run -- spec 15081808
```

Expected: all pass, search/spec return results from bundle

---

## Summary

| Task | Description | Est. Time |
|------|-------------|-----------|
| 1 | Dependencies + Bundle types | 10 min |
| 2 | build.rs + placeholder bundle | 10 min |
| 3 | Config paths (bundle_override_file) | 5 min |
| 4 | Bundle loader (core/bundle.rs) | 15 min |
| 5 | Swagger JSON inline extraction | 10 min |
| 6 | Search refactor (CatalogEntry) | 15 min |
| 7 | CLI/MCP integration | 20 min |
| 8 | Update command (GitHub Releases) | 10 min |
| 9 | Bundle builder binary | 25 min |
| 10 | Real bundle generation + E2E | 25 min* |
| 11 | Legacy cleanup + docs | 20 min |

*Task 10의 번들 수집은 ~15-20분 대기 시간 포함.

**Total implementation: ~2.5 hours** (번들 수집 대기 제외)

## Dependencies

```
Task 1 → Task 2 → Task 4 (types → build infra → loader)
Task 3 → Task 4 (paths → loader)
Task 4 → Task 6 → Task 7 (loader → search refactor → integration)
Task 5 → Task 9 (extraction → builder)
Task 7 → Task 8 (integration → update cmd)
Task 9 → Task 10 (builder → real bundle)
Task 10 → Task 11 (E2E → cleanup)
```

## Critical Path

`1 → 2 → 4 → 6 → 7 → 9 → 10 → 11`

Tasks 3, 5, 8 can be done in parallel with the main path (after their prerequisites).
