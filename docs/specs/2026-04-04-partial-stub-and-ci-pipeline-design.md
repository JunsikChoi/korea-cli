# PartialStub + CI 수집 파이프라인 설계

## 배경

- Gateway API 662건이 부분 성공 (일부 operation만 AJAX 추출, 나머지 네트워크 에러)
- 현재는 실패한 operation을 버리고 성공한 것만 사용
- 번들 빌드가 수동 실행이라 재수집이 번거로움

## 목표

1. 부분 성공 API를 명시적으로 분류 (`PartialStub`) + 실패 operation 기록
2. CI 크론으로 주 1회 자동 수집 + 실패분 재시도
3. GitHub Releases로 번들 배포 + `korea-cli update`로 다운로드
4. 카탈로그 문서(`docs/api-catalog/`) 자동 업데이트

## 비목표 (YAGNI)

- 실시간/이벤트 기반 번들 업데이트
- 번들 diff/패치 (전체 교체로 충분)
- 사용자 인증 (public repo)

## 설계 결정

### 복구 전략 (전문가 토론 결과)

662건 부분 성공의 실패 원인은 대부분 네트워크 에러 (429, 타임아웃, body read 에러).
`parse_operation_detail`은 현재 항상 `Ok`를 반환하므로 파싱 실패는 거의 없음.

**기각된 옵션:**
- Sibling fallback (성공한 operation의 service_url 복사 → stub 생성) — service_url 일관성 미검증 + AI가 빈 params를 "파라미터 없음"으로 오해하는 위험
- CatalogEntry endpoint_url 폴백 — Sibling fallback의 열등 버전

**채택: 에러 분류 → PartialStub → 선택적 retry**
- stub operation을 만들지 않음
- 부분 성공 API를 `PartialStub`으로 분류만 함
- 이미 성공한 operation은 정상 사용 가능
- CI retry로 점진적 완성

---

## 1. PartialStub + 에러 분류

### 1.1 SpecStatus 확장

`SpecStatus` enum 끝에 `PartialStub` 추가 (postcard variant 순서 보존 필수):

```rust
pub enum SpecStatus {
    Available,
    Skeleton,
    HtmlOnly,
    External,
    CatalogOnly,
    Unsupported,
    PartialStub,  // 새로 추가 — 반드시 끝에
}
```

`CURRENT_SCHEMA_VERSION` = 2 → 3

`PartialStub` 정의: Gateway AJAX에서 1개+ operation 성공했지만 전체는 아닌 API.
기존 `Available`과의 차이: 일부 operation이 누락되어 있음을 명시적으로 표시.

`is_callable()` 처리: `PartialStub`은 `true` 반환 (존재하는 operation은 정상 호출 가능).

```rust
pub fn is_callable(&self) -> bool {
    matches!(self, Self::Available | Self::PartialStub)
}
```

`user_message()` 추가:
```rust
Self::PartialStub => "일부 operation만 수집됨 — 존재하는 operation은 호출 가능, 누락분은 다음 업데이트에서 복구 예정"
```

### 1.2 PartialStub 분류 경로

현재 `classify()`는 `has_spec: true`면 무조건 `Available`을 반환한다. `PartialStub`은 `classify()`를 우회하여 별도 경로에서 판정한다.

**구현 방식**: `ClassificationHints`에 `is_partial: bool` 필드 추가:

```rust
pub struct ClassificationHints<'a> {
    pub has_spec: bool,
    pub is_skeleton: bool,
    pub endpoint_url: &'a str,
    pub is_link_api: bool,
    pub is_partial: bool,  // 추가
}

impl SpecStatus {
    pub fn classify(hints: &ClassificationHints) -> Self {
        // 순서 중요: is_link_api를 먼저 체크.
        // LINK API는 fetch_single_spec에서 ExternalLink로 먼저 분기하므로
        // is_partial과 동시에 true가 되지 않지만, 방어적으로 선행 체크.
        if hints.is_link_api { return Self::External; }
        if hints.is_partial { return Self::PartialStub; }
        // 나머지 기존 로직 유지 (has_spec → Available, etc.)
    }
}
```

`build_bundle.rs` `main()`에서 부분 성공 API 처리 순서:

```rust
// is_partial 분기를 operations.is_empty() 보다 먼저 체크.
// 극단 케이스: partial이면서 build_api_spec에서 모든 op이 걸러져 operations가 빈 경우
// → PartialStub으로 분류 (Skeleton이 아님)
SpecResult::Spec { spec, is_partial, .. } => {
    if is_partial {
        partial_ids.insert(id.clone());
    }
    if spec.operations.is_empty() && !is_partial {
        skeleton_ids.insert(id);
    } else {
        specs.insert(id, *spec);
    }
}
```

부분 성공 API의 `list_id`를 `partial_ids: HashSet<String>`으로 추적:

