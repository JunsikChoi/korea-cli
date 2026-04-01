# Spec 품질 개선 설계

## 목표

번들 내 12,080 API의 spec 품질을 개선하여, AI 에이전트가 각 API의 사용 가능 여부를 즉시 판단하고 적절한 안내를 받을 수 있도록 한다.

## 현재 문제

1. CatalogEntry에 spec 유무/상태 정보가 없어 에이전트가 불필요한 spec 조회 시도
2. ~1,400개 skeleton spec(빈 operations)이 번들에 포함되어 사용자 혼동
3. spec 없는 API 조회 시 "찾을 수 없습니다" 에러만 — 대안 안내 없음
4. ~1,200개 Gateway API의 HTML 테이블에서 파싱 가능한 spec을 미수집

## 설계

### 0. 번들 스키마 버전 관리

postcard는 필드 순서 기반 직렬화로, CatalogEntry에 필드를 추가하면 기존 번들과 역직렬화가 호환되지 않는다.

#### 문제 시나리오

- 사용자가 구 버전 CLI로 `korea-cli update` 실행 → 구 스키마 번들 다운로드
- 이후 신 버전 CLI로 업데이트 → 신 스키마로 역직렬화 시도 → 크래시

#### 해결: schema_version 필드

```rust
pub struct BundleMetadata {
    pub version: String,
    pub schema_version: u32,   // 신규: 1 → 2
    pub api_count: usize,
    pub spec_count: usize,
    pub checksum: String,
}
```

- `schema_version`은 BundleMetadata의 **두 번째 필드**로 추가 (postcard 순서 의존)
- 번들 로드 시 먼저 메타데이터만 부분 역직렬화하여 schema_version 체크
- 버전 불일치 시: 내장 번들로 폴백 + "korea-cli update를 실행하세요" 안내
- 현재 번들(schema_version 없음) = v1, 이번 변경 후 = v2

#### 마이그레이션 전략

BundleMetadata에도 필드가 추가되므로, 구 번들의 BundleMetadata 자체가 역직렬화 실패한다. 이를 이용하여:

```rust
fn load_bundle(bytes: &[u8]) -> Result<Bundle> {
    match postcard::from_bytes::<Bundle>(bytes) {
        Ok(bundle) => {
            // schema_version 체크 (향후 v2→v3 전환 대비)
            if bundle.metadata.schema_version != CURRENT_SCHEMA_VERSION {
                anyhow::bail!("번들 스키마 버전 불일치. korea-cli update를 실행하세요.");
            }
            Ok(bundle)
        }
        Err(_) => {
            // 구 스키마 번들 → 내장 번들로 폴백
            eprintln!("외부 번들이 현재 버전과 호환되지 않습니다. 내장 번들을 사용합니다.");
            eprintln!("최신 번들을 받으려면: korea-cli update");
            load_embedded_bundle()
        }
    }
}
```

**핵심**: 역직렬화 실패 = 구 스키마로 간주하고 내장 번들 폴백. 크래시 없이 graceful degradation.

#### postcard enum 직렬화 참고

- postcard는 `#[repr(u8)]`를 무시하고 자체 varint 인코딩 사용
- SpecStatus enum의 variant 순서를 변경하면 호환성이 깨지므로, **새 variant는 항상 끝에 추가**
- `#[repr(u8)]`는 메모리 레이아웃 힌트일 뿐, 직렬화 안정성을 보장하지 않음

### 1. 데이터 모델 변경

#### SpecStatus enum

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[repr(u8)]
pub enum SpecStatus {
    Available,    // 유효 Swagger spec — 호출 가능
    Skeleton,     // Swagger 있으나 operations 비어있음 — 외부 서비스
    HtmlOnly,     // apis.data.go.kr endpoint, HTML 파싱으로 spec 생성 예정
    External,     // 외부 포탈 링크 (서울열린데이터, vworld 등)
    CatalogOnly,  // endpoint URL 없음, 문서 링크만
    Unsupported,  // WMS/WFS 등 비REST 프로토콜
}

