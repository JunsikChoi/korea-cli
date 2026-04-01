# korea-cli 번들 전환 설계

## Context

korea-cli Phase 1 MVP는 카탈로그 검색은 잘 작동하지만, API spec 조회(search → spec → call)가 안정적으로 이어지지 않는다. Codex 실사용 테스트에서 다수 API의 spec 조회 실패가 확인되었다.

**근본 원인**: 사용자 측에서 data.go.kr을 실시간 스크래핑하여 Swagger spec을 가져오는 방식의 세 가지 문제.

1. **안정성**: 기존 코드가 `var swaggerUrl = '...'` 패턴만 탐지 → 대부분 실패
2. **속도**: 매 요청마다 2번의 HTTP 호출 (HTML 페이지 + Swagger JSON)
3. **차단**: rate limit, 연결 거부 발생

**핵심 발견**: 실제로 ~99% API가 `var swaggerJson = \`{...}\``로 인라인 Swagger JSON을 제공한다. 200개 랜덤 샘플에서 swaggerUrl 존재 = 0개, 50개 샘플에서 swaggerJson 인라인 존재 = 50개(100%). 두 패턴을 모두 처리하면 12,080개 API 거의 전체의 spec 수집이 가능하다.

**결정**: 사용자 측 실시간 스크래핑을 폐기하고, 사전 수집된 데이터를 번들로 배포한다.

## 아키텍처

### 접근법: Pre-bundled Specs

바이너리에 카탈로그 + 전체 API spec을 번들링하여 설치 즉시 검색·스펙 조회 가능. API 호출만 네트워크 사용.

```
┌─────────────────────────────────────────────────────┐
│                  korea-cli 바이너리                    │
│                                                       │
│  ┌─────┐  ┌──────┐  ┌──────┐  ┌──────────────────┐  │
│  │ CLI │  │ MCP  │  │ Core │  │ Bundle Loader    │  │
│  │     │──│Server│──│Engine│──│                  │  │
│  │     │  │      │  │      │  │ 1. 로컬 override │  │
│  └─────┘  └──────┘  └──┬───┘  │ 2. 내장 번들     │  │
│                         │      └──────────────────┘  │
└─────────────────────────┼────────────────────────────┘
                          │
              ┌───────────┼───────────┐
              ▼           │           ▼
     GitHub Releases      │     개별 API 서버
     (korea-cli update)   │     (call_api만)
                          ▼
              data/bundle.zstd
              (include_bytes!)
```

### 데이터 흐름

1. **검색 (search)** — 번들 카탈로그에서 텍스트 매칭 → 후보 API 목록 반환 (오프라인, <1ms)
2. **스펙 조회 (spec)** — 번들 specs HashMap에서 O(1) lookup → 정규화된 스펙 반환 (오프라인, <1ms)
3. **호출 (call)** — 번들 스펙에서 엔드포인트/인증 확인 → HTTP 호출 → 응답 정규화
4. **갱신 (update)** — GitHub Releases에서 최신 bundle.zstd 다운로드 → 로컬 저장

### 데이터 우선순위 (오버라이드 체인)

```
1순위: ~/.config/korea-cli/bundle.zstd  ← korea-cli update로 받은 것
2순위: 바이너리 내장 번들               ← include_bytes!로 빌드 시 포함
```

## 데이터 모델

### 번들 구조 (2-레이어 분리)

```rust
struct Bundle {
    metadata: BundleMetadata,
    catalog: Vec<CatalogEntry>,           // Layer 1: 검색용 (경량)
    specs: HashMap<String, ApiSpec>,      // Layer 2: API 사용법 (ID lookup)
}

struct BundleMetadata {
    version: String,        // "2026-03-31"
    api_count: usize,       // 12,080
    spec_count: usize,      // 수집 성공한 spec 수
    checksum: String,       // SHA256
}
```

### Layer 1: 카탈로그 (검색용 경량 데이터)

검색에 필요한 필드만 포함. 기존 ApiService에서 검색에 불필요한 필드 제거.

```rust
struct CatalogEntry {
    list_id: String,
    title: String,
    description: String,
    keywords: Vec<String>,
    org_name: String,
    category: String,
    request_count: u32,
}
```

**제거 필드** (검색에 미사용, 18MB → ~10.5MB, 42% 절감):
- `operations` (5.3MB, 32.6%) — 표시용
- `endpoint_url`, `data_format`, `auto_approve`, `is_free`, `updated_at`

