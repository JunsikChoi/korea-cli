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

| Tier            | 호스트            | Swagger 상태                  | 수량                        |
| --------------- | ----------------- | ----------------------------- | --------------------------- |
| **1. Infuser**  | `api.odcloud.kr`  | `swaggerUrl`로 제공           | ~32                         |
| **2. Gateway**  | `apis.data.go.kr` | `swaggerJson` 인라인 (선택적) | ~3,960 유효 / ~1,200 미생성 |
| **3. External** | 각 기관 서버      | 없음                          | ~5,500+                     |

### 12,080개 전체 분류

| 유형                               | 추정 수량 | 비율  | 설명                                                     |
| ---------------------------------- | --------- | ----- | -------------------------------------------------------- |
| **유효 Swagger**                   | ~3,960    | 32.8% | 작동하는 spec, 번들 포함                                 |
| **Skeleton Swagger**               | ~1,400    | 11.6% | 빈 host/paths — 제거 필요                                |
| **Gateway API인데 Swagger 미생성** | ~1,200    | 9.9%  | `apis.data.go.kr` endpoint 있지만 포탈이 Swagger 안 만듦 |
| **외부 포탈 링크**                 | ~2,500    | 20.7% | 서울열린데이터, vworld, tour.go.kr 등                    |
| **카탈로그 전용**                  | ~2,500    | 20.7% | endpoint URL 없이 문서 링크만                            |
| **WMS/WFS 공간 서비스**            | ~89       | 0.7%  | OGC 프로토콜, REST 아님                                  |
| **기타**                           | ~430      | 3.6%  | undefined host, 기타 비정형                              |

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

| 분류        |  실측 | 이전 추정 | 차이                     |
| ----------- | ----: | --------: | ------------------------ |
| Available   | 3,949 |    ~3,960 | 거의 일치                |
| Skeleton    | 1,404 |    ~1,400 | 거의 일치                |
| External    | 4,652 |    ~2,500 | **+2,152** (대폭 증가)   |
| CatalogOnly | 2,101 |    ~2,500 | -399                     |
| HtmlOnly    |     2 |    ~1,200 | **-1,198** (사실상 소멸) |

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

| 항목                                 |     수 |            비율 |
| ------------------------------------ | -----: | --------------: |
| pk 탐지 성공 (`id=`)                 | 12,098 |           99.9% |
| select 옵션 1개+                     |  3,186 |           26.3% |
| AJAX 프로브 성공 (요청주소+파라미터) |  2,522 | 79.2% (대상 중) |
| AJAX 에러                            |      0 |              0% |

**커버리지 개선:**

| 구분                           |        수 |      비율 |
| ------------------------------ | --------: | --------: |
| 현재 Available (Swagger)       |     3,953 |     32.6% |
| **신규 추출 가능 (HTML AJAX)** | **2,522** | **20.8%** |
| **합계**                       | **6,475** | **53.5%** |
| 나머지 미커버                  |     5,633 |     46.5% |

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

### 크롤링 실행 결과

```
Total:   12,108
Success: 12,108 (실패 0건)
Size:    2.0 GB (페이지당 평균 ~168KB)
소요:    ~22분 (중단 1회 + resume)
```

- 속도: ~10건/초 (concurrency=5, delay=100ms)
- `--resume` 정상 작동 확인: 중단 후 3,195건 스킵 → 나머지 8,913건 이어받기
- `data/pages/`, `data/crawl_failures.json` → `.gitignore` 추가

### 핵심 발견: 페이지 92%가 공통 템플릿

- 페이지당 ~157KB 중 ~144KB(92%)가 data.go.kr 공통 레이아웃
- 고유 콘텐츠는 ~13KB뿐 (swagger, pk, select, 외부 링크 등)
- 사전 필터링은 하지 않기로 결정 — 확증 편향 방지를 위해 원본 보존

### 다음 작업

- ~~로컬 HTML 분석 도구 구현 (발견 기반 — 모든 유의미한 신호 추출)~~ ✓

---

## 2026-04-02: HTML 패턴 발견 도구 구현 + 실행

### 배경

이전 전수조사들이 확증 편향으로 실패한 교훈을 반영하여, "가정 없이" 모든 구조적 신호를 추출하는 2단계 파이프라인을 구현. 크롤링된 12,108개 HTML 파일을 입력으로 사용.

