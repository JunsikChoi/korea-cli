# Gateway API 스펙 추출 설계

## 목표

번들 빌드 시 Swagger가 없는 Gateway API(클러스터 3+7, 3,187개)에서 `selectApiDetailFunction.do` AJAX 호출을 통해
완전한 `ApiSpec`을 추출하고, 분류 우선 파이프라인으로 SpecStatus 정확도를 개선한다.

**기대 효과**: Available API 수 ~4,042 → ~7,229 (+79%)

## 배경

### signal_summary.json 클러스터 분석 결과

12,108개 HTML 페이지가 8개 클러스터로 분류됨:

| 클러스터 | 수량 | 핵심 신호 | 실체 | 현재 처리 | 개선 후 |
|----------|------|-----------|------|-----------|---------|
| 1 | 4,010 | `swaggerJson:object` + slides | 정상 Swagger | ✅ Available/Skeleton | 변경 없음 |
| 2 | 3,399 | `swaggerJson:empty` + `html:other` | LINK API (`PRDE04`) | ❌ HtmlOnly로 오분류 | **External** |
| 3 | 3,165 | `apiObj` + `paramList` + `paramObj:object` | Gateway API (HTML 스펙) | ❌ HtmlOnly (bail) | **✅ Available** |
| 4 | 1,397 | `swaggerJson:object` + `html:other` | Skeleton (paths: {}) | ✅ Skeleton | 변경 없음 |
| 5 | 75 | `swaggerJson:undefined` | LINK API 변형 | ❌ HtmlOnly | **External** |
| 6 | 32 | `swaggerUrl:string` | 외부 Swagger URL | ✅ Available | 변경 없음 |
| 7 | 22 | `apiObj` + `paramList` (paramObj 없음) | 불완전 Gateway | ❌ HtmlOnly | **✅ Available** |
| 8 | 8 | `(none)` | 404 에러 페이지 | ✅ CatalogOnly | 변경 없음 |

### 검증된 사실

1. **AJAX 응답 구조 = 프리렌더 HTML 구조**: `selectApiDetailFunction.do` 응답은 `<div id="open-api-detail-result">` 형태의 HTML 프래그먼트. `parse_operation_detail()` 파서 하나로 양쪽 모두 처리 가능.
2. **세션 쿠키 필요**: AJAX 호출 시 `JSESSIONID` 필요. 페이지 GET 시 받은 쿠키 재사용.
3. **`publicDataDetailPk`는 UUID**: `list_id`가 아닌 `uddi:xxxx-xxxx` 형식. HTML hidden input에서 추출.
4. **오퍼레이션별 서비스 URL이 다름**: 같은 API 내에서도 오퍼레이션마다 endpoint가 다름 (예: `getTourismTotqyList` vs `getTourismExprncrtList`).

## 아키텍처

### 분류 우선 파이프라인

현재의 "추출 시도 → 실패하면 분류" 방식에서 "분류 먼저 → 해당 전략으로 추출"으로 변경.

```
fetch_single_spec(client, list_id)
│
├── GET /data/{list_id}/openapi.do → html + session cookie
│
│ ① 페이지 타입 판별 (저비용, 즉시)
├── tyDetailCode 추출 (regex)
├── swaggerJson 존재 여부 확인
├── <select id="open_api_detail_select"> 존재 여부
│
│ ② 타입별 라우팅
├── 404 / 파싱 불가              → bail("404")        → CatalogOnly
├── PRDE04 (LINK API)            → bail("LINK API")   → External
├── WMS/WFS/WCS (endpoint URL)   → bail("WMS/WFS")    → Unsupported
├── swaggerJson:object            → parse_swagger       → Available/Skeleton
├── swaggerUrl:string             → fetch + parse_swagger → Available
├── select 있음 (Gateway API)    → extract_gateway_spec → Available
└── 그 외                         → bail("no pattern")  → HtmlOnly
```

