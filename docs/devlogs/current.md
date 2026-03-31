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
- PoC: 메타 API로 카탈로그 수집 파이프라인 구현
- API 스펙 자동 파싱 검증