impl SpecStatus {
    pub fn is_callable(&self) -> bool {
        matches!(self, Self::Available)
    }
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Available => "API spec 사용 가능",
            Self::Skeleton => "Swagger spec이 비어있습니다. 외부 서비스 페이지에서 확인하세요.",
            Self::HtmlOnly => "spec 파싱 준비 중입니다. endpoint URL을 참고하세요.",
            Self::External => "외부 포탈에서 제공하는 API입니다.",
            Self::CatalogOnly => "카탈로그 정보만 있습니다.",
            Self::Unsupported => "REST가 아닌 프로토콜(WMS/WFS 등)입니다.",
        }
    }
}
```

선정 근거:
- 심층 분석 + Codex 교차검증 결과, AI 에이전트(주 소비자)에게 1필드로 즉시 판단 가능한 단순 enum이 최적
- endpoint_url 필드가 이미 추가되므로 EndpointType::External(String)의 정보가 중복 없이 대체됨
- postcard 직렬화 시 entry당 1바이트, 번들 크기 영향 ~12KB (무시 수준)

#### CatalogEntry 변경

```rust
pub struct CatalogEntry {
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub org_name: String,
    pub category: String,
    pub request_count: u32,
    pub endpoint_url: String,      // 새 필드
    pub spec_status: SpecStatus,   // 새 필드
}
```

#### SearchEntry 변경

```rust
pub struct SearchEntry {
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub org: String,
    pub category: String,
    pub popularity: u32,
    pub spec_status: SpecStatus,   // 새 필드
    pub endpoint_url: String,      // 새 필드
}
```

검색 정렬은 기존 유지 (매칭 단어 수 × 100 + request_count). spec_status 가중치 미부여 — 인기 API가 HtmlOnly여도 상위 노출 가치 있음.

### 2. 번들 빌더 변경

```
Step 1: 카탈로그 수집 (메타 API) — 기존과 동일

Step 2: Swagger 스펙 수집
  → 파싱 성공 + operations 있음 → specs에 저장 + Available
  → 파싱 성공 + operations 비어있음 → specs에 저장 안 함 + Skeleton

Step 3: HTML 테이블 파싱 (신규)
  → spec_status가 HtmlOnly 대상
  → GET openapi.do → publicDataDetailPk + 오퍼레이션 목록 추출
  → 각 오퍼레이션마다 POST selectApiDetailFunction.do → 상세 HTML
  → 파라미터/응답 테이블 파싱 → ApiSpec 생성
  → 성공 → specs에 저장 + Available로 승격

Step 4: CatalogEntry 생성 + spec_status 판정
  specs에 있음 → Available
  swagger 있었지만 skeleton → Skeleton
  endpoint_url에 apis.data.go.kr 포함 + spec 없음 → HtmlOnly
  endpoint_url 있음 + 외부 호스트 → External
  endpoint_url 없음 → CatalogOnly
  WMS/WFS 패턴 → Unsupported

Step 5: 직렬화 + 압축 — 기존과 동일
```

### 3. HTML 테이블 파서

새 모듈: `src/core/html_parser.rs`

#### AJAX 엔드포인트 (검증 완료)

세션 쿠키, CSRF 토큰, 특수 헤더 모두 불필요. reqwest 기본 설정만으로 동작 확인됨.

```
최소 요구:
- Method: POST
- URL: https://www.data.go.kr/tcs/dss/selectApiDetailFunction.do
- Content-Type: application/x-www-form-urlencoded
- Body: oprtinSeqNo={값}&publicDataDetailPk={URL-encoded 값}&publicDataPk={list_id}
```

#### 파싱 플로우

```
1. openapi.do HTML에서 추출:
   - publicDataDetailPk (hidden input, multiline regex)
   - 오퍼레이션 목록 (<select id="open_api_detail_select"> options)
   - 첫 번째 오퍼레이션의 상세 (인라인 HTML)