### Gateway 스펙 추출 상세 (Pattern 3)

```
extract_gateway_spec(html, client, list_id) → Result<ApiSpec>
│
│ ① 정적 HTML 파싱 (parse_openapi_page)
├── publicDataDetailPk: UUID (hidden input)
├── publicDataPk: list_id (hidden input)
├── operations: Vec<(oprtinSeqNo, name)> (select options)
│
│ ② 오퍼레이션별 AJAX 호출
├── for each (seqNo, name) in operations:
│   POST /tcs/dss/selectApiDetailFunction.do
│   body: { oprtinSeqNo, publicDataDetailPk(UUID), publicDataPk }
│   cookie: 동일 세션
│   delay: operation_delay_ms (rate limiting)
│   → HTML 프래그먼트
│   → parse_operation_detail()
│       ├── 서비스URL → operation URL
│       ├── 요청변수 <tr class="paramtrCls"> → request params
│       ├── 출력결과 테이블 → response fields
│       └── <h4 class="tit"> → operation summary
│   → 실패 시: 해당 오퍼레이션 skip, 나머지 계속 (부분 성공)
│
│ ③ ApiSpec 조합 (build_api_spec)
└── ApiSpec {
      list_id,
      base_url: 첫 오퍼레이션의 service_url,
      protocol: DataGoKrRest,
      auth: ServiceKey (QueryParam),
      operations: [Operation { path, method: GET, summary, params, response_fields }],
      ...
    }
```

## 변경 범위

### 1. `src/core/html_parser.rs` — 파서 버그 수정 + 기능 추가

#### 1.1 CRITICAL: 응답 필드 파싱 수정

`extract_response_fields()`가 "출력결과" 헤더를 `<tr>` 행에서 찾지만,
실제 HTML에서는 `<h4>출력결과(Response Element)</h4>` 다음에 테이블이 옴.

**수정**: HTML 구조를 이해하는 방식으로 변경. 테이블이 2개(요청변수 + 출력결과)임을 활용:
- 요청변수 테이블: `<h4>요청변수</h4>` 다음 `<table>`
- 출력결과 테이블: `<h4>출력결과</h4>` 다음 `<table>`

#### 1.2 CRITICAL: 빈 요청주소 처리

`build_api_spec()`에서 `request_url`이 비어있으면 Operation을 버림.
실제로 "요청주소"가 비어있고 "서비스URL"만 있는 케이스가 있음.

**수정**: `request_url`이 비어있으면 `service_url`을 사용하고 path를 `"/"`로 설정.

#### 1.3 요청 파라미터 셀렉터 수정

`tr[data-paramtr-nm]` → 실제는 `<td data-paramtr-nm>`. fallback이 커버하지만 정확한 셀렉터로 변경:
`tr.paramtrCls` 행을 찾고, 내부 `<td>`의 data 속성에서 파라미터 추출.

#### 1.4 Operation summary 추출

AJAX 응답의 `<h4 class="tit">`에 오퍼레이션 설명이 있음.
`parse_operation_detail()`에서 summary 필드로 추출.

#### 1.5 tyDetailCode 추출 추가

`PageInfo` 구조체에 `ty_detail_code: Option<String>` 필드 추가.
`parse_openapi_page()`에서 `var tyDetailCode = 'PRDE04';` 패턴으로 추출.

#### 1.6 ResponseFormat 개선

`ResponseFormat::Xml` 하드코딩 대신, HTML에서 데이터포맷 정보를 추출 가능한 경우 활용.
기본값은 `Xml` 유지 (data.go.kr Gateway API의 기본 응답 형식).

### 2. `src/bin/build_bundle.rs` — Pattern 3 통합

#### 2.1 reqwest Client — 기존 공유 Client 유지 + API별 AJAX Client