```rust
// SpecResult::Spec에 partial 플래그 추가
SpecResult::Spec { spec, is_gateway, is_partial: bool }

// fetch_gateway_spec에서:
// parsed_ops.len() < total_ops → is_partial: true
```

`main()`에서 `is_partial`인 ID를 수집하고 `classify()` 호출 시 `is_partial` 힌트를 전달한다.

### 1.3 에러 분류 + failed_ops.json

`fetch_gateway_spec`에서 실패한 operation을 종류별로 기록.

**SpecResult 확장**: 실패 operation 정보를 반환값에 포함:

```rust
SpecResult::Spec {
    spec: Box<ApiSpec>,
    is_gateway: bool,
    is_partial: bool,
    failed_ops: Vec<FailedOp>,  // 추가
}

struct FailedOp {
    list_id: String,
    seq_no: String,
    op_name: String,       // OperationOption.name (디버깅용)
    error_type: ErrorType,
    error_message: String, // 원본 에러 메시지 (디버깅용)
}

enum ErrorType {
    NetworkTimeout,  // reqwest 타임아웃 (15초)
    RateLimited,     // HTTP 429
    BodyReadError,   // 응답 body 읽기 실패
    ParseError,      // HTML 파싱 실패 (현재 거의 없음)
}
```

에러 분류 로직 (`fetch_gateway_spec` 내부):
```rust
match ajax_result {
    Err(e) if e.is_timeout() => ErrorType::NetworkTimeout,
    Err(e) if e.is_status() && e.status() == Some(429) => ErrorType::RateLimited,
    Ok(resp) => match resp.text().await {
        Err(_) => ErrorType::BodyReadError,
        Ok(html) => match parse_operation_detail(&html) {
            Err(_) => ErrorType::ParseError,
            Ok(detail) => { /* 성공 */ }
        }
    }
}
```

빌드 완료 시 `--output` 경로의 동일 디렉토리에 `failed_ops.json` 출력 (기본: `data/failed_ops.json`).
`--retry-stubs`도 동일 규칙으로 입력 경로를 결정: `--output`의 디렉토리 + `failed_ops.json`.

출력 예시:

```json
[
  { "list_id": "15084084", "seq_no": "1", "op_name": "일별 박스오피스 조회",
    "error_type": "NetworkTimeout", "error_message": "connection timed out after 15s" },
  { "list_id": "15084084", "seq_no": "3", "op_name": "주간 박스오피스 조회",
    "error_type": "RateLimited", "error_message": "HTTP 429" }
]
```

### 1.4 caller.rs 안내 메시지

`PartialStub` spec 호출 시:
- 존재하는 operation은 정상 호출
- 누락된 operation 요청 시 "이 API는 일부 operation만 수집됨 — `korea-cli update`로 최신 번들을 받으면 추가 operation이 포함될 수 있습니다" 안내
- MCP `handle_get_spec`에서도 `spec_status=PartialStub` 표시

---

## 2. CI 수집 파이프라인

### 2.1 워크플로우 개요

- **트리거**: 주 1회 크론 + `workflow_dispatch` (수동)
- **크론**: `0 17 * * 6` (UTC 토요일 17:00 = KST 일요일 02:00)
- **러너**: `ubuntu-latest`
- **예상 소요**: 30-50분 (Rust 컴파일 캐시 히트 시 1-2분 / 미스 시 5-8분 + API 수집 15-20분 + retry 5-15분 + 문서 생성 1-2분)
- **권한**: `permissions: { contents: write }` 필수 (main push + Release 생성)

### 2.2 파이프라인 단계

