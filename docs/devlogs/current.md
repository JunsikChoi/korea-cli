# korea-cli devlog

## 2026-03-31: 프로젝트 킥오프

### 완료
- 아이디어 구체화 및 경쟁 분석
- 기술 스택 결정: Rust (CLI + MCP 단일 바이너리)
- GitHub 리포 생성 (public): JunsikChoi/korea-cli
- 프로젝트 초기 구조 세팅

### 핵심 발견
- 공공데이터포털이 **메타 API** (목록조회서비스)를 제공 → 전체 API 카탈로그 자동 수집 가능
- 각 API에 Swagger UI + schema.org 메타데이터 있음
- 기존 유사 프로젝트 (data-go-mcp-servers 231★)는 6개 API만 하드코딩 → 차별화 가능

### 다음 작업
- ~~PoC: 메타 API로 카탈로그 수집 파이프라인 구현~~ ✓
- ~~API 스펙 자동 파싱 검증~~ ✓

---

## 2026-03-31: Phase 1 MVP 구현 완료

### 완료
- 프로젝트 구조 재편: `catalog/`, `api/` → `core/` (types, catalog, swagger, caller)
- Config 관리: `config.toml` + 환경변수 `DATA_GO_KR_API_KEY` 우선순위 해결
- 카탈로그 수집: 메타 API 페이지네이션으로 12,080개 API 서비스 수집
- 카탈로그 검색: 제목/설명/키워드/기관명 텀 매칭 + 인기도 가중치
- Swagger 파싱: data.go.kr 상세페이지 스크래핑 → Swagger 2.0 JSON 정규화
- API 호출 엔진: GET query / POST JSON body 빌드, 인증 주입, 응답 추출
- MCP 서버: stdio JSON-RPC (search_api, get_api_spec, call_api 3개 도구)
- 31개 테스트 (단위 + 통합), clippy/fmt 클린

### 핵심 발견/결정
- **edition 2024 → 2021 다운그레이드**: Rust edition 2024는 아직 일부 크레이트와 호환 이슈
- **번들 카탈로그 보류**: 수집된 catalog.json이 17MB로 바이너리 임베딩에 부적합. 향후 경량화 방안 필요 (필드 제거, 압축, 또는 첫 실행 시 자동 다운로드)
- **data.go.kr User-Agent 필수**: User-Agent 없이 요청하면 Connection reset. `korea-cli/0.1.0` 설정으로 해결
- **serial_test 필요**: Config 테스트가 환경변수를 조작하므로 병렬 실행 시 flaky. `serial_test` 크레이트로 직렬화
- **Swagger spec body fields 빈 경우 있음**: 일부 API의 Swagger가 body parameter에 schema properties를 포함하지 않음. MVP에서는 허용

### 다음 작업
- ~~Claude Desktop / Cursor MCP 연동 테스트~~
- ~~번들 카탈로그 경량화 방안 설계~~
- Phase 2: apis.data.go.kr 호스팅 API 지원 (DataGoKrRest 프로토콜)

---

## 2026-03-31: Codex 실사용 테스트 — 문제점 발견

### 핵심 발견
Codex가 실제 API를 search → spec → call 흐름으로 사용하면서 다수 문제 확인.
**한 줄 요약: 카탈로그 검색기로는 쓸 만하지만, 범용 API 실행기로는 스펙 추출과 호출 추상화가 부족.**

### 문제 목록

1. **Swagger URL 스크래핑 실패 (BLOCKER)**
   - `swagger.rs:301`이 `var swaggerUrl = '...'` 패턴만 찾음
   - 에어코리아 등 다수 API에서 swaggerUrl이 빈 문자열 → `spec` 명령 실패
   - `spec 15073861`, `spec 15073877` 모두 `Could not find swaggerUrl` 에러
   - **결정: 사용자 측 실시간 스크래핑을 폐기하고, CI 사전 수집 + 번들 배포로 전환**

2. **호출 엔진이 JSON 전용 (Phase 1.1)**
   - `caller.rs:45~59`: POST는 무조건 JSON body, 응답도 무조건 JSON 파싱
   - XML 응답, form-urlencoded 등 공공 API 다수가 깨질 가능성

