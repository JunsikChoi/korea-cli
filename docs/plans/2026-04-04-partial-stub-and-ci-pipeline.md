# PartialStub + CI 수집 파이프라인 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 부분 성공 Gateway API를 PartialStub으로 명시 분류하고, CI 크론으로 자동 수집/retry/배포하며, `korea-cli update`로 번들을 다운로드하는 파이프라인을 구축한다.

**Architecture:** SpecStatus에 PartialStub variant를 추가하고 ClassificationHints에 is_partial 필드를 넣어 classify() 경로를 확장한다. build_bundle.rs에서 실패 operation을 FailedOp/ErrorType으로 분류하여 failed_ops.json에 출력하고, --retry-stubs 플래그로 list_id 단위 재수집을 지원한다. GitHub Actions 크론 워크플로우가 전체 파이프라인을 자동화한다.

**Tech Stack:** Rust (postcard, zstd, reqwest, tokio, serde, clap), GitHub Actions, gh CLI

---

## Task 1: SpecStatus::PartialStub 추가 + classify() 확장

**Files:**
- Modify: `src/core/types.rs:43` (CURRENT_SCHEMA_VERSION)
- Modify: `src/core/types.rs:64-71` (SpecStatus enum)
- Modify: `src/core/types.rs:74-80` (ClassificationHints)
- Modify: `src/core/types.rs:82-127` (is_callable, user_message, classify)
- Modify: `src/core/types.rs:310-548` (tests)

**Step 1: Write failing tests**

`src/core/types.rs`의 `#[cfg(test)] mod tests` 안에 다음 테스트를 추가한다 (기존 테스트 블록 마지막, 547행 `}` 직전에 삽입):

```rust
    #[test]
    fn test_partial_stub_is_callable() {
        assert!(SpecStatus::PartialStub.is_callable());
    }

    #[test]
    fn test_partial_stub_user_message() {
        let msg = SpecStatus::PartialStub.user_message();
        assert!(msg.contains("일부"));
        assert!(msg.contains("operation"));
    }

    #[test]
    fn test_classify_partial_stub() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                has_spec: true,
                is_partial: true,
                endpoint_url: "https://apis.data.go.kr/x",
                ..Default::default()
            }),
            SpecStatus::PartialStub,
        );
    }

    #[test]
    fn test_classify_partial_stub_link_api_takes_priority() {
        // LINK API는 is_partial보다 우선
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                has_spec: true,
                is_partial: true,
                is_link_api: true,
                endpoint_url: "https://apis.data.go.kr/x",
                ..Default::default()
            }),
            SpecStatus::External,
        );
    }

    #[test]
    fn test_partial_stub_postcard_roundtrip() {
        let status = SpecStatus::PartialStub;
        let bytes = postcard::to_allocvec(&status).unwrap();
        let decoded: SpecStatus = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, status);
    }

    #[test]
    fn test_partial_stub_postcard_index() {
        // PartialStub은 variant index 6이어야 함 (0-indexed, 끝에 추가)
        let bytes = postcard::to_allocvec(&SpecStatus::PartialStub).unwrap();
        assert_eq!(bytes[0], 6);
    }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib -- types::tests::test_partial_stub 2>&1 | head -30`
Expected: FAIL — `PartialStub` variant 미존재 컴파일 에러

**Step 3: Implement SpecStatus::PartialStub + ClassificationHints.is_partial**

`src/core/types.rs`에서:

1. Line 43 — schema version bump:
```rust
pub const CURRENT_SCHEMA_VERSION: u32 = 3;
```

2. Lines 64-71 — PartialStub variant 추가 (반드시 끝에):
```rust
pub enum SpecStatus {
    Available,
    Skeleton,
    HtmlOnly,
    External,
    CatalogOnly,
    Unsupported,
    PartialStub,  // 반드시 끝에 — postcard varint 순서 보존
}
```

3. Lines 74-80 — is_partial 필드 추가:
```rust
pub struct ClassificationHints<'a> {
    pub has_spec: bool,
    pub is_skeleton: bool,
    pub endpoint_url: &'a str,
    pub is_link_api: bool,
    pub is_partial: bool,
}
```

4. Lines 83-85 — is_callable 확장:
```rust
    pub fn is_callable(&self) -> bool {
        matches!(self, Self::Available | Self::PartialStub)
    }
```

5. Lines 87-96 — user_message에 PartialStub arm 추가:
```rust
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Available => "API spec 사용 가능",
            Self::Skeleton => "Swagger spec이 비어있습니다. 외부 서비스 페이지에서 확인하세요.",
            Self::HtmlOnly => "spec 파싱 준비 중입니다. endpoint URL을 참고하세요.",
            Self::External => "외부 포탈에서 제공하는 API입니다.",
            Self::CatalogOnly => "카탈로그 정보만 있습니다.",
            Self::Unsupported => "REST가 아닌 프로토콜(WMS/WFS 등)입니다.",
            Self::PartialStub => "일부 operation만 수집됨 — 존재하는 operation은 호출 가능, 누락분은 다음 업데이트에서 복구 예정",
        }
    }
```