### Layer 2: Specs (호출용 상세 데이터)

기존 `types.rs`의 ApiSpec 구조 그대로 사용. 12,080개 × ~640 bytes = ~7.7MB raw.

```rust
// 기존 ApiSpec 재사용
struct ApiSpec {
    list_id: String,
    base_url: String,
    protocol: ApiProtocol,
    auth: AuthMethod,
    extractor: ResponseExtractor,
    operations: Vec<Operation>,   // endpoint, method, params, response
    fetched_at: String,
}
```

### Swagger spec 수집 (두 가지 패턴)

data.go.kr 상세페이지에서 Swagger 데이터를 추출하는 두 가지 방식:

```
패턴 1 (대다수 ~99%): var swaggerJson = `{"swagger":"2.0",...}`  → 인라인 JSON 직접 파싱
패턴 2 (극소수  ~1%): var swaggerUrl = 'https://infuser...'     → 외부 URL fetch 후 파싱
```

### 크기 추정

| 단계 | 크기 |
|------|------|
| Raw (카탈로그 + specs) | ~18 MB |
| postcard 직렬화 | ~11 MB (JSON 대비 40% 절감) |
| zstd 압축 | **~2-3 MB** |

### 기술 선택

| 결정 | 선택 | 이유 |
|------|------|------|
| 압축 | zstd (level 3) | gzip 대비 10x 빠른 해제, 더 좋은 압축률 |
| 직렬화 | postcard (serde 호환) | JSON 대비 40-60% 작음, bincode보다 작음 |
| 임베딩 | `include_bytes!` | `include_str!`은 100MB+ 메타데이터 bloat 유발 |
| 로딩 | `once_cell::Lazy` | 첫 접근 시 1회 해제, 이후 O(1) |
| spec 조회 | `HashMap<String, ApiSpec>` | 12K 엔트리에서 O(1), FST/Trie는 과잉 |
| 텍스트 검색 | 기존 term matching 유지 | 12K 규모에서 <1ms, inverted index는 과잉 |

### 추가 의존성

```toml
[dependencies]
postcard = { version = "1", features = ["alloc"] }
zstd = "0.13"
once_cell = "1"

[build-dependencies]
postcard = { version = "1", features = ["alloc"] }
zstd = "0.13"
```

### 로컬 저장소 (변경 후)

```
~/.config/korea-cli/
├── config.toml            # api_key
└── bundle.zstd            # korea-cli update로 받은 최신 번들 (선택적)
```

기존 `catalog.json`, `cache/specs/` 는 더 이상 사용하지 않음.

## 모듈 구조

```
src/
├── main.rs              # CLI 엔트리포인트 (clap 서브커맨드)
├── core/
│   ├── mod.rs
│   ├── types.rs         # 기존 타입 + CatalogEntry, BundleMetadata 추가
│   ├── bundle.rs        # [신규] Bundle 로드/해제, 오버라이드 체인
│   ├── catalog.rs       # load_catalog() → 번들에서 로드로 변경
│   ├── swagger.rs       # extract_swagger_json() 추가, fetch_and_cache_spec() → 번들 lookup
│   └── caller.rs        # 변경 없음
├── mcp/
│   ├── mod.rs
│   ├── server.rs        # 변경 없음
│   └── tools.rs         # 번들 사용 (변경 최소)
├── config/
│   ├── mod.rs
│   └── paths.rs         # bundle 경로 추가
└── cli/
    ├── mod.rs
    ├── search.rs        # 변경 최소
    ├── spec.rs          # 스크래핑 호출 제거 → 번들 lookup
    ├── call.rs          # 변경 최소
    └── update.rs        # 메타 API → GitHub Releases 다운로드
scripts/
└── build_bundle.rs      # [신규] 수동 번들 생성 도구
data/
└── bundle.zstd          # [신규] 번들 파일 (git 커밋, ~2-3MB)
build.rs                 # [신규] include_bytes! 연결
```

## 구현 단계 (4 Step)

### Step 1: 번들 데이터 타입 + 로더

- `core/types.rs` — `CatalogEntry`, `BundleMetadata`, `Bundle` 추가. postcard `Serialize`/`Deserialize` derive
- `core/bundle.rs` — `load_bundle()`: 로컬 override 확인 → 없으면 내장 번들 → `once_cell::Lazy`로 1회 해제
- `build.rs` — `include_bytes!("data/bundle.zstd")` 연결
- `Cargo.toml` — postcard, zstd, once_cell 의존성 추가
- 검증: 테스트 번들(소량)로 로드/해제 + 카탈로그 검색 + spec lookup 동작 확인

