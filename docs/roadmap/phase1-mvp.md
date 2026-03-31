# Phase 1: MVP

## 목표
공공데이터포털의 Infuser 호스팅 오픈 API를 검색하고 호출할 수 있는 CLI + MCP 서버.
AI 에이전트(Codex, Claude Code, Claude Desktop, Cursor)가 주 사용자.

## 마일스톤

### 1. 프로젝트 기반
- [ ] 데이터 타입 정의 (ApiService, ApiSpec, ApiProtocol 등)
- [ ] 설정 관리 (config.toml + 환경변수 `DATA_GO_KR_API_KEY`)
- [ ] 모듈 구조 재편 (core/, cli/, mcp/, config/)

### 2. API 카탈로그
- [ ] 메타 API로 전체 오픈 API 목록 수집 (페이지네이션)
- [ ] 오퍼레이션→서비스(list_id) 그룹핑
- [ ] 로컬 catalog.json 저장/로드
- [ ] 텍스트 검색 (title, description, keywords, org 매칭)
- [ ] `korea-cli update` / `korea-cli search` 명령

### 3. Swagger 스펙 파싱
- [ ] data.go.kr 상세페이지 스크래핑 (swaggerUrl 추출)
- [ ] Swagger 2.0 JSON → 정규화된 ApiSpec 변환
- [ ] 파라미터 타입/필수여부/HTTP 메서드 추출
- [ ] 로컬 캐시 (cache/specs/{list_id}.json)
- [ ] `korea-cli spec` 명령

### 4. API 호출 엔진
- [ ] ApiSpec 기반 HTTP 요청 빌드 (GET query / POST JSON body)
- [ ] 인증 주입 (serviceKey query param / Infuser header)
- [ ] 응답 추출 (data_path 기반)
- [ ] 구조화된 에러 응답 (action 필드 포함)
- [ ] `korea-cli call` 명령

### 5. MCP 서버
- [ ] stdio JSON-RPC 2.0 프로토콜 (initialize, tools/list, tools/call)
- [ ] search_api 도구 (카탈로그 검색)
- [ ] get_api_spec 도구 (Swagger 파싱 → 스펙 반환)
- [ ] call_api 도구 (API 호출 → 정규화된 응답)
- [ ] Claude Desktop / Cursor 연동 테스트

### 6. 마무리
- [ ] 번들 카탈로그 (include_str! 빌드 시 포함)
- [ ] 통합 테스트 (search → spec → call E2E)
- [ ] README 업데이트
- [ ] clippy + fmt 통과

---

## Phase 2 (예정)
- apis.data.go.kr 호스팅 API 지원 (DataGoKrRest 프로토콜)
- XML 응답 처리
- 페이지네이션 자동화 (numOfRows/pageNo)

## Phase 3 (예정)
- 외부 기관 호스팅 API 지원 (ExternalRest)
- SOAP API 지원
- CI 카탈로그 자동 수집 + GitHub Releases 배포
- koreacli.com 랜딩 페이지
- 크로스 플랫폼 바이너리 빌드 (macOS/Linux/Windows)