6. Lines 99-127 — classify()에 is_partial 분기 추가 (is_link_api 바로 다음):
```rust
    pub fn classify(hints: &ClassificationHints) -> Self {
        if hints.is_link_api {
            return Self::External;
        }
        // is_partial이면 has_spec 여부와 무관하게 PartialStub
        // (빈 ops로 spec이 삽입 안 된 경우도 스펙 의도상 PartialStub)
        if hints.is_partial {
            return Self::PartialStub;
        }
        if hints.has_spec {
            return Self::Available;
        }
        // ... 나머지 기존 로직 동일
    }
```

**Step 4: Fix existing tests that need updating**

기존 테스트에서 `ClassificationHints`를 `..Default::default()`로 생성하는 것들은 `is_partial`이 `false`로 기본값되어 변경 불필요. 단, `test_spec_status_is_callable`에 PartialStub assertion 추가:

```rust
    #[test]
    fn test_spec_status_is_callable() {
        assert!(SpecStatus::Available.is_callable());
        assert!(SpecStatus::PartialStub.is_callable());
        assert!(!SpecStatus::Skeleton.is_callable());
        assert!(!SpecStatus::HtmlOnly.is_callable());
        assert!(!SpecStatus::External.is_callable());
        assert!(!SpecStatus::CatalogOnly.is_callable());
        assert!(!SpecStatus::Unsupported.is_callable());
    }
```

`test_spec_status_postcard_roundtrip`에 PartialStub 추가:
```rust
        let statuses = [
            SpecStatus::Available,
            SpecStatus::Skeleton,
            SpecStatus::HtmlOnly,
            SpecStatus::External,
            SpecStatus::CatalogOnly,
            SpecStatus::Unsupported,
            SpecStatus::PartialStub,
        ];
```

`test_bundle_postcard_roundtrip`과 `test_bundle_zstd_roundtrip`의 `make_test_metadata()`는 `CURRENT_SCHEMA_VERSION`을 사용하므로 자동 업데이트.

**Step 5: Run tests to verify they pass**

Run: `cargo test --lib -- types::tests 2>&1 | tail -20`
Expected: ALL tests PASS

**Step 6: Lint check**

Run: `cargo clippy --lib 2>&1 | tail -10`
Expected: No errors

**Step 7: Commit**

```bash
git add src/core/types.rs
git commit -m "feat: SpecStatus::PartialStub 추가 + classify() 확장

CURRENT_SCHEMA_VERSION 2→3. ClassificationHints에 is_partial 필드 추가.
PartialStub은 is_callable()=true — 존재하는 operation은 호출 가능."
```

---

## Task 2: FailedOp/ErrorType 타입 + SpecResult 확장

**Files:**
- Modify: `src/bin/build_bundle.rs:20-42` (BuildConfig, SpecResult)

**Step 1: Write the types**

`src/bin/build_bundle.rs`에서 `SpecResult` enum 위에 에러 타입을 추가하고, SpecResult를 확장한다.