기존 `Arc<Client>`는 Swagger 패턴(Pattern 1/2)에서 그대로 사용.
Gateway API(Pattern 3) 진입 시 **API별 독립 Client** 생성하여 쿠키 격리:
```rust
// Pattern 3 진입 시 (fetch_single_spec 내부)
let ajax_client = reqwest::Client::builder()
    .user_agent("korea-cli-builder/0.1.0")
    .timeout(Duration::from_secs(15))
    .cookie_store(true)
    .build()?;
// 이 client로 openapi.do 재요청 (쿠키 획득) + selectApiDetailFunction.do 호출
```

#### 2.2 fetch_single_spec 확장

분류 우선 파이프라인 적용:
1. HTML에서 `tyDetailCode` 확인 → PRDE04면 즉시 bail
2. 기존 Pattern 1 (swaggerJson) 시도
3. 기존 Pattern 2 (swaggerUrl) 시도
4. **신규 Pattern 3**: `parse_openapi_page` → 오퍼레이션별 AJAX 호출 → `build_api_spec`
5. bail (진짜 파싱 불가)

#### 2.3 AJAX rate limiting — 글로벌 Semaphore + 오퍼레이션 delay

```rust
// 글로벌 AJAX 동시 요청 상한 (API 동시성과 별도)
let ajax_semaphore = Arc::new(tokio::sync::Semaphore::new(config.ajax_concurrency)); // 기본 10

// 오퍼레이션별 호출 시
let _permit = ajax_semaphore.acquire().await?;
let result = client.post(url).form(&data).send().await;
tokio::time::sleep(Duration::from_millis(config.ajax_delay_ms)).await; // 기본 50ms
drop(_permit);
```

CLI 파라미터 추가:
- `--ajax-concurrency` (기본 10): 글로벌 AJAX 동시 요청 수
- `--ajax-delay` (기본 50): 오퍼레이션 간 delay (ms)

#### 2.4 부분 실패 허용

```rust
let mut parsed_ops = Vec::new();
for op in page_info.operations {
    match fetch_operation_detail(&client, &pk, &op).await {
        Ok(detail) => parsed_ops.push(detail),
        Err(e) => eprintln!("  PARTIAL SKIP {}: {}", op.seq_no, e),
    }
}
// parsed_ops가 비어있지 않으면 ApiSpec 생성 (부분 성공 허용)
```

#### 2.5 분류 로직 개선

`SpecStatus::classify` 호출 시 `is_link_api` 정보 전달:
- fetch_single_spec의 bail 메시지 또는 반환 타입으로 분류 힌트 전달
- LINK API → External, Gateway 실패 → HtmlOnly, 404 → CatalogOnly

### 3. `src/core/types.rs` — classify 시그니처 변경

```rust
// 변경 전
pub fn classify(has_spec: bool, is_skeleton: bool, endpoint_url: &str) -> Self

// 변경 후
pub fn classify(has_spec: bool, is_skeleton: bool, endpoint_url: &str, is_link_api: bool) -> Self
```

`is_link_api`가 true면 스펙 유무와 무관하게 `External` 반환.
기존 로직은 그대로 유지하되, `is_link_api` 분기만 추가.

### 4. `src/bin/survey.rs` — classify 호출 수정

`survey.rs:266`의 `classify` 호출에 `is_link_api: false` 기본값 추가.
survey는 HTML 분석 없이 카탈로그 데이터만 사용하므로 false가 적합.

## Codex 기술 검증 결과

### 확인됨
- `Arc<Client>` + `cookie_store(true)` + `buffer_unordered` 조합은 **thread-safe**. `reqwest::cookie::Jar`가 `RwLock`으로 감싸져 있어 동시 읽기/쓰기 안전.

### 반영된 WARNING