3. **인증 처리 일반화 부족 (Phase 1.1)**
   - `caller.rs:99`: AuthMethod::Both + Header 선호 시 실제 헤더 미부착 (버그)
   - `swagger.rs:132`: Both/Header 접두사가 `Infuser ` 하드코딩 — 일반 규칙 아님

4. **사용자 입력 정규화 없음 (Phase 1.1)**
   - 사업자번호 `220-81-62517` → "등록되지 않은 번호", `2208162517` → 정상
   - spec 기반 포맷 힌트/자동 변환 필요

### 결정
- 문제 1 → **번들 전환 설계** (이번 세션)
- 문제 2, 3, 4 → **Phase 1.1 로드맵에 추가** (호출 엔진 안정화)

### 다음 작업
- ~~번들 전환 설계 완료~~ ✓
- ~~구현 계획 수립~~ ✓

---

## 2026-03-31: Swagger spec 수집 가능성 재검증

### 핵심 발견
기존 코드(`swagger.rs:302`)가 `var swaggerUrl = '...'` 패턴만 찾고 있었으나,
**대부분의 API(~99%)는 `var swaggerJson = \`{...}\``로 인라인 Swagger JSON을 제공**한다.

- 200개 랜덤 샘플: swaggerUrl 있음 = **0개** (0%)
- 50개 랜덤 샘플: swaggerJson 인라인 있음 = **50개** (100%)
- 인기 상위 10개 중 swaggerUrl 있음 = 1개 (사업자등록 API만)

### 두 가지 패턴
1. `var swaggerJson = \`{...}\`` — 페이지 내 인라인 JSON (**대다수**)
2. `var swaggerUrl = 'https://infuser.odcloud.kr/...'` — 외부 URL 참조 (**극소수**)

### 결론
- 12,080개 API 거의 전체의 Swagger spec을 수집 가능
- CI 수집 파이프라인에서 두 패턴 모두 처리하면 번들에 전체 spec 포함 가능
- 기존 `extract_swagger_url()` → `swaggerUrl` + `swaggerJson` 두 패턴 모두 처리하도록 변경 필요

---

## 2026-03-31: 번들 전환 설계 + 구현 계획 완료

### 완료
- 번들 전환 설계 스펙 작성: `docs/specs/2026-03-31-bundle-transition-design.md`
- 구현 플랜 작성 (11 Task, TDD 기반): `docs/plans/2026-03-31-bundle-transition.md`
- docs 경로 통일: spec → `docs/specs/`, plan → `docs/plans/`

### 핵심 결정
- **직렬화**: postcard (serde 호환, JSON 대비 40-60% 작음)
- **압축**: zstd level 3 (gzip 대비 10x 빠른 해제)
- **임베딩**: `include_bytes!` + build.rs (placeholder 자동 생성)
- **글로벌 접근**: `once_cell::Lazy<Bundle>` (첫 접근 시 1회 해제)
- **오버라이드 체인**: 로컬 bundle.zstd > 내장 번들
- **번들 크기 추정**: ~2-3 MB (raw 18MB → postcard 11MB → zstd 압축)

### 다음 작업
- ~~번들 전환 구현 (플랜 Task 1~11)~~ ✓

---

## 2026-03-31: 번들 전환 구현 완료

### 완료
- Bundle 타입 + postcard/zstd 직렬화 (Task 1)
- build.rs placeholder 번들 자동 생성 (Task 2)
- Bundle override path (Task 3)
- Bundle loader + once_cell global static (Task 4)
- extract_swagger_json 인라인 파싱 (Task 5)
- search_bundle_catalog CatalogEntry 기반 검색 (Task 6)
- CLI/MCP 번들 기반 전환 (Task 7)
- Update command GitHub Releases 다운로드 (Task 8)
- Bundle builder binary — 전체 API spec 수집 (Task 9)
- 실제 번들 생성 + E2E 검증 (Task 10)
- 레거시 스크래핑 코드 제거 + 문서 업데이트 (Task 11)