`BuildConfig` 뒤 (line 28 뒤), `SpecResult` 전에:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
enum ErrorType {
    NetworkTimeout,
    RateLimited,
    BodyReadError,
    ParseError,
    ConnectionError,  // DNS 실패, SSL 에러, 연결 리셋 등
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FailedOp {
    list_id: String,
    seq_no: String,
    op_name: String,
    error_type: ErrorType,
    error_message: String,
}
```

파일 상단 imports에 `use serde::{Serialize, Deserialize};` 추가 (이미 `use korea_cli::core::types::*;`로 re-export된 Serialize/Deserialize가 있으면 확인, 없으면 직접 추가).

`SpecResult` enum 확장:

```rust
#[derive(Debug)]
enum SpecResult {
    Spec {
        spec: Box<ApiSpec>,
        is_gateway: bool,
        is_partial: bool,
        failed_ops: Vec<FailedOp>,
    },
    Bail { reason: String, failed_ops: Vec<FailedOp> },
    ExternalLink { url: Option<String> },
}
```

`BuildConfig`에 `retry_timeout_secs` 추가:

```rust
#[derive(Clone)]
struct BuildConfig {
    api_key: String,
    output: String,
    concurrency: usize,
    delay_ms: u64,
    ajax_concurrency: usize,
    ajax_delay_ms: u64,
    retry_timeout_secs: u64,
}
```

**Step 2: timeout_secs 파라미터를 fetch_single_spec/fetch_gateway_spec에 전파**

`fetch_gateway_spec`은 하드코딩된 15초 timeout을 사용한다 (line 352). 이를 파라미터로 변경:

`fetch_gateway_spec` 시그니처에 `timeout_secs: u64` 추가:
```rust
async fn fetch_gateway_spec(
    list_id: &str,
    page_info: &korea_cli::core::html_parser::PageInfo,
    ajax_semaphore: &tokio::sync::Semaphore,
    ajax_delay_ms: u64,
    timeout_secs: u64,  // 추가
) -> SpecResult {
```

ajax_client timeout 변경 (line 352):
```rust
        .timeout(std::time::Duration::from_secs(timeout_secs))
```

`fetch_single_spec`에도 `timeout_secs: u64` 파라미터 추가:
```rust
async fn fetch_single_spec(
    client: &reqwest::Client,
    list_id: &str,
    ajax_semaphore: &tokio::sync::Semaphore,
    ajax_delay_ms: u64,
    timeout_secs: u64,  // 추가
) -> SpecResult {
    // ... Pattern 3 호출 시:
    return fetch_gateway_spec(list_id, info, ajax_semaphore, ajax_delay_ms, timeout_secs).await;
}
```

`collect_specs`에서 호출 시 — 클로저에 `timeout_secs` 캡처 필수:
```rust
let timeout_secs = config.retry_timeout_secs;  // collect_specs 상단에서 바인딩
// 클로저 내부:
let ts = timeout_secs;  // 클로저 캡처를 위한 복사
async move {
    let result = fetch_single_spec(&client, &list_id, &ajax_sem, ajax_delay_ms, ts).await;
    // ... 기존 로직 동일
}
```

**Step 3: Fix compilation — SpecResult 사용 부분 업데이트**

`collect_specs` 내 `SpecResult::Spec` 매칭 부분들을 업데이트:

Line 213 (match arm in collect_specs):
```rust
SpecResult::Spec { is_gateway, .. } => {
```
— 이미 `..`로 나머지를 무시하므로 변경 불필요.

`fetch_single_spec` 내 Pattern 1, 2 반환 (lines 296-298, 319-321):
```rust
SpecResult::Spec {
    spec: Box::new(spec),
    is_gateway: false,
    is_partial: false,
    failed_ops: vec![],
}
```

`fetch_gateway_spec` 반환 (line 446-449) — 다음 Task에서 실제 partial 로직 구현. 지금은 임시:
```rust
SpecResult::Spec {
    spec: Box::new(spec),
    is_gateway: true,
    is_partial: false,
    failed_ops: vec![],
}
```

**기존 `fetch_single_spec`의 모든 `SpecResult::Bail` 반환에 `failed_ops: vec![]` 추가:**
```rust
SpecResult::Bail {
    reason: format!("..."),
    failed_ops: vec![],
}
```

`parse_args`에서 기본값:
```rust
Ok(BuildConfig {
    // ...기존 필드들...
    retry_timeout_secs: 15,  // main build 기본값
})
```

**Step 3: Verify compilation**

Run: `cargo check --bin build-bundle 2>&1 | tail -10`
Expected: OK (warnings 허용)

**Step 4: Commit**

```bash
git add src/bin/build_bundle.rs
git commit -m "feat: FailedOp/ErrorType 타입 추가 + SpecResult 확장

build_bundle에 is_partial, failed_ops 필드 추가.
에러 분류용 ErrorType enum: NetworkTimeout/RateLimited/BodyReadError/ParseError."
```

---

## Task 3: fetch_gateway_spec 에러 분류 + partial 감지

**Files:**
- Modify: `src/bin/build_bundle.rs:343-457` (fetch_gateway_spec)

**Step 1: fetch_gateway_spec에서 에러를 FailedOp로 수집**

`fetch_gateway_spec` 함수의 operation 루프 (lines 394-430)를 수정하여 실패 operation을 `FailedOp`로 기록한다.

기존:
```rust
    let mut parsed_ops = Vec::new();
    let total_ops = page_info.operations.len();

    for op in &page_info.operations {
        // ... semaphore + AJAX ...
        match ajax_result {
            Ok(resp) => match resp.text().await {
                Ok(html) => match parse_operation_detail(&html) {
                    Ok(detail) => parsed_ops.push(detail),
                    Err(e) => eprintln!("  PARTIAL SKIP {list_id}/{}: parse: {e}", op.seq_no),
                },
                Err(e) => eprintln!("  PARTIAL SKIP {list_id}/{}: body: {e}", op.seq_no),
            },
            Err(e) => eprintln!("  PARTIAL SKIP {list_id}/{}: {e}", op.seq_no),
        }
    }
```

변경 후:
```rust
    let mut parsed_ops = Vec::new();
    let mut failed_ops = Vec::new();
    let total_ops = page_info.operations.len();

    for op in &page_info.operations {
        // ... semaphore + AJAX ... (기존 동일)

        match ajax_result {
            Ok(resp) => match resp.text().await {
                Ok(html) => match parse_operation_detail(&html) {
                    Ok(detail) => parsed_ops.push(detail),
                    Err(e) => {
                        eprintln!("  PARTIAL SKIP {list_id}/{}: parse: {e}", op.seq_no);
                        failed_ops.push(FailedOp {
                            list_id: list_id.to_string(),
                            seq_no: op.seq_no.clone(),
                            op_name: op.name.clone(),
                            error_type: ErrorType::ParseError,
                            error_message: e.to_string(),
                        });
                    }
                },
                Err(e) => {
                    eprintln!("  PARTIAL SKIP {list_id}/{}: body: {e}", op.seq_no);
                    failed_ops.push(FailedOp {
                        list_id: list_id.to_string(),
                        seq_no: op.seq_no.clone(),
                        op_name: op.name.clone(),
                        error_type: ErrorType::BodyReadError,
                        error_message: e.to_string(),
                    });
                }
            },
            Err(e) => {
                let error_type = if e.is_timeout() {
                    ErrorType::NetworkTimeout
                } else if e.is_status() && e.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) {
                    ErrorType::RateLimited
                } else if e.is_connect() {
                    ErrorType::ConnectionError  // DNS 실패, SSL, 연결 리셋
                } else {
                    ErrorType::NetworkTimeout  // 기타 네트워크 에러
                };
                eprintln!("  PARTIAL SKIP {list_id}/{}: {e}", op.seq_no);
                failed_ops.push(FailedOp {
                    list_id: list_id.to_string(),
                    seq_no: op.seq_no.clone(),
                    op_name: op.name.clone(),
                    error_type,
                    error_message: e.to_string(),
                });
            }
        }
    }
```

반환값 변경 — 전부 실패 (lines 432-436) — failed_ops도 함께 반환:
```rust
    if parsed_ops.is_empty() {
        return SpecResult::Bail {
            reason: format!("Gateway AJAX 전부 실패 (0/{total_ops} ops)"),
            failed_ops,
        };
    }
```

성공 반환 (lines 445-456):
```rust
    let is_partial = !failed_ops.is_empty();
    if is_partial {
        eprintln!(
            "  PARTIAL: {}/{total_ops} operations ({list_id})",
            parsed_ops.len()
        );
    }

    match build_api_spec(list_id, &parsed_ops) {
        Some(spec) => SpecResult::Spec {
            spec: Box::new(spec),
            is_gateway: true,
            is_partial,
            failed_ops,
        },
        None => SpecResult::Bail {
            reason: format!(
                "Gateway build_api_spec 실패 ({}/{total_ops} ops)",
                parsed_ops.len()
            ),
            failed_ops,
        },
    }
```

**Step 2: Verify compilation**

Run: `cargo check --bin build-bundle 2>&1 | tail -10`
Expected: OK

**Step 3: Commit**

```bash
git add src/bin/build_bundle.rs
git commit -m "feat: fetch_gateway_spec 에러 분류 — FailedOp 수집 + is_partial 감지

AJAX 실패를 NetworkTimeout/RateLimited/BodyReadError/ParseError로 분류.
부분 성공 시 is_partial=true + failed_ops 반환."
```

---

## Task 4: main()에서 partial_ids 추적 + failed_ops.json 출력

**Files:**
- Modify: `src/bin/build_bundle.rs:44-179` (main 함수)

**Step 1: main()에서 PartialStub 분류 + failed_ops.json 출력**

`main()` 함수의 결과 수집 부분 (lines 61-83)을 수정:

```rust
    let mut specs: HashMap<String, ApiSpec> = HashMap::new();
    let mut skeleton_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut link_api_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut external_urls: HashMap<String, String> = HashMap::new();
    let mut partial_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut all_failed_ops: Vec<FailedOp> = Vec::new();

    for (id, result) in all_results {
        match result {
            SpecResult::Spec { spec, is_partial, failed_ops, .. } => {
                if is_partial {
                    partial_ids.insert(id.clone());
                    all_failed_ops.extend(failed_ops);
                }
                if spec.operations.is_empty() {
                    // 빈 operations는 bundle.specs에 삽입하지 않음
                    // (partial이어도 빈 spec은 call 시 매칭 실패를 유발하므로)
                    if !is_partial {
                        skeleton_ids.insert(id);
                    }
                    // partial + 빈 ops → partial_ids에만 기록 (PartialStub 분류)
                } else {
                    specs.insert(id, *spec);
                }
            }
            SpecResult::ExternalLink { url } => {
                link_api_ids.insert(id.clone());
                if let Some(u) = url {
                    external_urls.insert(id, u);
                }
            }
            SpecResult::Bail { failed_ops, .. } => {
                all_failed_ops.extend(failed_ops);
            }
        }
    }
```

로그에 partial 정보 추가 (기존 eprintln 뒤):
```rust
    if !partial_ids.is_empty() {
        eprintln!("  partial: {} APIs ({} failed ops)", partial_ids.len(), all_failed_ops.len());
    }
```

ClassificationHints에 is_partial 전달 (lines 111-116):
```rust
            let spec_status = SpecStatus::classify(&ClassificationHints {
                has_spec: specs.contains_key(&svc.list_id),
                is_skeleton: skeleton_ids.contains(&svc.list_id),
                endpoint_url: &effective_url,
                is_link_api: link_api_ids.contains(&svc.list_id),
                is_partial: partial_ids.contains(&svc.list_id),
            });
```

failed_ops.json 출력 (Step 4/4 직렬화 전, 또는 파일 출력 직후):
```rust
    // failed_ops.json 출력
    if !all_failed_ops.is_empty() {
        let failed_ops_path = std::path::Path::new(&config.output)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("failed_ops.json");
        let failed_json = serde_json::to_string_pretty(&all_failed_ops)?;
        std::fs::write(&failed_ops_path, &failed_json)?;
        eprintln!("  failed_ops: {} → {}", all_failed_ops.len(), failed_ops_path.display());
    }
```

**Step 2: Verify compilation**

Run: `cargo check --bin build-bundle 2>&1 | tail -10`
Expected: OK

**Step 3: Commit**

```bash
git add src/bin/build_bundle.rs
git commit -m "feat: main()에서 PartialStub 분류 + failed_ops.json 출력

partial_ids로 부분 성공 API 추적, ClassificationHints에 is_partial 전달.
실패 operation을 data/failed_ops.json에 기록."
```

---

## Task 5: --retry-stubs 플래그 구현

**Files:**
- Modify: `src/bin/build_bundle.rs:459-488` (parse_args)
- Modify: `src/bin/build_bundle.rs:44-179` (main — retry 분기 추가)

**Step 1: parse_args에 --retry-stubs, --max-retry-time 추가**

```rust
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
    let ajax_concurrency: usize = get_arg(&args, "--ajax-concurrency")
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let ajax_delay_ms: u64 = get_arg(&args, "--ajax-delay")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    let retry_stubs = get_arg(&args, "--retry-stubs");
    let max_retry_time: u64 = get_arg(&args, "--max-retry-time")
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);