```
Step 1: Rust 빌드 (cargo build --release)
  - actions/cache로 target/ 캐시
  - build-bundle + gen-catalog-docs 바이너리 빌드

Step 2: 번들 수집 (cargo run --release --bin build-bundle)
  - DATA_GO_KR_API_KEY는 GitHub Secrets
  - data/failed_ops.json 출력

Step 3: 실패분 재시도 (--retry-stubs data/failed_ops.json)
  - NetworkTimeout/RateLimited/BodyReadError만 대상 (ParseError 제외)
  - retry client timeout = 30s (main build = 15s)
  - retry params:
    - NetworkTimeout/BodyReadError: delays=[2s, 8s, 30s], max_retries=3
    - RateLimited: delays=[60s, 120s, 300s], max_retries=3 (rate limit reset 대기)
  - 최대 실행 시간 제한: `--max-retry-time 600` (10분, CLI 파라미터로 `parse_args()`에 추가)
    - 경과 시간 체크: 각 list_id 재수집 전에 `Instant::elapsed()` 확인, 초과 시 남은 대상 skip
  - retry 성공 시 기존 번들의 해당 list_id spec을 **merge** 교체:
    - 기존 spec의 operation + retry 결과의 operation을 합집합 (union)
    - 동일 path의 operation은 retry 결과로 덮어쓰기
    - 이전에 성공했던 operation이 retry에서 실패해도 기존 것을 보존 (퇴행 방지)
  - retry timeout: `BuildConfig`에 `retry_timeout_secs: u64` 필드 추가 (기본 30s).
    `fetch_gateway_spec` 내부에서 `ajax_client` 생성 시 이 값을 사용.
    main build는 15s, --retry-stubs는 30s.
  - 여전히 실패하면 PartialStub 유지

Step 4: 변경 감지
  - bundle.zstd의 SHA-256 해시 계산 (CI에서 sha256sum 명령 사용)
  - 이전 Release에서 번들 다운로드 → SHA-256 비교
  - 최초 실행(이전 Release 없음): gh release view 실패 → 항상 배포로 진행
  - 동일 → skip (변경 없음), 워크플로우 종료
  - 다름 → Step 5로 진행

Step 5: 카탈로그 문서 재생성 (cargo run --release --bin gen-catalog-docs)
  - docs/api-catalog/ 전체 재생성

Step 6: 배포
  a) docs/api-catalog/ 변경분 자동 커밋 + push (main)
  b) 커밋 성공 후 bundle.zstd를 GitHub Release로 업로드
  - Release 실패 시 워크플로우 전체 실패 (알림 발생)
```

### 2.3 --retry-stubs 구현 상세

retry는 단순 AJAX 재호출이 아니라 **해당 list_id의 전체 spec을 재수집**한다:

1. `data/failed_ops.json` 읽기
2. 기존 번들 로드: `bundle::decompress_and_deserialize(&std::fs::read(&config.output)?)` 직접 호출
   (`load_bundle()`은 ~/.config 또는 embedded만 읽으므로 사용 불가)
3. 고유 `list_id` 목록 추출
4. 각 `list_id`에 대해 `fetch_single_spec()` 재실행 (openapi.do 재방문 → AJAX 전체 재시도)
4. 성공 시 기존 번들의 해당 spec을 교체
5. 여전히 부분 성공이면 PartialStub 유지, 완전 성공이면 Available로 승격

retry 딜레이 루프는 `--retry-stubs` runner 레벨에서 구현:
- list_id 목록을 순회하며 `fetch_single_spec()` 호출
- 각 list_id 사이에 에러 타입별 딜레이 적용:
  - `NetworkTimeout`/`BodyReadError`: [2s, 8s, 30s] (3회)
  - `RateLimited`: [60s, 120s, 300s] (3회, rate limit reset 대기)
- `fetch_gateway_spec` 내부의 AJAX 루프는 기존과 동일 (retry 없이 1회)
- retry runner가 `fetch_single_spec()` 전체를 재호출하므로 세션 쿠키도 갱신됨
- 동일 list_id에 여러 에러 타입이 혼합된 경우: 가장 긴 딜레이(RateLimited 기준)를 적용

이 방식의 장점:
- `failed_ops.json`에 `total_ops`, `existing_seq_nos` 등 복잡한 병합 정보 불필요
- 기존 `fetch_single_spec` 코드를 그대로 재사용
- operation 순서/구성이 바뀌어도 안전

### 2.4 다음 크론 실행 시 PartialStub 처리

매주 크론은 Step 2에서 전체 재수집을 실행한다. 이전 주의 PartialStub API도 처음부터 다시 AJAX를 시도한다. 따라서:
- 일시적 네트워크 문제였다면 자연스럽게 Available로 승격
- 지속적 문제면 다시 PartialStub으로 분류
- `failed_ops.json`은 매주 새로 생성 (누적하지 않음)

### 2.5 Release 전략

- **태그**: `bundle-YYYY-MM-DD` (예: `bundle-2026-04-06`)
- **`--latest` 플래그 필수**: 날짜 태그는 semver가 아니므로 GitHub의 자동 latest 마킹에 의존할 수 없음
  ```bash
  gh release create "bundle-${DATE}" data/bundle.zstd \
    --title "Bundle ${DATE}" \
    --latest \
    --notes "자동 수집 ${DATE}"
  ```
- `korea-cli update`의 다운로드 URL은 항상 고정:
  `https://github.com/{owner}/{repo}/releases/latest/download/bundle.zstd`
- 이전 릴리즈는 자연스럽게 쌓임 (4MB 수준이라 용량 문제 없음)

### 2.6 카탈로그 문서 자동 업데이트

Step 5 후:

1. `git diff --stat docs/api-catalog/`로 변경 확인
2. 변경 있으면:
   ```bash
   git add docs/api-catalog/
   git commit -m "docs: 카탈로그 문서 자동 업데이트 (YYYY-MM-DD)"
   git push
   ```
3. 변경 없으면: skip