**W1: 쿠키 오염 (Cookie Contamination)**
공유 Client의 cookie jar에 12K API의 세션 쿠키가 섞이면 AJAX 응답이 엉뚱한 데이터를 반환할 수 있음.
→ **대응**: Gateway API(Pattern 3) 처리 시 API별 독립 Client 생성. 빌드 타임 전용이므로 객체 생성 비용 감수 가능.
```rust
// Pattern 3 진입 시 API별 독립 client
let ajax_client = reqwest::Client::builder()
    .cookie_store(true)
    .timeout(Duration::from_secs(15))
    .build()?;
```

**W2: 빌드 시간 증가**
총 요청 수 추정: 3,187 API × 평균 N ops ≈ 수만~26만 POST. 각 200ms 기준 3~4시간 추가 소요 가능.
→ **대응**: `--ajax-concurrency` 파라미터를 API concurrency와 별도로 분리. 글로벌 동시 AJAX 요청 수를 제어.

**W3: Rate limiting 범위**
오퍼레이션 간 50ms delay는 단일 API 내에서만 적용. 5개 API가 동시에 AJAX를 쏘면 서버 관점에서 burst 발생.
→ **대응**: `tokio::sync::Semaphore`로 글로벌 AJAX 동시 요청 상한 설정 (예: 10).

### 반영된 SUGGESTION

**S1: classify 시그니처** — `is_link_api: bool` 채택 확정. `tyDetailCode` 같은 포털 내부 코드가 core 타입에 침투하는 것 방지. 향후 힌트 3개 이상이면 `ClassificationHints` 구조체로 리팩토링.

**S2: 부분 성공 로그** — 빌드 로그에 `"PARTIAL: N/M operations (list_id)"` 통계를 남겨 사후 검토 가능하게 함. `SpecStatus`에 새 variant 추가는 하지 않음 (Available로 처리하되 로그로 구분).

## 변경하지 않는 것

- `src/core/swagger.rs` — 기존 Pattern 1/2 로직 그대로
- `SpecStatus` enum variants — 새 variant 추가 없음 (Gateway 성공 → Available)
- `CURRENT_SCHEMA_VERSION` — 타입 구조 변경 없으므로 유지
- `src/core/caller.rs` — 호출 로직 변경 불필요
- `src/core/bundle.rs` — 직렬화 로직 변경 불필요

## 후속 작업 (이번 스코프 밖)

1. **WMS/WFS/WCS 실제 분포 확인**: signal_summary에서 별도 클러스터로 안 잡힘. endpoint URL 기반 분류가 정확한지 검증 필요.
2. **ResponseFormat 자동 감지**: 실제 API 호출로 JSON/XML 응답 형식 확인. 현재 Xml 기본값이 맞는지 샘플 테스트.
3. **data_path 자동 설정**: data.go.kr Gateway API의 응답 래핑 구조(`{ response: { body: { items: [...] } } }`) 패턴 분석 후 자동 설정.
4. **오퍼레이션 수 통계**: 3,187개 Gateway API의 평균/최대 오퍼레이션 수 파악 → AJAX 호출 총량 추정.

## 검증 계획

### 단위 테스트

1. `html_parser.rs` — 실제 AJAX 응답 HTML을 테스트 fixture로 사용:
   - 응답 필드 파싱 (수정된 `extract_response_fields`)
   - 빈 요청주소 케이스
   - `paramtrCls` 셀렉터
   - summary 추출
   - tyDetailCode 추출
2. `types.rs` — `classify` 시그니처 변경 후 기존 테스트 + `is_link_api` 테스트

### 통합 테스트

3. 실제 data.go.kr에 소수(3-5개) Gateway API로 Pattern 3 E2E 테스트:
   - 세션 쿠키 유지 확인
   - AJAX 호출 → 파싱 → ApiSpec 생성 → 호출 가능 여부
4. 전체 번들 빌드 드라이런 (dry-run 모드 또는 소규모 서브셋)

### 수동 검증

5. 생성된 번들의 SpecStatus 분포 비교 (이전 vs 이후)
6. Gateway API에서 추출된 ApiSpec으로 실제 API 호출 테스트