    let retry_timeout_secs: u64 = if retry_stubs.is_some() { 30 } else { 15 };

    Ok(BuildConfig {
        api_key,
        output,
        concurrency,
        delay_ms,
        ajax_concurrency,
        ajax_delay_ms,
        retry_timeout_secs,
        retry_stubs,
        max_retry_time,
    })
}
```

`BuildConfig`에 필드 추가:
```rust
#[derive(Clone)]
struct BuildConfig {
    api_key: String,
    output: String,
    concurrency: usize,
    delay_ms: u64,
    ajax_concurrency: usize,
    ajax_delay_ms: u64,
    retry_timeout_secs: u64,
    retry_stubs: Option<String>,   // --retry-stubs <failed_ops.json path>
    max_retry_time: u64,           // --max-retry-time <seconds>
}
```

**Step 2: retry_stubs 실행 함수 구현**

main() 상단에 `--retry-stubs` 분기 추가. main 시작 직후 (config 파싱 뒤):

```rust
    if let Some(ref failed_ops_path) = config.retry_stubs {
        return run_retry(&config, failed_ops_path).await;
    }
```

main() 함수 뒤에 `run_retry` 함수 추가:

```rust
async fn run_retry(config: &BuildConfig, failed_ops_path: &str) -> Result<()> {
    let start = Instant::now();
    eprintln!("=== Retry: {} 읽기 ===", failed_ops_path);

    // 1. failed_ops.json 읽기
    let failed_json = std::fs::read_to_string(failed_ops_path)
        .with_context(|| format!("failed_ops.json 읽기 실패: {}", failed_ops_path))?;
    let failed_ops: Vec<FailedOp> = serde_json::from_str(&failed_json)?;

    // ParseError는 재시도 불가 — 제외
    let retryable: Vec<&FailedOp> = failed_ops.iter()
        .filter(|op| !matches!(op.error_type, ErrorType::ParseError))
        .collect();

    // 고유 list_id 추출
    let mut retry_ids: Vec<String> = retryable.iter()
        .map(|op| op.list_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    retry_ids.sort();

    eprintln!("  {} failed ops → {} retryable → {} unique list_ids",
        failed_ops.len(), retryable.len(), retry_ids.len());

    if retry_ids.is_empty() {
        eprintln!("  재시도 대상 없음 (ParseError만 존재)");
        return Ok(());
    }

    // 2. 기존 번들 로드
    let bundle_bytes = std::fs::read(&config.output)
        .with_context(|| format!("기존 번들 읽기 실패: {}", config.output))?;
    let mut bundle_data: Bundle = bundle::decompress_and_deserialize(&bundle_bytes)?;

    // 에러 타입별 최대 딜레이 결정
    let has_rate_limited = retryable.iter().any(|op| matches!(op.error_type, ErrorType::RateLimited));

    let delays: &[u64] = if has_rate_limited {
        &[60, 120, 300]
    } else {
        &[2, 8, 30]
    };
    let max_retries = 3usize;

    // 3. list_id별 재수집 — 공유 client 재사용 (TLS 재협상 절감)
    let retry_client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-builder/0.1.0")
            .timeout(std::time::Duration::from_secs(config.retry_timeout_secs))
            .build()?,
    );
    let ajax_semaphore = Arc::new(tokio::sync::Semaphore::new(config.ajax_concurrency));
    let mut success_count = 0usize;
    let mut still_partial = 0usize;

    for (i, list_id) in retry_ids.iter().enumerate() {
        // 최대 실행 시간 체크
        if start.elapsed().as_secs() > config.max_retry_time {
            let remaining = retry_ids.len() - i;
            eprintln!("  MAX_RETRY_TIME({}s) 초과 — 남은 {} APIs skip",
                config.max_retry_time, remaining);
            still_partial += remaining;
            break;
        }

        eprintln!("  [{}/{}] retry: {}", i + 1, retry_ids.len(), list_id);

        let mut succeeded = false;
        for attempt in 0..max_retries {
            let result = fetch_single_spec(
                &retry_client, list_id, &ajax_semaphore, config.ajax_delay_ms, config.retry_timeout_secs,
            ).await;

            match result {
                SpecResult::Spec { spec, is_partial, .. } => {
                    // 기존 spec과 merge: 기존 operation 보존 + 새 operation 추가/덮어쓰기
                    if let Some(existing) = bundle_data.specs.get(list_id) {
                        let merged = merge_operations(existing, &spec);
                        bundle_data.specs.insert(list_id.clone(), merged);
                    } else {
                        bundle_data.specs.insert(list_id.clone(), *spec);
                    }

                    if is_partial {
                        still_partial += 1;
                        eprintln!("    여전히 부분 성공 (attempt {})", attempt + 1);
                    } else {
                        succeeded = true;
                        success_count += 1;
                        eprintln!("    완전 성공!");
                    }
                    break;
                }
                SpecResult::Bail { reason, .. } => {
                    if attempt < max_retries - 1 {
                        let delay = delays.get(attempt).copied().unwrap_or(30);
                        eprintln!("    attempt {}: {} — {}s 대기", attempt + 1, reason, delay);
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    } else {
                        eprintln!("    {}회 시도 실패: {}", max_retries, reason);
                        still_partial += 1;
                    }
                }
                SpecResult::ExternalLink { .. } => {
                    eprintln!("    LINK API — skip");
                    break;
                }
            }
        }

        // list_id 간 딜레이 — rate limit 재발 방지
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // catalog에서 PartialStub → Available 승격 (완전 성공 시)
        if succeeded {
            if let Some(entry) = bundle_data.catalog.iter_mut().find(|e| e.list_id == *list_id) {
                entry.spec_status = SpecStatus::Available;
            }
        }
    }

    // 4. 메타데이터 갱신 + 번들 재직렬화 + 저장
    bundle_data.metadata.spec_count = bundle_data.specs.len();
    bundle_data.metadata.checksum = format!(
        "{:x}",
        md5_hash(&format!("{}-{}", bundle_data.metadata.api_count, bundle_data.metadata.spec_count))
    );
    let compressed = bundle::serialize_and_compress(&bundle_data, 3)?;
    std::fs::write(&config.output, &compressed)?;

    let elapsed = start.elapsed();
    eprintln!("\n=== Retry 완료 ===");
    eprintln!("  성공: {}/{}", success_count, retry_ids.len());
    eprintln!("  여전히 partial: {}", still_partial);
    eprintln!("  소요: {:.1}초", elapsed.as_secs_f64());

    Ok(())
}