- 커밋 저자: `github-actions[bot]`
- 브랜치: main 직접 커밋 (기계 생성 파일)

### 2.7 Secrets 및 권한

| Secret | 용도 |
|--------|------|
| `DATA_GO_KR_API_KEY` | 공공데이터포털 API 키 |
| `GITHUB_TOKEN` | 자동 제공 (Release 생성 + 커밋 push) |

**필수 설정:**
- 워크플로우: `permissions: { contents: write }`
- Repository Settings > Actions > General > Workflow permissions: "Read and write permissions" 활성화

---

## 3. korea-cli update 번들 다운로드

### 3.1 다운로드 로직

- URL: `https://github.com/{owner}/{repo}/releases/latest/download/bundle.zstd`
- 저장: `~/.config/korea-cli/bundle.zstd`
- 기존 번들이 있으면 덮어쓰기

### 3.2 흐름

```
1. latest release의 bundle.zstd 다운로드 (reqwest GET, redirect follow)
2. 임시 파일에 저장 (~/.config/korea-cli/bundle.zstd.tmp)
   — 반드시 최종 경로와 동일 디렉토리 (크로스 파티션 rename 방지)
3. postcard 역직렬화 + schema_version 검증
   — bundle.rs의 decompress_and_deserialize()로 역직렬화
   — 주의: decompress_and_deserialize()는 schema_version을 확인하지 않음.
     update 명령에서 직접 bundle.metadata.schema_version == CURRENT_SCHEMA_VERSION 비교 추가 필수.
   — 이것은 "재사용"이 아니라 update.rs에 schema_version 체크를 새로 구현하는 것
4. 검증 성공 → .tmp를 bundle.zstd로 rename (atomic)
5. 검증 실패 → .tmp 삭제 + 에러 메시지
```

### 3.3 버전 불일치 처리

번들 `schema_version` != CLI의 `CURRENT_SCHEMA_VERSION`인 경우:
- `schema_version > CURRENT`: "새 번들은 최신 CLI가 필요합니다. `cargo install korea-cli`로 업데이트하세요"
- `schema_version < CURRENT`: "구버전 번들입니다. 최신 Release가 아직 생성되지 않았습니다"
- 두 경우 모두 기존 번들 유지 (덮어쓰지 않음)

**배포 순서 원칙**: schema_version bump 시 CLI 릴리즈(cargo publish)를 먼저 하고, 그 후 CI 번들 배포를 실행한다. 역순이면 사용자가 update해도 번들이 거부된다.

### 3.4 기존 코드와의 관계

현재 `bundle.rs`의 로드 체인:
1. `~/.config/korea-cli/bundle.zstd` 확인
2. 스키마 버전 검증 (`== CURRENT_SCHEMA_VERSION`)
3. 실패 시 내장 번들 fallback

`update` 명령은 1번의 파일을 교체하는 것. 로드 체인 자체는 변경 불필요.
역직렬화는 `bundle.rs`의 `decompress_and_deserialize()` 재사용. schema_version 체크는 `update.rs`에 별도 구현 (`decompress_and_deserialize`는 버전을 확인하지 않으므로).

---

## 4. 구현 범위 요약

### 코드 변경

| 파일 | 변경 | 범위 |
|------|------|------|
| `types.rs` | `SpecStatus::PartialStub` 추가, `is_callable()` 확장, `user_message()` 추가, `ClassificationHints.is_partial`, `CURRENT_SCHEMA_VERSION=3` | ~15줄 |
| `build_bundle.rs` | `SpecResult::Spec`에 `is_partial`+`failed_ops` 추가, 에러 분류, `partial_ids` 추적, `data/failed_ops.json` 출력 | ~60줄 |
| `build_bundle.rs` | `--retry-stubs` 플래그 (`failed_ops.json` 읽어 list_id 단위 재수집) | ~80줄 |
| `caller.rs` | `PartialStub` 안내 메시지 | ~10줄 |
| `cli/update.rs` | Releases에서 번들 다운로드 + `decompress_and_deserialize()` 재사용 검증 + atomic 교체 | ~50줄 |
| `mcp/tools.rs` | `spec_status=PartialStub` 표시 | ~5줄 |

### 신규 파일

| 파일 | 용도 |
|------|------|
| `.github/workflows/bundle-ci.yml` | 주 1회 번들 수집 + retry + 문서 업데이트 + Release |

### 스키마 변경

- `schema_version` 2 → 3 (`PartialStub` 추가 + `ClassificationHints.is_partial`)
- 기존 번들 호환: `load_bundle()`의 버전 체크 + 내장 번들 fallback으로 처리

### 의존성 변경

없음 (reqwest, tokio 이미 사용 중. SHA-256은 `sha2` 크레이트 또는 CI에서 `sha256sum` 명령 사용)