2. 각 오퍼레이션 상세 HTML에서 추출:
   - 요청주소 (<strong>요청주소</strong> 뒤의 URL)
   - 서비스URL (<strong>서비스URL</strong> 뒤의 URL)
   - 요청변수 테이블 → Vec<Parameter>
     - name: data-paramtr-nm 속성 또는 2번째 <td>
     - required: data-paramtr-division이 "필"로 시작하면 필수
     - description: data-paramtr-dc 속성 또는 6번째 <td>
     - param_type: "string" 고정 (타입 정보 없음)
   - 출력결과 테이블 → Vec<ResponseField>
     - name: 2번째 <td>
     - description: 6번째 <td>

3. ApiSpec 조립:
   - base_url: 서비스URL
   - path: 요청주소 - 서비스URL
   - method: GET 기본 가정
   - auth: ServiceKey/serviceKey 행 존재 → QueryParam
```

#### 라이브러리

`scraper` 크레이트 (CSS selector 기반). regex 대비 구조적 파싱에 안정적.

#### 에러 처리

파싱 실패한 오퍼레이션은 건너뛰고 로그만 남김. 하나라도 성공하면 ApiSpec 생성.

#### 비일관성 대응

- `항목구분` 값: "필수"/"옵션" 또는 "필"/"옵" 잘림 → prefix 매칭
- ServiceKey 대소문자: ServiceKey / serviceKey → 대소문자 무시 매칭
- PC/모바일 중복: 첫 번째 occurrence만 파싱

### 4. CLI/MCP 응답 변경

#### 검색 결과 (search_api)

```json
{
  "results": [
    {
      "list_id": "15084084",
      "title": "기상청_단기예보 조회서비스",
      "description": "...",
      "org": "기상청",
      "category": "과학기술",
      "popularity": 53803,
      "spec_status": "Available",
      "endpoint_url": "https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0"
    }
  ],
  "total": 42
}
```

#### spec 조회 (get_api_spec)

Available:
```json
{
  "success": true,
  "spec_status": "Available",
  "base_url": "https://apis.data.go.kr/...",
  "operations": [...]
}
```

그 외:
```json
{
  "success": false,
  "list_id": "15139452",
  "spec_status": "Skeleton",
  "endpoint_url": "https://apihub.kma.go.kr/...",
  "message": "Swagger spec이 비어있습니다. 외부 서비스 페이지에서 확인하세요.",
  "data_go_kr_url": "https://www.data.go.kr/data/15139452/openapi.do"
}
```

#### call_api

`spec_status != Available`이면 호출 시도 없이 동일한 안내 응답 반환.

#### MCP tool description

`search_api` 설명에 "spec_status가 Available인 API만 get_api_spec/call_api로 사용 가능" 문구 추가.

## 예상 효과

| 지표 | 현재 | 개선 후 |
|------|------|---------|
| Available API | ~3,960 (32.8%) | ~5,160 (42.7%) |
| skeleton spec 번들 포함 | ~1,400개 | 0개 |
| spec 없는 API 조회 UX | 에러만 | 안내 + endpoint_url |
| 검색 시 사용 가능 여부 | 알 수 없음 | spec_status로 즉시 판단 |
| 번들 크기 변화 | 2.77 MB | ~2.5 MB (skeleton 제거 + endpoint_url 추가 상쇄) |

## 리스크 및 대응

| 리스크 | 심각도 | 대응 |
|--------|--------|------|
| postcard 스키마 호환성 깨짐 | BLOCK → 해결됨 | schema_version + 내장 번들 폴백 (§0) |
| data.go.kr AJAX 엔드포인트 변경 | 중 | 빌드 타임에만 사용, CI 실패 시 알림으로 대응 |
| SpecStatus variant 순서 변경 시 호환성 깨짐 | 중 | 새 variant는 항상 끝에 추가하는 규칙 |
| HTML 테이블 구조 변경 | 낮 | 파싱 실패 시 해당 API만 skip, 전체 빌드 실패 아님 |

## 의존성

- `scraper` 크레이트 추가 (HTML 파싱, CSS selector 기반)
- 기존 `postcard`, `zstd`, `reqwest`, `serde` 유지
