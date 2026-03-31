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
- 번들 전환 설계 스펙 작성: `docs/superpowers/specs/2026-03-31-bundle-transition-design.md`
- 구현 플랜 작성 (11 Task, TDD 기반): `docs/plans/2026-03-31-bundle-transition.md`
- docs 경로 통일: spec → `docs/superpowers/specs/`, plan → `docs/plans/`

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
- CatalogEntry에 endpoint_url 필드 추가 + 번들 빌더 반영
- skeleton spec 필터링 로직 추가
- HTML 테이블 파싱 PoC (기상청 단기예보 대상)