/// 기존 spec의 operation + retry 결과의 operation을 합집합
/// path + method 쌍으로 identity를 판별 (동일 path에 GET/POST 공존 가능)
fn merge_operations(existing: &ApiSpec, new_spec: &ApiSpec) -> ApiSpec {
    let mut merged = new_spec.clone();
    // 기존에만 있는 operation을 보존
    for existing_op in &existing.operations {
        let dominated = merged.operations.iter().any(|op| {
            op.path == existing_op.path
                && std::mem::discriminant(&op.method) == std::mem::discriminant(&existing_op.method)
        });
        if !dominated {
            merged.operations.push(existing_op.clone());
        }
    }
    merged
}
```

**Step 3: `use anyhow::Context;` import 추가**

`run_retry`에서 `with_context`를 사용하므로 파일 상단 imports에 추가:

```rust
use anyhow::{Context, Result};
```

**Step 4: Verify compilation**

Run: `cargo check --bin build-bundle 2>&1 | tail -10`
Expected: OK

**Step 5: Commit**

```bash
git add src/bin/build_bundle.rs
git commit -m "feat: --retry-stubs 플래그 — list_id 단위 재수집 + merge

failed_ops.json 읽어 retryable list_id 추출 후 fetch_single_spec 재실행.
에러타입별 딜레이, max-retry-time 제한, operation 합집합 merge."
```

---

## Task 6: caller.rs + mcp/tools.rs — PartialStub 안내 메시지

**Files:**
- Modify: `src/cli/call.rs:5-26`
- Modify: `src/mcp/tools.rs:49-93` (handle_get_spec)
- Modify: `src/mcp/tools.rs:95-156` (handle_call)

**Step 1: cli/call.rs — PartialStub인데 spec이 있는 경우 처리**

현재 `call.rs`는 `BUNDLE.specs.contains_key(list_id)`로만 체크한다. PartialStub API는 specs에 존재하므로 호출 자체는 정상 작동한다. 그러나 존재하지 않는 operation을 요청했을 때 안내가 필요하다.

`src/cli/call.rs` — `caller::call_api` 호출 결과 반환 전에 PartialStub 주석 추가:

```rust
pub async fn run(list_id: &str, operation: &str, params: &[(String, String)]) -> Result<()> {
    // Check spec availability before attempting call
    if !BUNDLE.specs.contains_key(list_id) {
        // ... 기존 코드 동일 ...
    }

    // PartialStub 안내: spec이 있지만 요청한 operation이 없을 수 있음
    let entry = BUNDLE.catalog.iter().find(|e| e.list_id == list_id);
    let is_partial = entry.map_or(false, |e| e.spec_status == crate::core::types::SpecStatus::PartialStub);

    let spec = BUNDLE.specs.get(list_id).unwrap();

    // 요청한 operation이 spec에 없고 PartialStub이면 안내
    let has_operation = spec.operations.iter().any(|op| op.path == operation || op.summary == operation);
    if !has_operation && is_partial {
        let response = serde_json::json!({
            "success": false,
            "list_id": list_id,
            "spec_status": "PartialStub",
            "message": "이 API는 일부 operation만 수집됨 — `korea-cli update`로 최신 번들을 받으면 추가 operation이 포함될 수 있습니다",
            "available_operations": spec.operations.iter().map(|op| &op.path).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    // ... 기존 API 키 체크 + call 로직 동일 ...
```

**Step 2: mcp/tools.rs — handle_get_spec에서 PartialStub 표시**

`handle_get_spec` (line 56-73) — spec이 존재하는 경우 SpecStatus를 항상 Available로 하드코딩하고 있다. PartialStub spec도 specs에 존재하므로, 카탈로그에서 실제 status를 조회해야 한다:

```rust
    if let Some(spec) = BUNDLE.specs.get(list_id) {
        let has_key = AppConfig::load()?.resolve_api_key().is_some();
        let entry = BUNDLE.catalog.iter().find(|e| e.list_id == list_id);
        let spec_status = entry.map_or(
            crate::core::types::SpecStatus::Available,
            |e| e.spec_status,
        );

        let mut output = serde_json::to_value(spec)?;
        if let Some(obj) = output.as_object_mut() {
            obj.insert("success".into(), json!(true));
            obj.insert("spec_status".into(), serde_json::to_value(spec_status).unwrap());
            obj.insert("has_api_key".into(), json!(has_key));
            if spec_status == crate::core::types::SpecStatus::PartialStub {
                obj.insert("partial_note".into(), json!(spec_status.user_message()));
            }
            if !has_key {
                obj.insert(
                    "key_guide".into(),
                    json!("이 API를 호출하려면 API 키가 필요합니다. DATA_GO_KR_API_KEY 환경변수를 설정하세요."),
                );
            }
        }
        return Ok(output);
    }
```

**Step 3: Verify compilation**

Run: `cargo check --lib --bins 2>&1 | tail -10`
Expected: OK

**Step 4: Lint**

Run: `cargo clippy --lib --bins 2>&1 | tail -10`
Expected: No errors

**Step 5: Commit**

```bash
git add src/cli/call.rs src/mcp/tools.rs
git commit -m "feat: PartialStub 안내 메시지 — caller/MCP에서 부분 수집 상태 표시

call: 누락 operation 요청 시 PartialStub 안내 + available_operations 목록.
MCP get_spec: 카탈로그에서 실제 spec_status 조회, PartialStub에 partial_note 추가."
```

---

## Task 7: update.rs — schema_version 검증 추가

**Files:**
- Modify: `src/cli/update.rs:7-51`

**Step 1: Write failing test**

`src/cli/update.rs`에는 별도 테스트 모듈이 없고, schema_version 체크는 런타임 로직이다. 대신 types.rs에 기존 `test_old_schema_bundle_fails_deserialization`이 있으므로 schema_version 검증 로직만 구현한다.

**Step 2: update.rs에 schema_version 검증 추가**

`src/cli/update.rs` — `decompress_and_deserialize` 성공 후, schema_version 체크 추가:

```rust
use anyhow::{Context, Result};
use korea_cli::core::types::CURRENT_SCHEMA_VERSION;

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

    // 임시 파일에 저장 (atomic 교체 위해)
    let path = crate::config::paths::bundle_override_file()?;
    let tmp_path = path.with_file_name("bundle.zstd.tmp");

    // Verify the downloaded bundle is valid
    let bundle = crate::core::bundle::decompress_and_deserialize(&bytes)
        .context("다운로드된 번들이 유효하지 않습니다")?;

    // Schema version 검증
    let remote_version = bundle.metadata.schema_version;
    if remote_version != CURRENT_SCHEMA_VERSION {
        // 임시 파일 정리 불필요 (아직 쓰지 않음)
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
```

**Step 3: Verify compilation**

Run: `cargo check 2>&1 | tail -10`
Expected: OK

**Step 4: Commit**

```bash
git add src/cli/update.rs
git commit -m "feat: update 명령 — schema_version 검증 + atomic 파일 교체

다운로드 후 decompress_and_deserialize + schema_version == CURRENT 검증.
version mismatch 시 기존 번들 보존. tmp 파일 → rename으로 atomic 교체."
```

---

## Task 8: GitHub Actions 워크플로우

**Files:**
- Create: `.github/workflows/bundle-ci.yml`

**Step 1: 워크플로우 파일 생성**

```yaml
name: Bundle CI

on:
  schedule:
    - cron: '0 17 * * 6'  # UTC 토요일 17:00 = KST 일요일 02:00
  workflow_dispatch:

permissions:
  contents: write

# 동시 실행 방지 — 수집 중복 및 태그 충돌 방지
concurrency:
  group: bundle-ci
  cancel-in-progress: false

env:
  BUNDLE_PATH: data/bundle.zstd

jobs:
  build-bundle:
    runs-on: ubuntu-latest
    timeout-minutes: 90

    steps:
      - uses: actions/checkout@v4

      - name: Rust 캐시 설정
        uses: Swatinem/rust-cache@v2
        with:
          cache-on-failure: true

      - name: Rust 빌드
        run: cargo build --release --bin build-bundle --bin gen-catalog-docs

      - name: 번들 수집
        id: collect
        run: ./target/release/build-bundle --output $BUNDLE_PATH
        env:
          DATA_GO_KR_API_KEY: ${{ secrets.DATA_GO_KR_API_KEY }}

      - name: 실패분 재시도
        if: steps.collect.outcome == 'success'
        run: |
          if [ -s data/failed_ops.json ]; then
            ./target/release/build-bundle \
              --output $BUNDLE_PATH \
              --retry-stubs data/failed_ops.json \
              --max-retry-time 600
          else
            echo "failed_ops.json 없음 또는 비어있음 — retry skip"
          fi
        env:
          DATA_GO_KR_API_KEY: ${{ secrets.DATA_GO_KR_API_KEY }}

      - name: 변경 감지
        id: check
        run: |
          NEW_HASH=$(sha256sum $BUNDLE_PATH | awk '{print $1}')
          echo "new_hash=$NEW_HASH" >> $GITHUB_OUTPUT

          # 이전 bundle- 태그 릴리즈에서 번들 다운로드 시도
          PREV_TAG=$(gh release list --limit 20 --json tagName --jq '[.[].tagName | select(startswith("bundle-"))][0]' 2>/dev/null || true)
          if [ -n "$PREV_TAG" ] && gh release download "$PREV_TAG" --pattern bundle.zstd --dir /tmp/prev --repo "${{ github.repository }}" 2>/dev/null; then
            OLD_HASH=$(sha256sum /tmp/prev/bundle.zstd | awk '{print $1}')
            echo "old_hash=$OLD_HASH" >> $GITHUB_OUTPUT
            if [ "$NEW_HASH" = "$OLD_HASH" ]; then
              echo "changed=false" >> $GITHUB_OUTPUT
              echo "번들 변경 없음 — skip"
            else
              echo "changed=true" >> $GITHUB_OUTPUT
              echo "번들 변경 감지!"
            fi
          else
            echo "changed=true" >> $GITHUB_OUTPUT
            echo "이전 릴리즈 없음 — 최초 배포"
          fi
        env:
          GH_TOKEN: ${{ github.token }}

      - name: 카탈로그 문서 재생성
        if: steps.check.outputs.changed == 'true'
        run: ./target/release/gen-catalog-docs --bundle $BUNDLE_PATH

      - name: 문서 커밋 + 푸시
        if: steps.check.outputs.changed == 'true'
        continue-on-error: true  # 문서 커밋 실패가 Release를 막지 않도록
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          if git diff --quiet docs/api-catalog/; then
            echo "문서 변경 없음"
          else
            git add docs/api-catalog/
            DATE=$(date +%Y-%m-%d)
            git pull --rebase origin main
            git commit -m "docs: 카탈로그 문서 자동 업데이트 ($DATE)"
            git push
          fi

      - name: GitHub Release 생성
        if: steps.check.outputs.changed == 'true'
        run: |
          DATE=$(date +%Y-%m-%d)
          TAG="bundle-${DATE}-${GITHUB_RUN_NUMBER}"
          gh release create "$TAG" $BUNDLE_PATH \
            --title "Bundle ${DATE}" \
            --latest \
            --notes "자동 수집 ${DATE}"
        env:
          GH_TOKEN: ${{ github.token }}
```

**Step 2: .github/workflows 디렉토리 확인**

Run: `ls -la .github/workflows/ 2>/dev/null || echo "디렉토리 없음"`

디렉토리가 없으면 생성됨 (Write 도구가 자동 처리).

**Step 3: Verify YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/bundle-ci.yml'))" 2>&1`
Expected: No errors

**Step 4: Commit**

```bash
git add .github/workflows/bundle-ci.yml
git commit -m "ci: 주 1회 번들 수집 + retry + Release 배포 파이프라인

토요일 17:00 UTC 크론. 수집 → retry → 변경 감지 → 문서 업데이트 → Release.
DATA_GO_KR_API_KEY secret 필요."
```

---

## Task 9: 전체 빌드 + 테스트 + 린트 검증

**Files:** (변경 없음 — 검증만)

**Step 1: 전체 테스트**

Run: `cargo test 2>&1 | tail -30`
Expected: ALL tests PASS

**Step 2: Clippy 린트**

Run: `cargo clippy --all-targets 2>&1 | tail -20`
Expected: No errors (warnings 허용)

**Step 3: Format 확인**

Run: `cargo fmt -- --check 2>&1`
Expected: No formatting issues

**Step 4: build-bundle 바이너리 빌드 확인**

Run: `cargo build --bin build-bundle 2>&1 | tail -5`
Expected: Compiling/Finished

**Step 5: --help 플래그 확인**

Run: `cargo run -- --help 2>&1`
Expected: update 서브커맨드 포함

**Step 6: 최종 커밋 (필요시 수정 후)**

린트/테스트 실패 시 수정하고 커밋. 통과하면 이 단계는 skip.

---

## 구현 순서 요약

| Task | 내용 | 예상 범위 |
|------|------|----------|
| 1 | SpecStatus::PartialStub + classify | ~30줄 변경 + ~50줄 테스트 |
| 2 | FailedOp/ErrorType 타입 + SpecResult 확장 + timeout 전파 | ~50줄 추가 + ~30줄 변경 |
| 3 | fetch_gateway_spec 에러 분류 | ~40줄 변경 |
| 4 | main() partial_ids + failed_ops.json | ~35줄 변경 |
| 5 | --retry-stubs 구현 | ~100줄 추가 |
| 6 | caller/MCP PartialStub 안내 | ~30줄 변경 |
| 7 | update.rs schema_version 검증 + atomic 교체 | ~20줄 변경 |
| 8 | GitHub Actions 워크플로우 | ~100줄 신규 |
| 9 | 전체 검증 | 변경 없음 |

**총 예상: ~350줄 변경/추가, 커밋 8-9개**
