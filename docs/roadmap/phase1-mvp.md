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
- [x] Gateway AJAX 번들 리빌드 — 12,119 APIs + 7,160 specs, 4.2 MB (Available 59.1%)
- [x] Spec 미수집 API 분석 → 유효 spec ~3,960 / skeleton ~1,400 / 미생성 ~1,200 / 외부 ~5,500

### 1. Spec 품질 개선 ✅
- [x] CatalogEntry에 `endpoint_url` 필드 추가 + 번들 빌더 반영
- [x] Skeleton spec 필터링 — `operations.is_empty()` spec 제거 + `SpecStatus` enum 태깅
- [x] CatalogEntry에 `spec_status: SpecStatus` 필드 추가 (Available/Skeleton/HtmlOnly/External/CatalogOnly/Unsupported)
- [x] HTML 테이블 파싱 모듈 (`scraper` 크레이트) — 번들 빌더 연결은 후속
- [x] 번들 스키마 v2 (schema_version) + 구 번들 graceful fallback
- [x] CLI/MCP spec_status 기반 안내 응답

### 2. HTML 폴백으로 스펙 커버리지 확장 (32.6% → 53.5%)
- [x] html_parser.rs 셀렉터 버그 수정 (`name=` → `id=` 우선)
- [x] HTML 전수조사 바이너리 (`html-survey`) 구현 + 실행
- [x] 전수조사 결과 분석: 2,522개 신규 추출 가능 확인
- [x] openapi.do 페이지 HTML 크롤러 (`crawl-pages`) — 수집/분석 분리 전략
- [x] 로컬 HTML 분석 도구 구현 (발견 기반 — analyze-pages + summarize-signals)
- [x] build_bundle.rs에 HTML 폴백 경로 추가 (분류 우선 파이프라인 — tyDetailCode → Swagger → Gateway AJAX)
  - openapi.do에서 pk + select 옵션 + tyDetailCode + publicDataPk 추출
  - 각 operation마다 `selectApiDetailFunction.do` AJAX POST (cookie-isolated client)
  - `parse_operation_detail` → `build_api_spec`으로 스펙 구성
- [x] html_parser.rs 보강 — h4 기반 응답 필드 파싱 + 요청주소 미발견 시 서비스URL 폴백
- [x] 번들 리빌드 + 커버리지 검증 — Available 7,160개 (59.1%), 목표 53.5% 초과 달성
- [ ] AJAX 부분 성공 662건 추가 파싱 (서비스URL 기반)

### 3. CI 수집 파이프라인 ✅
- [x] GitHub Actions 크론으로 Swagger + HTML AJAX 전체 수집 (`.github/workflows/bundle-ci.yml`)
- [x] 변경 감지 + 새 번들 생성 → GitHub Releases 배포 (sha256sum 비교, `bundle-{date}-{run}` 태그)
- [x] `korea-cli update`가 Releases에서 최신 번들 다운로드 (schema_version 검증 + atomic 교체)
- [x] PartialStub 분류 — 부분 성공 Gateway API 명시 분류 + failed_ops.json 출력
- [x] `--retry-stubs` 재수집 플래그 — failed_ops.json 기반 list_id 단위 retry + operation merge

### 4. 호출 엔진 개선
- [x] XML 응답 파싱 지원 (quick-xml Reader 이벤트 기반 custom parser, $text/CDATA/self-closing 처리)
- [ ] 인증 처리 일반화 — `Infuser ` 접두사 하드코딩 제거, Both+Header 경로 버그 수정
- [ ] 사용자 입력 정규화 — 사업자번호 하이픈 등 포맷 자동 변환 (spec 기반 힌트)

### 5. PartialStub 마무리 + 번들 배포 파이프라인
설계: `docs/specs/2026-04-05-partial-stub-finalization-design.md`

**번들 인프라 정비:**
- [x] 번들 v3 스키마 교체 (`data/bundle.zstd` 최신화)
- [x] orphan `bundle-gateway.zstd` 정리
- [x] `Makefile update-bundle` 헬퍼 추가 (Release → 로컬 번들 동기화)

**번들 배포 파이프라인 (Option A': 임베드 유지 + DX/CI 개선):**
- [ ] `build.rs` 3단계 fallback (로컬 → `BUNDLE_DOWNLOAD_URL` env → placeholder)
- [ ] 바이너리 릴리즈 CI (`.github/workflows/release.yml`) — 크로스 빌드 4 플랫폼
- [ ] crates.io publish 파이프라인 — `Cargo.toml include` + `scripts/publish.sh`

**문서 분류 개선 (schema v4):**
- [x] `ApiSpec.missing_operations: Vec<String>` 필드 추가 (schema v3 → v4)
- [x] `build_bundle.rs`: PartialStub 시 `FailedOp.op_name` → `missing_operations` 수집
- [x] `merge_operations` retry 복구 op 동기화 (정확 일치 매칭)
- [x] `gen_catalog_docs.rs`: PartialStub을 "호출 가능" 섹션으로 이동 + 상태/누락 컬럼
- [x] v3 번들 graceful fallback 테스트
- [x] `verify-bundle` 바이너리 + release.yml schema gate 추가
- [x] `bundle.rs` embedded bundle 사용자 친화적 panic 메시지
- [x] CI 재빌드 트리거로 v4 번들 Release 생성 (Task 9, 별도 세션)

**E2E 스모크 테스트:**
- [x] `tests/integration/e2e_gateway_smoke.rs` (Gateway AJAX Available 5개)
- [ ] 수동 실행: `cargo test --test e2e_gateway_smoke -- --ignored --nocapture` (Task 11)
- [x] 대상 5개 API 이용신청 완료: 15059468, 15012690, 15073855, 15000415, 15134735

**PartialStub 실제 발생률 조사 결론 (2026-04-05):**
- 4/4 CI 실행 결과: PartialStub **0건** (Gateway AJAX all-or-nothing 패턴)
- `fetch_gateway_spec` 감지 로직 자체는 버그 없음
- feature는 timeout/간헐 오류 방어망으로 유지 (3개월 후 재평가)

---

## Phase 2: 호출 안정화 + 배포 (예정)
- XML 응답 처리
- 페이지네이션 자동화 (numOfRows/pageNo)
- Claude Desktop / Cursor MCP 연동 테스트 + 문서화
- `cargo install korea-cli` + GitHub Releases 바이너리 배포
- koreacli.com 랜딩 페이지

## Phase 3: 확장 (예정)
- 외부 기관 호스팅 API 지원 (ExternalRest) — 주요 81개 도메인 중 인기 순 선별
- SOAP/WMS/WFS 프로토콜 지원
- 크로스 플랫폼 바이너리 빌드 (macOS/Linux/Windows)

## 커버리지 한계 참고 (2026-04-02 전수조사 기준)

현재 기술적으로 추출 불가능한 API:
- **operation 미등록 (4,899건)**: pk는 있지만 select 옵션 없음 — 기관이 데이터 입력해야 함
- **Skeleton (1,405건)**: Swagger 파일 있지만 paths 비어있음 + select 옵션도 없음
- **폐기/서비스 종료 (282건)**: 비활성 API
- **접근 불가 (10건)**: JS 리다이렉트 또는 네트워크 에러