### 완료

- `src/bin/analyze_pages.rs` — 1단계: HTML 구조 신호 추출기
  - DOM 추출 (태그 카운트, id, name, class, data-*, th, select/option)
  - JS 추출 (변수 선언 regex, AJAX URL, JSON-LD, script type)
  - 메타 추출 (meta name/property/http-equiv, link rel)
  - 6개 단위 테스트 + 실제 HTML 통합 테스트
- `src/bin/summarize_signals.rs` — 2단계: 빈도 분석 + 클러스터링
  - 요소별 빈도 집계, 변별 신호 추출 (universal 제외)
  - js_var_types 기반 클러스터링 (95% 미만만 key signal로 사용)
  - 3개 단위 테스트
- 전체 파이프라인 실행 완료
- clippy/fmt 클린, 9개 테스트 전체 통과

### 실행 결과

**1단계 (analyze-pages)**: 12,108개 분석, 35초 소요 → `data/page_raw_signals.json`
**2단계 (summarize-signals)**: 빈도 분석 + 클러스터링 → `data/signal_summary.json` (93KB)

### 핵심 발견: 8개 클러스터

| 클러스터 | 파일 수 | 비율 | 핵심 신호 |
|----------|---------|------|-----------|
| **Swagger+Slider** | 4,010 | 33.1% | `swaggerJson:object` + slide UI |
| **No-Swagger** | 3,399 | 28.1% | `swaggerJson:empty`, `html:other` |
| **Gateway API** | 3,165 | 26.1% | `swaggerJson:empty` + `apiObj`/`paramList` |
| **Swagger-Other** | 1,397 | 11.5% | `swaggerJson:object`, `html:other` |
| **Undefined** | 75 | 0.6% | `swaggerJson:undefined` |
| **SwaggerURL** | 32 | 0.3% | `swaggerUrl:string` (Infuser API) |
| **Gateway-Var** | 22 | 0.2% | `apiObj` 패턴 변형 |
| **Error** | 8 | 0.1% | 에러 페이지 (JS 변수 없음) |

**Swagger 3변형**: `swaggerJson` = object(5,407), empty(6,618), undefined(75)
**555개 변별 신호**: universal(12,100+) 요소 제외 후 나머지

### 통계 요약

- ids: 121종류, classes: 213종류, js_var_types: 79종류
- meta_keys: 11종류 (거의 전부 공통)
- 76개 id가 12,100+ 파일에 출현 (공통 템플릿)

### 다음 작업

- ~~signal_summary.json을 분석하여 패턴 그룹 확정~~ ✓ (설계 스펙으로 정리)
- ~~각 그룹과 현재 SpecStatus 매핑 비교~~ ✓
- ~~build_bundle.rs 분류 로직 개선~~ ✓

---

## 2026-04-03: Gateway API 스펙 추출 구현

### 배경

HTML 패턴 발견에서 확인된 3개 패턴(Swagger inline, Swagger URL, Gateway AJAX)을 build_bundle.rs에 분류 우선 파이프라인으로 통합. Gateway API ~3,187개에서 AJAX로 ApiSpec을 추출하여 Available API 수를 +79% 증가시키는 것이 목표.

### 완료

- `ClassificationHints` 구조체 도입 (types.rs) — positional bool → named fields, `is_link_api` 지원
- `PageInfo` 확장 (html_parser.rs) — `ty_detail_code`, `public_data_pk` 추출
- `ParsedOperation`에 `summary` 필드 추가 (h4.tit 추출)
- 응답 필드 파싱 h4 기반 전략 + tr-scan fallback
- 빈 요청주소 service_url fallback (Operation 누락 방지)
- `build_bundle.rs` 전면 개편:
  - `SpecResult` enum (Spec/Bail with metadata)
  - 분류 우선 파이프라인: tyDetailCode → Swagger inline → Swagger URL → Gateway AJAX
  - `fetch_gateway_spec`: cookie-isolated reqwest client, Semaphore rate limiting
  - LINK API (PRDE04) 즉시 External 분류
  - `--ajax-concurrency`, `--ajax-delay` 파라미터
