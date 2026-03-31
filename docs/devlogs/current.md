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
- Claude Desktop / Cursor MCP 연동 테스트
- 번들 카탈로그 경량화 방안 설계
- Phase 2: apis.data.go.kr 호스팅 API 지원 (DataGoKrRest 프로토콜)