### 핵심 결과
- **12,080 API 카탈로그 + 5,363 Swagger 스펙** 번들 내장
- 번들 크기: **2.77 MB** (postcard + zstd level 3)
- 오프라인 search/spec 즉시 사용 가능
- spec 수집률 44.4% (REST API가 아닌 데이터셋도 포함된 전체 목록 대비)

### 다음 작업
- ~~spec 미수집 API 분석~~ ✓
- CI 수집 파이프라인 (GitHub Actions cron)
- spec 수집률 개선
- Phase 1.1 호출 엔진 안정화 (XML, 인증 일반화)

---

## 2026-04-01: Swagger spec 미수집 API 심층 분석

### 배경
번들에 12,080 카탈로그 + 5,363 spec이 수집되었지만, 44.4% 수집률의 실체를 파악할 필요.

### 핵심 발견: 유효 spec은 3,960개 (32.8%)

수집된 5,363 spec 중 **~1,400개가 skeleton placeholder** (`host:""`, `paths:{}`) — 오퍼레이션 0개로 실질 무용.

### data.go.kr 3-tier 호스팅 구조

| Tier | 호스트 | Swagger 상태 | 수량 |
|------|--------|-------------|------|
| **1. Infuser** | `api.odcloud.kr` | `swaggerUrl`로 제공 | ~32 |
| **2. Gateway** | `apis.data.go.kr` | `swaggerJson` 인라인 (선택적) | ~3,960 유효 / ~1,200 미생성 |
| **3. External** | 각 기관 서버 | 없음 | ~5,500+ |

### 12,080개 전체 분류

| 유형 | 추정 수량 | 비율 | 설명 |
|------|----------|------|------|
| **유효 Swagger** | ~3,960 | 32.8% | 작동하는 spec, 번들 포함 |
| **Skeleton Swagger** | ~1,400 | 11.6% | 빈 host/paths — 제거 필요 |
| **Gateway API인데 Swagger 미생성** | ~1,200 | 9.9% | `apis.data.go.kr` endpoint 있지만 포탈이 Swagger 안 만듦 |
| **외부 포탈 링크** | ~2,500 | 20.7% | 서울열린데이터, vworld, tour.go.kr 등 |
| **카탈로그 전용** | ~2,500 | 20.7% | endpoint URL 없이 문서 링크만 |
| **WMS/WFS 공간 서비스** | ~89 | 0.7% | OGC 프로토콜, REST 아님 |
| **기타** | ~430 | 3.6% | undefined host, 기타 비정형 |

### 인기도 역전 현상

가장 인기 있는 API들이 spec이 없는 그룹에 집중:
- Swagger 있음: 평균 104 요청, 중앙값 20
- Swagger 없음: **평균 322 요청, 중앙값 68** (3배 더 인기)

상위 미수집: 기상청 단기예보 (53,803), 에어코리아 대기질 (51,347), 환율정보 (37,541), 지하철 실시간 (21,584)

### 개선 전략 (ROI 순)

1. **Skeleton 정리**: 번들에서 `operations.is_empty()` spec 제거 또는 `spec_status` 태깅
2. **CatalogEntry에 endpoint_url 추가**: 메타 API에 `endpoint_url`이 이미 있지만 번들 빌더가 버리고 있음. 추가하면 외부 링크 API도 원본 URL 안내 가능
3. **HTML 테이블 파싱** (ROI 최고): `apis.data.go.kr` endpoint가 있는 ~1,200개의 openapi.do 페이지에서 오퍼레이션 테이블 HTML 파싱 → 파라미터/응답 추출
4. **외부 API 스코프 아웃**: 카탈로그 검색에는 표시하되, spec/call 미지원 → endpoint_url로 외부 포탈 링크 안내

### 결정
- **CatalogEntry에 endpoint_url 포함하기로 결정** (2026-04-01)
  - 근거: skeleton/외부 링크 API에서 "이 API는 외부 포탈에서 제공됩니다: {url}" 안내 가능
  - 번들 크기 영향 미미 (URL 문자열 12K개 추가)
  - Swagger 미생성 gateway API도 endpoint URL을 알면 향후 HTML 파싱과 결합 가능