- reqwest `cookies` feature 추가 (Cargo.toml)
- 8개 신규 테스트, 전체 159 테스트 통과

### 핵심 결정

- **쿠키 격리**: Gateway API마다 독립 reqwest::Client 생성. data.go.kr AJAX 엔드포인트가 세션 기반 응답을 반환할 가능성 대비. E2E 검증 후 불필요하면 제거 예정
- **Semaphore rate limiting**: permit을 send+sleep 동안 보유, body read 전 해제. 서버 부하는 요청 시점에 발생하므로 이 순서가 적절
- **SpecResult vs Result<ApiSpec>**: bail 시 `is_link_api` 힌트를 분류에 전달해야 하므로 전용 enum 채택. Box<ApiSpec>으로 enum 사이즈 최적화
- **ResponseFormat::Xml 하드코딩**: Gateway API의 기본 응답 형식. JSON 감지는 후속 작업

### 리뷰 결과

- /review (backend-architect + Claude 교차검증): BLOCK 1건 해소 (regex LazyLock)
- /eval (3명 전문가 병렬): BLOCK 0건, WARNING 4건 수정 (parse error 로깅, HTTP 상태 확인, publicDataPk fallback 경고, ResponseFormat TODO)

### 다음 작업

- ~~번들 리빌드 + 커버리지 검증~~ ✓
- 소규모 E2E 검증 (실제 data.go.kr에서 Gateway API 3-5개 테스트)
- 쿠키 격리 필요 여부 검증 (불필요하면 공유 client로 전환)
- ResponseFormat JSON 감지 구현

---

## 2026-04-03: 번들 리빌드 — Gateway AJAX 통합 결과

### 배경

Gateway API 스펙 추출 파이프라인(분류 우선 + AJAX 추출) 구현 완료 후 첫 번째 전체 번들 리빌드. 기존 Swagger 전용 번들 대비 커버리지 변화를 검증.

### 실행

```bash
cargo run --bin build-bundle -- \
  --api-key $DATA_GO_KR_API_KEY \
  --concurrency 5 --delay 100 \
  --ajax-concurrency 10 --ajax-delay 50 \
  --output data/bundle-gateway.zstd
```

소요: 39.6분, 12,119 서비스 대상

### 결과

| 분류 | 기존 번들 | 새 번들 | 변화 |
|------|----------|---------|------|
| **Available** | ~4,042 | **7,160** | **+77%** |
| **External** | ~4,652 | **4,942** | +290 |
| **Skeleton** | ~1,404 | **8** | -1,396 (대부분 재분류) |
| **CatalogOnly** | ~2,101 | **9** | -2,092 (대부분 재분류) |
| **HtmlOnly** | 2 | **0** | 소멸 |
| **총 API** | ~12,080 | **12,119** | +39 (카탈로그 증가) |

번들 크기: 2.9 MB → 4.2 MB (+45%)

### 핵심 수치

- **Gateway AJAX 추출 성공: 3,125건** — 목표 ~2,500-3,000 초과 달성
- **LINK API (PRDE04) External 분류: ~4,942건** — 정상 동작
- **Skeleton/CatalogOnly 급감**: 분류 우선 파이프라인이 이전 분류 로직보다 정밀. tyDetailCode 기반 분류가 endpoint_url 기반보다 정확
- **FAIL 4,902건**: pk 미발견(PARSE_ERR), 페이지 요청 실패(SKIP), operation 미등록 등. 기술적으로 추출 불가능한 API들

### 커버리지 해석

| 구분 | 수 | 비율 |
|------|---:|-----:|
| 호출 가능 (Available) | 7,160 | 59.1% |
| 외부 포탈 (External) | 4,942 | 40.8% |
| 미분류 (Skeleton+CatalogOnly) | 17 | 0.1% |

**호출 가능 API가 59.1%로 향상** — 이전 32.6%(Swagger only) → 53.5%(설계 목표) → 59.1%(실측). 설계 목표를 초과 달성한 이유는 AJAX 부분 성공 662건 중 상당수가 실제로도 성공했기 때문으로 추정.

### 다음 작업

- bundle-gateway.zstd를 기본 번들로 교체 (내장 번들 업데이트)
- AJAX 부분 성공 케이스 추가 분석 (서비스URL 기반)
- 쿠키 격리 필요 여부 검증
- CI 수집 파이프라인 (GitHub Actions cron)

