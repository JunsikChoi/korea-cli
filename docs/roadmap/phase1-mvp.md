# Phase 1: MVP

## 목표
공공데이터포털의 Infuser 호스팅 오픈 API를 검색하고 호출할 수 있는 CLI + MCP 서버.
AI 에이전트(Codex, Claude Code, Claude Desktop, Cursor)가 주 사용자.

## 마일스톤

### 1. 프로젝트 기반
- [x] 데이터 타입 정의 (ApiService, ApiSpec, ApiProtocol 등)
- [x] 설정 관리 (config.toml + 환경변수 `DATA_GO_KR_API_KEY`)
- [x] 모듈 구조 재편 (core/, cli/, mcp/, config/)

### 2. API 카탈로그
- [x] 메타 API로 전체 오픈 API 목록 수집 (페이지네이션)
- [x] 오퍼레이션→서비스(list_id) 그룹핑
- [x] 로컬 catalog.json 저장/로드
- [x] 텍스트 검색 (title, description, keywords, org 매칭)
- [x] `korea-cli update` / `korea-cli search` 명령

### 3. Swagger 스펙 파싱
- [x] data.go.kr 상세페이지 스크래핑 (swaggerUrl 추출)
- [x] Swagger 2.0 JSON → 정규화된 ApiSpec 변환
- [x] 파라미터 타입/필수여부/HTTP 메서드 추출
- [x] 로컬 캐시 (cache/specs/{list_id}.json)
- [x] `korea-cli spec` 명령

### 4. API 호출 엔진
- [x] ApiSpec 기반 HTTP 요청 빌드 (GET query / POST JSON body)
- [x] 인증 주입 (serviceKey query param / Infuser header)
- [x] 응답 추출 (data_path 기반)
- [x] 구조화된 에러 응답 (action 필드 포함)
- [x] `korea-cli call` 명령

### 5. MCP 서버
- [x] stdio JSON-RPC 2.0 프로토콜 (initialize, tools/list, tools/call)
- [x] search_api 도구 (카탈로그 검색)
- [x] get_api_spec 도구 (Swagger 파싱 → 스펙 반환)
- [x] call_api 도구 (API 호출 → 정규화된 응답)
- [ ] Claude Desktop / Cursor 연동 테스트

### 6. 마무리
- [ ] 번들 카탈로그 (include_bytes! + postcard + zstd, ~2-3MB) → Phase 1.1에서 구현
- [x] 통합 테스트 (search → spec → call E2E)
- [x] README 업데이트
- [x] clippy + fmt 통과

---

## Phase 1.1: 안정화 (예정)

### 0. 번들 전환 (스크래핑 → 사전 수집 데이터) ✅
- [x] 번들 데이터 구조 설계 (카탈로그 + spec 통합) → `docs/specs/2026-03-31-bundle-transition-design.md`
- [x] 번들 로드/조회 로직 구현 (기존 실시간 스크래핑 대체)
- [x] `korea-cli update` 번들 다운로드 뼈대
- [x] 초기 번들 생성 (수동 1회 수집) — 12,080 APIs + 5,363 specs, 2.77 MB
- [x] Spec 미수집 API 분석 → 유효 spec ~3,960 / skeleton ~1,400 / 미생성 ~1,200 / 외부 ~5,500

### 1. Spec 품질 개선
- [ ] CatalogEntry에 `endpoint_url` 필드 추가 + 번들 빌더 반영
  - 외부 링크 API: 원본 포탈 URL 안내 가능 (예: lofin365.go.kr)
  - Swagger 미생성 API: endpoint URL 보존 → 향후 HTML 파싱과 결합
- [ ] Skeleton spec 필터링 — `operations.is_empty()` spec 제거 또는 `spec_status` 태깅
- [ ] CatalogEntry에 `spec_status` 필드 추가 (has_spec / skeleton / html_only / external / none)
- [ ] HTML 테이블 파싱 PoC — `apis.data.go.kr` endpoint 있지만 Swagger 없는 ~1,200개 대상
  - 대상: 기상청 단기예보 (53,803건), 에어코리아 (51,347건) 등 인기 API
  - openapi.do 페이지의 오퍼레이션 테이블에서 파라미터/응답 추출

### 2. CI 수집 파이프라인
- [ ] GitHub Actions 크론으로 data.go.kr 전체 Swagger spec 수집
- [ ] 변경 감지 + 새 번들 생성 → GitHub Releases 배포
- [ ] `korea-cli update`가 Releases에서 최신 번들 다운로드

### 3. 호출 엔진 개선
- [ ] XML 응답 파싱 지원 (현재 JSON만 처리)
- [ ] 인증 처리 일반화 — `Infuser ` 접두사 하드코딩 제거, Both+Header 경로 버그 수정
- [ ] 사용자 입력 정규화 — 사업자번호 하이픈 등 포맷 자동 변환 (spec 기반 힌트)

---

## Phase 2 (예정)
- apis.data.go.kr Swagger 미생성 API 지원 (HTML 테이블 파싱 → spec 추출)
- XML 응답 처리
- 페이지네이션 자동화 (numOfRows/pageNo)

## Phase 3 (예정)
- 외부 기관 호스팅 API 지원 (ExternalRest) — 서울열린데이터, vworld 등
- SOAP/WMS/WFS 프로토콜 지원
- CI 카탈로그 자동 수집 + GitHub Releases 배포
- koreacli.com 랜딩 페이지
- 크로스 플랫폼 바이너리 빌드 (macOS/Linux/Windows)