### 다음 작업
- ~~CatalogEntry에 endpoint_url 필드 추가 + 번들 빌더 반영~~ ✓
- ~~skeleton spec 필터링 로직 추가~~ ✓
- ~~HTML 테이블 파싱 PoC~~ ✓

---

## 2026-04-01: Spec 품질 개선 구현 완료

### 완료
- SpecStatus enum (Available/Skeleton/HtmlOnly/External/CatalogOnly/Unsupported)
- BundleMetadata에 schema_version 추가 (v1→v2), 구 번들 graceful fallback
- CatalogEntry에 endpoint_url, spec_status 필드 추가
- SearchEntry에 spec_status, endpoint_url 필드 추가
- 번들 빌더에서 skeleton spec 제거 + SpecStatus::classify() 자동 분류
- HTML 테이블 파서 모듈 (scraper 크레이트, data.go.kr AJAX 파싱)
- CLI/MCP에서 spec 없는 API 조회 시 상태 안내 + endpoint_url 제공
- build.rs placeholder 번들 스키마 v2 대응
- 90개 테스트 통과 (39 lib + 51 통합/바이너리)

### 핵심 결정
- **postcard 스키마 호환성**: schema_version 필드로 버전 관리, 역직렬화 실패 시 내장 번들 폴백. 크래시 없는 graceful degradation
- **SpecStatus 설계**: AI 에이전트가 1필드로 즉시 판단 가능한 단순 enum 채택. endpoint_url이 이미 있으므로 EndpointType::External(String) 패턴 불필요
- **scraper 크레이트 채택**: data.go.kr HTML 테이블 파싱용. regex 대비 구조적 파싱에 안정적
- **html_parser build_bundle 연결 보류**: 파서 모듈 + 테스트는 완성했으나, 실제 번들 빌더 연결은 후속 작업으로 분리 (네트워크 의존성)

### 다음 작업
- ~~API 전수조사 설계 + 구현~~ ✓
- html_parser를 build_bundle에 연결하여 ~1,200개 HtmlOnly API → Available 승격
- CI 수집 파이프라인 (GitHub Actions cron)
- Phase 1.1 호출 엔진 안정화 (XML, 인증 일반화)

---

## 2026-04-02: API 전수조사 (survey) 실행 + 결과 분석

### 배경
이전 세션에서 추정치로 분류했던 12K API의 실제 분포를 전수조사로 확인.
`src/bin/survey.rs` 바이너리를 구현하여 모든 openapi.do 페이지를 실제로 방문하고 분석.

### 완료
- survey 바이너리 구현 (7개 커밋, TDD 기반)
  - `SurveyResult` 구조체, `analyze_page`, `detect_anomalies`, `survey_single_api`, `main` 파이프라인
  - `--resume` 이어하기 기능, `--concurrency`/`--delay` 조절
- 전수조사 실행: 12,108개 API, 28.4분 소요
- 상세 결과 보고서 작성: `data/survey-report.md`

### 핵심 결과 — 실측 vs 이전 추정

| 분류 | 실측 | 이전 추정 | 차이 |
|------|---:|---:|------|
| Available | 3,949 | ~3,960 | 거의 일치 |
| Skeleton | 1,404 | ~1,400 | 거의 일치 |
| External | 4,652 | ~2,500 | **+2,152** (대폭 증가) |
| CatalogOnly | 2,101 | ~2,500 | -399 |
| HtmlOnly | 2 | ~1,200 | **-1,198** (사실상 소멸) |

### 핵심 발견

1. **HtmlOnly 사실상 소멸 (2건)**: 이전에 ~1,200건으로 추정했던 "Gateway API인데 Swagger 미생성" 카테고리가 실측 2건으로 급감. 원인은 endpoint_url이 `' '`(공백 1자)인 API 8,883건이 External로 분류된 것. `SpecStatus::classify`의 빈 URL 판정이 공백을 처리하지 않아서 발생.