---

## 2026-04-03: API 카탈로그 문서 생성기 구현

### 완료

- `src/bin/gen_catalog_docs.rs` 바이너리 구현 (7 commits, TDD 기반)
  - 번들(.zstd) 로드 → CatalogEntry를 org_name으로 그룹핑
  - `docs/api-catalog/README.md` (통계 요약 + 기관별 목차, request_count 내림차순)
  - `docs/api-catalog/by-org/{org}.md` (기관별 Available/External/기타 섹션)
  - ID를 `data.go.kr/data/{list_id}/openapi.do` 클릭 링크로 생성
  - eval 3 rounds (architect-reviewer + code-reviewer + Codex 교차검증) 통과
- 406개 기관, 12,119 API 문서 생성
- `.gitattributes` linguist-generated 설정 (PR diff 자동 접기)
- 프로젝트 README에 카탈로그 링크 추가

### 방어 로직 (eval 반영)

- `sanitize_filename`: 기관명 → 파일시스템 안전 이름 변환
- `org_safe_filename`: sanitize 결과 빈 문자열 시 `_org_{list_id}` fallback
- 파일명 충돌 감지 (`seen_filenames` HashMap, 충돌 시 bail)
- `escape_md_table`: `|`, `\r`, `\n` 이스케이프
- External URL `>` → `%3E` percent-encode + angle bracket 링크

### 발견된 이슈: External API의 endpoint_url 누락

카탈로그 문서 생성 중 **External API 4,942건의 endpoint_url이 전부 빈 값**(`" "` 또는 `""`)인 것을 확인. External 섹션의 "링크" 컬럼이 전부 `—`으로 표시되는 원인.

**데이터 흐름 추적:**

```
메타 API (api.odcloud.kr)
  → endpoint_url: " " (빈 값으로 반환)
  → build_bundle.rs: svc.endpoint_url을 CatalogEntry에 그대로 복사 (108행)
  → SpecStatus::classify: is_link_api=true → External 판정
  → 번들: endpoint_url=" ", spec_status=External
  → CLI spec 조회: endpoint_url=" ", message="외부 포탈에서 제공하는 API입니다"
  → gen-catalog-docs: url.trim().starts_with("http") → false → "—" 표시
```

**근본 원인**: 메타 API가 External(LINK) API의 실제 외부 포탈 URL을 제공하지 않음. 이전 전수조사(2026-04-02)에서도 확인된 사실:

- devlog "endpoint_url 데이터 품질: 73.4%가 `' '`(공백)" (296행)
- devlog "pk_no_options 4,899건 중 93%가 외부 포탈 URL을 보유" (379행)

**실제 외부 URL은 존재하지만 수집하지 않고 있음:**

data.go.kr 상세 페이지(`/data/{list_id}/openapi.do`) HTML에는 외부 포탈 링크가 `<a>` 태그로 존재. 이전 크롤링(12,108 페이지, `data/pages/`)에서 이미 확인. 그러나 build_bundle.rs의 현재 추출 경로(Swagger → Gateway AJAX)에는 외부 URL 수집 단계가 없음.

**영향 범위:**

| 항목 | 영향 |
|------|------|
| CLI `spec` 조회 | External API에 "외부 포탈" 안내만, 실제 URL 미제공 |
| MCP 서버 | AI가 External API의 실제 접속처를 모름 |
| 카탈로그 문서 | External 섹션 링크 전부 `—` (ID 클릭으로 data.go.kr 우회 가능) |

**해결 방향:**

build_bundle.rs에 외부 URL 수집 단계 추가:
1. LINK API (PRDE04)로 분류된 list_id에 대해 openapi.do 페이지 요청
2. HTML에서 외부 포탈 `<a>` 태그 추출 (이미 data/pages/에 크롤링 데이터 존재)
3. 추출된 URL을 CatalogEntry.endpoint_url에 저장
4. 번들 리빌드 시 External API의 endpoint_url이 실제 외부 URL로 채워짐

기존 크롤링 데이터(`data/pages/`)를 활용하면 네트워크 요청 없이 로컬에서 추출 가능.

### 다음 작업