### Step 2: 번들 수집 스크립트

- `scripts/build_bundle.rs` (또는 `src/bin/build_bundle.rs`) — 수집 바이너리
  1. 메타 API로 카탈로그 수집 (기존 `fetch_all_services` 재사용)
  2. 각 서비스 data.go.kr 페이지에서 Swagger spec 수집:
     - `var swaggerJson = \`{...}\`` 인라인 파싱 (대다수)
     - `var swaggerUrl = '...'` 외부 URL fetch (소수)
     - 둘 다 없으면 스킵 + 로그
  3. 카탈로그 경량화 (`ApiService` → `CatalogEntry`)
  4. postcard 직렬화 → zstd 압축 → `data/bundle.zstd` 출력
  5. 결과 리포트: "12,080개 중 N개 spec 수집 (X%)"
- `core/swagger.rs` — `extract_swagger_json()` 추가 (인라인 JSON 파싱)
- rate limit 대응: 요청 간 100-200ms 딜레이, 실패 시 3회 재시도 (exponential backoff)
- 검증: 스크립트 실행 → bundle.zstd 생성 + 크기 < 5MB 확인

### Step 3: CLI/MCP 통합

- `core/catalog.rs` — `load_catalog()` → 번들에서 로드. 기존 `Catalog` 타입 → `Bundle` 카탈로그 사용
- `cli/spec.rs` — 스크래핑 호출 제거 → `bundle.specs.get(list_id)` lookup
- `cli/update.rs` — 메타 API 페이지네이션 → GitHub Releases에서 bundle.zstd 다운로드
- `mcp/tools.rs` — `get_api_spec` 도구 → 번들 lookup으로 변경
- `config/paths.rs` — `bundle_override_file()` 경로 추가
- 검증: CLI `search` → `spec` → `call` E2E 흐름 + MCP 서버 동일 흐름

### Step 4: 정리 + 마무리

- 기존 실시간 스크래핑 코드 정리 (fetch_and_cache_spec, load_cached_spec 제거 또는 수집 스크립트 전용으로 이동)
- 기존 `catalog.json`, `cache/specs/` 참조 제거
- README 업데이트 (설치 후 바로 사용 가능, update 명령 설명)
- clippy + fmt + 테스트 통과
- 검증: 클린 환경에서 `cargo install` → 즉시 `search` + `spec` 동작 확인

## 검증 시나리오 (E2E)

```bash
# 1. 설치 직후 (네트워크 없이)
$ korea-cli search "기상청"
# [1] 기상청_단기예보 조회서비스 (list_id: 15084084)

# 2. spec 조회 (네트워크 없이)
$ korea-cli spec 15084084
# GET /getVilageFcst — 파라미터: base_date, base_time, nx, ny ...

# 3. API 호출 (네트워크 필요)
$ korea-cli call 15081808 /status --param b_no='["1234567890"]'
# { "success": true, "data": [...] }

# 4. 번들 업데이트
$ korea-cli update
# "번들 업데이트: v2026-03-31 → v2026-04-07 (12,135 APIs)"

# 5. MCP 서버 (Claude Desktop)
# 사용자: "1234567890 사업자번호가 유효한지 확인해줘"
# AI → search_api → get_api_spec → call_api → 자연어 응답
```

## 이번 스코프 외 (Phase 1.1)

- CI 수집 파이프라인 (GitHub Actions 크론 + 변경 감지 + 자동 릴리스)
- XML 응답 파싱 지원
- 인증 처리 일반화 (`Infuser ` 접두사 하드코딩 제거)
- 사용자 입력 정규화 (사업자번호 하이픈 등)

## 공공데이터포털 Swagger 가용성 (참고)

| 유형 | 수 | 비율 | Swagger 수집 방식 |
|------|------|------|------|
| swaggerJson 인라인 | ~11,950 | ~99% | 페이지 HTML에서 직접 파싱 |
| swaggerUrl 외부 참조 | ~130 | ~1% | URL fetch 후 파싱 |
| 둘 다 없음 | 소수 | <1% | 카탈로그만 포함, spec 미제공 안내 |
| **합계** | **12,080** | | |