2. **login_required anomaly 99.8% 오탐**: openapi.do 페이지의 공통 레이아웃에 login/session 관련 텍스트가 항상 포함. Swagger inline JSON은 로그인 없이도 렌더링되지만, HTML pk는 전혀 탐지되지 않음.

3. **swagger_json_var_but_unparsed 6,732건**: `swaggerJson` 변수명이 HTML에 존재하지만 값이 비어있는 케이스. External/CatalogOnly API에서 포탈이 변수만 선언하고 값을 채우지 않은 것으로 추정.

4. **endpoint_url 데이터 품질**: 73.4%가 `' '`(공백), 21.5%가 빈 문자열. `extract_domain`에서 trim 처리 누락.

### 개선 필요 항목
- `SpecStatus::classify`에서 endpoint_url trim 처리 → HtmlOnly 재분류
- `login_required` anomaly 로직 개선 (공통 레이아웃 제외)
- 브라우저 세션 로그인 후 HTML pk 탐지 재검증
- `survey.json`은 7.5MB이므로 `.gitignore`에 추가

### 다음 작업
- ~~HTML 스펙 추출 전수조사 (셀렉터 버그 수정 + AJAX 프로브)~~ ✓

---

## 2026-04-02: HTML 스펙 전수조사 — "로그인 벽" 오진 해결

### 배경
1차 전수조사에서 "login_required 99.8%, HTML pk 탐지 0건"이라는 결과가 나와 "로그인 벽" 때문에 HTML 스펙 추출이 불가능하다고 진단했으나, 사용자가 직접 브라우저에서 확인한 결과 로그인 벽이 없었다. 근본 원인을 조사하고 재전수조사를 실행.

### 핵심 발견: 로그인 벽이 아닌 셀렉터 버그

**1차 진단 (오진)**: "비로그인 상태에서 publicDataDetailPk가 렌더링되지 않음"
**실제 원인**: `html_parser.rs:159`의 CSS 셀렉터가 `input[name="publicDataDetailPk"]`를 찾고 있었으나, 실제 HTML은 `<input id="publicDataDetailPk" value="uddi:...">`로 **`id=` 속성**을 사용. 한 줄 버그로 12,108건 전부 탐지 실패.

추가 검증:
- `curl`로 비로그인 openapi.do 접근: HTTP 200, 완전한 HTML 반환
- `publicDataDetailPk` hidden input: `id=` 속성으로 정상 존재
- `selectApiDetailFunction.do` AJAX: **Referer 헤더만 추가하면 비로그인으로 100% 작동**
- **CI 완전 호환** — 브라우저 없이 순수 HTTP로 추출 가능

### 완료
- `html_parser.rs` 셀렉터 수정: `id=` 우선 → `name=` 폴백 → regex 폴백 (3단계)
- `html-survey` 바이너리 구현 (27개 테스트, 2-phase 조사)
  - Phase 1: openapi.do 페이지 분석 (pk/select/swagger)
  - Phase 2: AJAX 프로브 (`selectApiDetailFunction.do` 실제 호출)
- 전수조사 실행: 12,108 API, 26.2분 소요

### 전수조사 결과

| 항목 | 수 | 비율 |
|------|---:|-----:|
| pk 탐지 성공 (`id=`) | 12,098 | 99.9% |
| select 옵션 1개+ | 3,186 | 26.3% |
| AJAX 프로브 성공 (요청주소+파라미터) | 2,522 | 79.2% (대상 중) |
| AJAX 에러 | 0 | 0% |

**커버리지 개선:**

| 구분 | 수 | 비율 |
|------|---:|-----:|
| 현재 Available (Swagger) | 3,953 | 32.6% |
| **신규 추출 가능 (HTML AJAX)** | **2,522** | **20.8%** |
| **합계** | **6,475** | **53.5%** |
| 나머지 미커버 | 5,633 | 46.5% |

**Swagger와 HTML은 상호 배타적**: Swagger 있는 API는 select 옵션 0개, HTML select 있는 API는 Swagger 비어있음. 2경로 폴백 체인으로 설계 가능.