- ~~External API 외부 URL 수집 로직 구현 (build_bundle.rs 또는 별도 바이너리)~~ ✓
- bundle-gateway.zstd를 기본 번들로 교체
- 쿠키 격리 필요 여부 검증
- CI 수집 파이프라인

---

## 2026-04-03: External API endpoint_url 추출 구현

### 완료

- `html_parser.rs`: `PageInfo`에 `external_url: Option<String>` 필드 추가, `a.link-api-btn[href]` 셀렉터로 외부 포탈 URL 추출
- `build_bundle.rs`: `SpecResult::ExternalLink` variant 추가, PRDE04 감지 시 외부 URL과 함께 반환
- `build_bundle.rs`: `external_urls` HashMap으로 `CatalogEntry.endpoint_url` 오버라이드 — classify에도 방어적 전달
- `Bail`에서 `is_link_api` 필드 제거 (PRDE04 경로가 ExternalLink로 이전됨)
- `link_count` 별도 카운터 분리 (fail_count 오염 방지)
- 단위 테스트 4개: 유효 href, 버튼 없음, javascript:void(0), &amp; 디코딩

### 핵심 결정

- **Bail에 필드 추가 vs ExternalLink variant**: 새 variant를 선택. 12곳+ bail 사이트에 보일러플레이트를 추가하는 것보다 의미론적으로 명확
- **추가 네트워크 요청 없음**: build_bundle.rs가 이미 openapi.do 페이지를 fetch하는 시점에서 PRDE04를 감지하므로, 같은 HTML에서 URL을 추출하면 추가 요청 불필요

### 다음 작업

- ~~번들 리빌드하여 External API endpoint_url 커버리지 확인~~ ✓ (이전 세션에서 완료)
- ~~카탈로그 문서 재생성~~ ✓
- bundle-gateway.zstd를 기본 번들로 교체
- ~~CI 수집 파이프라인~~ ✓

---

## 2026-04-04: PartialStub + CI 수집 파이프라인 구현

### 배경

Gateway API AJAX 추출에서 부분 성공(일부 operation만 성공)하는 케이스가 존재했으나, 기존 분류에서는 Available과 동일하게 취급하거나 Bail로 버려졌다. 부분 성공을 명시적으로 분류하고, 실패분을 자동 재수집하며, CI 크론으로 전체 파이프라인을 자동화하는 것이 목표.

### 완료

- `SpecStatus::PartialStub` variant 추가 (postcard varint 끝에, schema v2→v3)
- `ClassificationHints`에 `is_partial` 필드 추가, `classify()`에서 `is_partial && has_spec` → PartialStub
- `FailedOp`/`ErrorType` 타입 — AJAX 실패를 NetworkTimeout/RateLimited/BodyReadError/ParseError/ConnectionError로 분류
- `SpecResult`에 `is_partial`, `failed_ops` 필드 추가
- `fetch_gateway_spec`에서 실패 operation을 FailedOp으로 수집 + is_partial 감지
- main()에서 partial_ids 추적 + `data/failed_ops.json` 출력
- `--retry-stubs` 플래그: failed_ops.json 기반 재수집 + operation merge (기존 spec base, 신규 operation 추가)
- CLI/MCP에서 PartialStub 안내 메시지 (누락 operation 시 available_operations 반환)
- `update.rs`에 schema_version 검증 + atomic 파일 교체 (tmp → rename)
- `.github/workflows/bundle-ci.yml` — 주 1회 크론 (UTC 토 17:00), 수집 → retry → 변경 감지 → 문서 업데이트 → Release
- 번들 저장 atomic 화 (main + run_retry 모두 tmp → rename)
- 6개 신규 PartialStub 테스트, 전체 테스트 통과

### 핵심 결정

- **PartialStub은 is_callable()=true**: 존재하는 operation은 호출 가능. 누락된 operation만 다음 업데이트에서 복구
- **classify()에서 `is_partial && has_spec` 조건**: has_spec 없이 is_partial만으로 PartialStub이 되는 것 방지 (pub API 방어)
- **merge_operations base를 existing으로**: retry 시 기존 메타데이터(auth, base_url 등) 보존. 새 operation만 추가
- **ParseError는 retry 불가**: failed_ops.json에서 ParseError 타입은 필터링하여 재시도하지 않음 (서버 HTML 구조 문제)
- **retry에서 PartialStub → catalog status 승격**: Bail이었던 API가 retry에서 PartialStub Spec으로 바뀌면 catalog도 PartialStub으로 업데이트

