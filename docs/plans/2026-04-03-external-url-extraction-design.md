# External API endpoint_url 채우기

## 배경

External API 4,942건의 `endpoint_url`이 전부 빈 값(`" "` 또는 `""`)이다. 메타 API가 LINK API(PRDE04)의 실제 외부 포탈 URL을 제공하지 않기 때문.

그러나 openapi.do 페이지 HTML에는 `a.link-api-btn[href]`로 외부 URL이 존재한다 (4,871개 페이지, ~93% 커버리지).

`build_bundle.rs`가 이미 openapi.do 페이지를 fetch하고 `parse_openapi_page`로 파싱하는 시점에서 PRDE04를 감지하고 bail하므로, 같은 시점에서 외부 URL을 추출하면 추가 네트워크 요청 없이 해결 가능.

## 변경 사항

### 1. `src/core/html_parser.rs` — PageInfo 확장

- `PageInfo`에 `external_url: Option<String>` 필드 추가
- `parse_openapi_page`에서 `a.link-api-btn[href]` 셀렉터로 추출
- **URL 정규화**: `href.trim()` 적용 후, `http`로 시작하는 유효 URL만 `Some`
  - scraper(html5ever)가 `&amp;` → `&` 자동 decode하므로 별도 처리 불필요

### 2. `src/bin/build_bundle.rs` — ExternalLink variant 추가

기존 `SpecResult::Bail`에 필드를 추가하면 12곳+ bail 사이트에 보일러플레이트가 필요하므로, 새 variant를 추가한다:

```rust
enum SpecResult {
    Spec { spec: Box<ApiSpec>, is_gateway: bool },
    Bail { is_link_api: bool, reason: String },       // 기존 그대로
    ExternalLink { url: Option<String> },              // 신규
}
```

- PRDE04 bail 시점에서 `SpecResult::ExternalLink { url: page_info.external_url }` 반환
- **`collect_specs`의 match arm 추가** (카운터/로그 블록):
  - `ExternalLink`를 `Bail`과 같은 `fc` (fail_count) 경로로 처리
  - 로그 출력도 기존 Bail과 동일 패턴
- `main`에서 `ExternalLink` 매칭:
  - `link_api_ids`에 id 삽입 (기존 is_link_api=true 동작 유지)
  - `url`이 `Some`이면 `external_urls: HashMap<String, String>`에 삽입
- 기존 `Bail` 코드 변경 없음

### 3. endpoint_url 오버라이드 순서

Step 3 카탈로그 구성에서 **classify 호출 전에** effective_url을 결정한다:

```rust
let effective_url = external_urls
    .get(&svc.list_id)
    .cloned()
    .unwrap_or_else(|| svc.endpoint_url.clone());

let spec_status = SpecStatus::classify(&ClassificationHints {
    endpoint_url: &effective_url,
    is_link_api: link_api_ids.contains(&svc.list_id),
    ..
});

CatalogEntry {
    endpoint_url: effective_url,
    ..
}
```

현재 `is_link_api=true`이면 classify는 endpoint_url을 참조하지 않고 즉시 `External`을 반환하므로, effective_url의 실질 효과는 `CatalogEntry.endpoint_url`을 채우는 것이다. classify에도 전달하는 이유는 향후 classify 로직 변경에 대한 방어적 설계.

## 테스트 계획

### 단위 테스트 (`html_parser.rs`)

1. `link-api-btn` + 유효 href → `external_url == Some("https://...")`
2. `link-api-btn` 없음 → `external_url == None`
3. href가 `javascript:void(0)` → `external_url == None`
4. href에 `&amp;` 포함 → scraper가 decode한 `&` URL 확인

### 통합 검증 (`build_bundle`)

- 빌드 로그에 `external_url: N/M LINK API (X%)` 출력하여 커버리지 확인

## 영향 범위

- **번들 데이터**: External API의 endpoint_url이 실제 외부 포탈 URL로 채워짐
- **CLI/MCP**: 변경 없이 자동 혜택 (이미 endpoint_url을 표시하는 로직 있음)
- **카탈로그 문서**: 재생성 시 링크 컬럼에 실제 URL 표시
- **번들 크기**: URL 문자열 ~5K개 추가, 미미

## 커버리지

- 크롤링 데이터 기준: link-api-btn 있는 4,871개 전수 확인, href 추출 100%
- LINK API 전체 대비 ~93% — 나머지 ~7%는 크롤링 미수집 또는 실제 URL 미존재