**미커버 5,633건 분류:**
- operation 미등록 (pk_no_options): 4,899 — 기관이 데이터 입력 안 함
- 폐기/서비스 종료: 282
- AJAX 부분 성공 (서비스URL만): 662 — 부분 추출 가능성
- Skeleton: 1,405 — select 옵션도 0개, 진정한 빈 등록 (pk_no_options와 별개)
- pk 미발견: 10 — JS 리다이렉트/네트워크 에러

### 핵심 결정
- **1차 전수조사의 "로그인 벽" 진단은 폐기** — 실제 원인은 CSS 셀렉터 버그
- **build_bundle에 HTML 폴백 추가**: Swagger 실패 → pk+AJAX → parse_operation_detail → 스펙 생성
- **AJAX 호출에 Referer 헤더 필수**: `https://www.data.go.kr/data/{list_id}/openapi.do`
- **td_fallback 파라미터 스타일이 100%**: AJAX 응답에서 `data-paramtr-nm` 사용 0건

### 다음 작업
- ~~build_bundle.rs에 HTML 폴백 경로 구현~~ **보류** — 전수조사 재설계 필요
- 전수조사 스크립트 재설계 (아래 참고)

### 발견: 2차 전수조사도 불완전

pk_no_options(4,899건)을 "operation 미등록"으로 분류했으나, 랜덤 30건 샘플 검증에서 **28건(93%)이 외부 포탈 URL을 보유**하고 있었다. 경기도 데이터(data.gg.go.kr), 도로공사, 농사로, 제주데이터허브, vworld, 법제처 등 실제 작동하는 외부 포탈 링크가 페이지 HTML `<a>` 태그에 존재.

**근본 원인**: 1차·2차 전수조사 모두 "특정 패턴을 확인하는" 방식으로 설계. Swagger/pk/select/AJAX만 체크하고 **페이지에 실제로 뭐가 있는지**는 스캔하지 않았다. 확증 편향(confirmation bias) — 기존 가설에 맞는 신호만 찾은 것.

**필요한 접근 전환**: 패턴 확인 → **발견 기반 전수조사**. 페이지의 모든 유의미한 신호(외부 링크, 테이블, 폼, JS 변수, 섹션 구조)를 먼저 수집하고, 거기서 패턴을 도출해야 한다.

---

## 2026-04-02: openapi.do 페이지 크롤러 구현

### 배경
두 차례 전수조사(survey.rs, html_survey.rs)가 확증 편향으로 실패. "예상 패턴이 있는지 확인"이 아닌 "페이지에 실제로 뭐가 있는지 발견"하려면 원본 HTML이 필요. **수집과 분석을 분리**하여, 분석 로직 변경 시 재크롤링 없이 로컬 HTML을 반복 분석할 수 있도록 설계.

### 완료
- 설계 문서: `docs/plans/2026-04-02-crawl-pages-design.md`
- `src/bin/crawl_pages.rs` 구현 (survey.rs 패턴 기반)
  - 메타 API로 12K+ list_id 수집 → `buffer_unordered` 병렬 다운로드
  - 출력: `data/pages/{list_id}.html`
  - `--resume`: 기존 파일 스캔 후 스킵
  - `--concurrency` (기본 5), `--delay` (기본 100ms)
  - 실패 목록 `data/crawl_failures.json`으로 저장
  - 매 건 진행 로그 + 최종 요약 (Total/Skipped/Success/Failed)
- 4개 단위 테스트 통과, clippy/fmt 클린

### 핵심 결정
- **수집/분석 분리**: 원본 HTML을 로컬 저장하면 분석 로직(셀렉터, 파싱 규칙)을 변경해도 재크롤링 불필요. 이전 전수조사에서 셀렉터 버그로 전체 재실행이 필요했던 경험에서 학습.
- **분석 도구는 별도 구현**: crawl_pages는 순수 수집만 담당. 분석 스크립트는 후속 작업으로 분리.

### 다음 작업
- 크롤링 실행 (12K+ 페이지, ~30분 예상)
- 로컬 HTML 분석 도구 구현 (발견 기반 — 모든 유의미한 신호 추출)
- 분석 결과 기반 build_bundle HTML 폴백 재설계