### Eval 결과

- 3명 전문가 병렬 검증 (architect-reviewer, backend-architect, deployment-engineer)
- Round 1: BLOCK 2건 + WARNING 6건 수정
- Round 2: Codex 교차검증 — BLOCK 2건 추가 발견 + 수정
- 최종: BLOCK 0건, WARNING 3건 미반영 (대규모 변경 필요)

### 다음 작업

- bundle-gateway.zstd를 기본 번들로 교체 (v3 schema로 재빌드)
- gen_catalog_docs에서 PartialStub을 Available 섹션에 포함 (현재 "기타" 분류)
- ~~DATA_GO_KR_API_KEY GitHub secret 설정 후 CI 첫 실행~~ ✓
- E2E 테스트: 실제 PartialStub API에서 available operation 호출 확인

---

## 2026-04-05: CI 수집 파이프라인 실 테스트 + 2건 수정

### 배경

PartialStub + CI 파이프라인 구현 완료 후, `workflow_dispatch`로 첫 실 테스트 수행. 로컬 환경(40분)과 달리 GitHub Actions 환경에서 예상치 못한 이슈 2건 발생.

### 이슈 1: timeout 초과 (90분 → 150분)

**1차 실행 결과**: 90분 timeout에서 `8,500/12,135 (70%)` 진행 상태로 중단.

- CI 환경(미국 데이터센터) → 한국 data.go.kr 네트워크 레이턴시가 로컬 대비 크게 높음
- 로컬 수집 속도: ~300건/분 (concurrency=5) vs CI: ~100건/분
- FAIL 비율도 로컬 4% → CI 13.4%로 증가 (네트워크 타임아웃 증가)

**수정**:
- `timeout-minutes: 90 → 150`
- `--concurrency 5 → 10`, `--delay 100ms → 50ms`
- `--max-retry-time 600s → 900s`

2차 실행: **57분 완료** (concurrency 2배로도 수집 자체는 ~60분 소요. Rate limiting이 병목)

### 이슈 2: 문서 커밋 단계 `git pull --rebase` 실패

**에러**: `cannot pull with rebase: You have unstaged changes.`

**원인**: gen-catalog-docs가 생성한 파일이 unstaged 상태에서 `git pull --rebase`를 먼저 실행.

**수정**: 커밋 순서 변경

```yaml
# Before: pull rebase → add → commit → push (실패)
# After:  add → commit → pull rebase → push (성공)
```

### 핵심 결정

- **CI 환경 네트워크 특성은 로컬과 다르다**: 로컬 벤치마크로 timeout을 산정하면 안 됨. 미국 DC → 한국 서버 RTT가 병목. 여유롭게 1.5~2배 buffer 필요.
- **실패한 step만 재실행은 불가**: GitHub Actions의 `gh run rerun --failed`는 job 단위 재실행. 단일 job 워크플로우에서는 전체 재실행뿐. 나중에 collect/publish job 분리를 고려할 수 있지만, 아티팩트 전달 복잡도가 추가됨.
- **Rate limiting이 concurrency 증가의 한계**: 5 → 10으로 올려도 수집 시간이 절반으로 줄지 않음. data.go.kr 서버 측 throttling 추정.

### 결과

- CI 첫 실행 성공: 57분 완료 (수집 + 재시도 + 변경 감지 + 문서 재생성 + Release 생성)
- GitHub Release: `bundle-2026-04-04-4` 생성 + `bundle.zstd` 업로드
- 자동 커밋: `docs: 카탈로그 문서 자동 업데이트 (2026-04-04)` — 222개 파일 업데이트
- scheduled run(cron 토요일 17:00 UTC)도 동일 시점에 자동 트리거 — 정상 작동 확인

### 다음 작업

- bundle-gateway.zstd를 기본 번들로 교체 (v3 schema로 재빌드)
- gen_catalog_docs에서 PartialStub을 Available 섹션에 포함 (현재 "기타" 분류)
- E2E 테스트: 실제 PartialStub API에서 available operation 호출 확인
