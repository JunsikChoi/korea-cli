# korea-cli Phase 1 MVP 설계

## Context

한국 공공데이터포털(data.go.kr)에는 17,000+ API 오퍼레이션이 등록되어 있지만, 이를 체계적으로 탐색하고 호출하는 도구가 없다. 기존 프로젝트들은 6개 이하의 API만 하드코딩한다. korea-cli는 **전체 API 카탈로그를 자동 수집**하고, **AI 에이전트가 자연어로 API를 검색·호출**할 수 있는 CLI + MCP 서버를 제공한다.

**핵심 사용자**: AI 에이전트 (Codex, Claude Code, Claude Desktop, Cursor)

- 개발자: CLI 바이너리 설치 → AI 도구(Codex/Claude Code)에서 활용
- 일반 AI 사용자: MCP 서버로 연결 → Claude Desktop/Cursor에서 자연어 질의

## 아키텍처

### 접근법: Catalog-First

바이너리에 경량 메타 카탈로그를 번들링하여 설치 즉시 검색 가능. Swagger 스펙은 사용자가 특정 API를 선택할 때 온디맨드 파싱 + 로컬 캐시.

```
┌─────────────────────────────────────────────────┐
│                korea-cli 바이너리                 │
│                                                   │
│  ┌─────┐  ┌──────┐  ┌──────┐  ┌──────────────┐  │
│  │ CLI │  │ MCP  │  │ Core │  │ Config Mgr   │  │
│  │     │──│Server│──│Engine│  │              │  │
│  │     │  │      │  │      │  │ env var →    │  │
│  └─────┘  └──────┘  └──┬───┘  │ config.toml  │  │
│                         │      └──────────────┘  │
└─────────────────────────┼────────────────────────┘
                          │
          ┌───────────────┼──────────────┐
          ▼               ▼              ▼
   번들 카탈로그    data.go.kr      개별 API 서버
   (include_str!)  메타API+상세페이지  (api.odcloud.kr)
                   (Swagger URL 추출)
```

CLI와 MCP가 동일 Core Engine을 공유하여 로직 중복 없음.

### 데이터 흐름

1. **검색 (search)** — 번들/로컬 카탈로그에서 텍스트 매칭 → 후보 API 목록 반환
2. **스펙 조회 (get_spec)** — 캐시 확인 → 없으면 상세페이지 스크래핑 → Swagger 파싱 → 정규화된 스펙 반환
3. **호출 (call)** — 캐시된 스펙에서 엔드포인트/인증 확인 → HTTP 호출 → 응답 정규화 (JSON)
4. **갱신 (update)** — 메타 API 전체 수집 (페이지네이션) → 로컬 카탈로그 갱신

## 데이터 모델

### 카탈로그 엔트리 (검색용 경량 데이터)

메타 API에서 수집. 서비스(list_id) 단위로 오퍼레이션을 그룹핑.

```rust
struct ApiService {
    list_id: String,              // "15081808"
    title: String,                // "국세청_사업자등록정보..."
    description: String,
    keywords: Vec<String>,        // ["사업자", "국세청"]
    org_name: String,             // "국세청"
    category: String,             // "공공행정"
    endpoint_url: String,         // "https://api.odcloud.kr/api/nts-businessman/v1"
    data_format: String,          // "JSON"
    auto_approve: bool,
    is_free: bool,
    request_count: u32,           // 인기도 지표
    updated_at: String,
    operations: Vec<OperationSummary>,
}

struct OperationSummary {
    id: String,                   // operation_seq
    name: String,                 // "상태조회"
    request_params: Vec<String>,  // 한글명
    request_params_en: Vec<String>, // 영문명
}
```

### API 스펙 (호출용 상세 데이터 — 확장 가능한 추상화)

Swagger 파싱 결과. MVP는 `InfuserRest`만 구현하되, 모든 API 유형을 수용하는 추상화 레이어.

```rust
enum ApiProtocol {
    InfuserRest,     // MVP: GET/POST, api.odcloud.kr (Swagger at infuser.odcloud.kr)
    DataGoKrRest,    // Phase 2: GET query params, apis.data.go.kr
    ExternalRest,    // Phase 3: 외부 기관 커스텀 REST
    Soap,            // Phase 3: SOAP XML envelope
}

struct ApiSpec {
    list_id: String,
    base_url: String,
    protocol: ApiProtocol,
    auth: AuthMethod,
    extractor: ResponseExtractor,
    operations: Vec<Operation>,
    fetched_at: String,
}

enum AuthMethod {
    QueryParam { name: String },
    Header { name: String, prefix: String },
    Both { query: String, header_name: String, header_prefix: String, prefer: AuthPreference }, // AuthPreference = Query | Header
    None,
}

struct ResponseExtractor {
    data_path: Vec<String>,        // ["data"] or ["response","body","items","item"]
    error_check: ErrorCheck,
    pagination: Option<PaginationStyle>,
    format: ResponseFormat,        // Json | Xml
}

enum ErrorCheck {
    HttpStatus,
    FieldEquals { path: Vec<String>, success_value: String, message_path: Vec<String> },
}

enum PaginationStyle {
    PagePerPage { page: String, per_page: String },
    NumOfRowsPageNo { rows: String, page_no: String },
    CursorBased { cursor_field: String },
    None,
}

struct Operation {
    path: String,                  // "/status"
    method: HttpMethod,            // GET | POST
    summary: String,
    content_type: ContentType,     // Json | Xml | FormUrlEncoded | None
    parameters: Vec<Parameter>,    // query/path/header params
    request_body: Option<RequestBody>,
    response_fields: Vec<ResponseField>,
}

struct Parameter {
    name: String,
    description: String,
    location: ParamLocation,       // Query | Path | Header | Body
    param_type: String,            // "string" | "integer" | "array"
    required: bool,
    default: Option<String>,
}
```

**확장 원칙**: 새 API 유형 지원 시 `ApiProtocol` variant 추가 + 해당 프로토콜 핸들러 구현. 검색/캐싱/MCP 도구는 변경 없이 재사용.

### 로컬 저장소

```
~/.config/korea-cli/
├── config.toml           # api_key, catalog_updated_at
├── catalog.json          # Vec<ApiService> (~2-5MB)
└── cache/specs/
    ├── 15081808.json     # ApiSpec 캐시 (서비스별)
    └── 15113968.json
```

### API 키 우선순위

1. 환경변수 `DATA_GO_KR_API_KEY` (MCP 표준 패턴)
2. `config.toml`의 `api_key` 필드 (CLI 사용자)
3. 미설정 시: 검색 가능, 호출 시 에러 + 설정 안내

MCP 서버의 경우 Claude Desktop `claude_desktop_config.json`에서 환경변수로 전달:

```json
{
  "mcpServers": {
    "korea-cli": {
      "command": "korea-cli",
      "args": ["mcp"],
      "env": { "DATA_GO_KR_API_KEY": "사용자_서비스키" }
    }
  }
}
```

## MCP 도구 & CLI 인터페이스

### MCP 도구 (3단계)

**1. search_api** — 카탈로그 검색

```
INPUT:  { query: "사업자등록 조회", category?: "공공행정", limit?: 10 }
OUTPUT: { results: [{ list_id, title, description, org, operations, auto_approve, popularity }], total }
```

**2. get_api_spec** — 상세 스펙 조회 (Swagger 파싱 → 캐시)

```
INPUT:  { list_id: "15081808" }
OUTPUT: { list_id, base_url, auth, has_api_key, operations: [{ path, method, summary, parameters, response_fields }], key_guide? }
```

**3. call_api** — API 호출

```
INPUT:  { list_id: "15081808", operation: "/status", params: { b_no: ["1234567890"] } }
OUTPUT: { success, data, raw_status, metadata }
```

### CLI 서브커맨드 매핑

```bash
korea-cli search "사업자등록"              # → search_api
korea-cli spec 15081808                    # → get_api_spec
korea-cli call 15081808 /status \          # → call_api
  --param b_no='["1234567890"]'
korea-cli update                           # 카탈로그 갱신
korea-cli config set api-key KEY           # 키 저장
korea-cli mcp                              # MCP 서버 시작 (stdio)
```

### 구조화된 에러 응답

모든 에러에 `action` 필드 포함 → AI가 자동 복구 가능.

```json
{
  "success": false,
  "error": "NO_API_KEY",
  "message": "API 키가 설정되지 않았습니다.",
  "action": "korea-cli config set api-key KEY 또는 환경변수 DATA_GO_KR_API_KEY 설정"
}
```

## 모듈 구조

```
src/
├── main.rs              # CLI 엔트리포인트 (clap 서브커맨드)
├── core/                # 핵심 비즈니스 로직 (CLI·MCP 공유)
│   ├── mod.rs
│   ├── types.rs         # 모든 데이터 타입
│   ├── catalog.rs       # 카탈로그 로드/검색/갱신
│   ├── swagger.rs       # Swagger 파싱 + 정규화
│   └── caller.rs        # API 호출 엔진
├── mcp/                 # MCP 서버 (stdio JSON-RPC)
│   ├── mod.rs
│   ├── server.rs        # JSON-RPC 프로토콜
│   └── tools.rs         # search_api, get_api_spec, call_api
├── config/              # 설정 관리
│   ├── mod.rs
│   └── keys.rs          # API 키 저장/조회 (env → config fallback)
└── cli/                 # CLI 서브커맨드 핸들러
    ├── mod.rs
    ├── search.rs
    ├── call.rs
    └── update.rs
```

## 구현 단계 (7 Step)

### Step 0: 테스트 API 키 준비

개발 중 로컬 테스트에 사용할 6개 API (모두 Infuser 호스팅, Swagger 완비):

| #   | API                           | pk       | 테스트 포인트                   |
| --- | ----------------------------- | -------- | ------------------------------- |
| 1   | 국세청\_사업자등록정보        | 15081808 | POST + JSON body, 복잡한 스키마 |
| 2   | 공공데이터\_목록조회서비스    | 15077093 | GET + 조건검색, 카탈로그 수집용 |
| 3   | 행안부\_공공서비스(혜택) 정보 | 15113968 | GET + 16개 파라미터 (복잡)      |
| 4   | 한국부동산원\_공동주택 단지   | 15106817 | GET + 3종 오퍼레이션            |
| 5   | 한국공항공사\_공항 소요시간   | 15095478 | GET + 심플 단일 엔드포인트      |
| 6   | (주)에스알\_SRT 회원현황      | 15125565 | GET + 통계 3종                  |

Swagger URL 패턴:

- Named: `https://infuser.odcloud.kr/api/stages/{stageId}/api-docs`
- Stage IDs: #1=28493, #2=26635, #3=44436, #4=41233, #5=32210, #6=50672

### Step 1: 프로젝트 기반 + 데이터 타입

- Cargo.toml edition 수정 (2024→2021)
- `core/types.rs` — 모든 데이터 타입 정의
- `config/` — config.toml 읽기/쓰기, API 키 관리 (환경변수 우선)
- 디렉토리 구조 재편 (catalog/ → core/, cli/ 추가)

### Step 2: 카탈로그 수집 + 검색

- `core/catalog.rs` — 메타 API 전체 수집 (페이지네이션), 오퍼레이션→서비스 그룹핑
- 로컬 catalog.json 저장/로드
- 텍스트 검색 (title, description, keywords, org 매칭)
- `cli/search.rs` + `cli/update.rs`
- 검증: `korea-cli update` → `korea-cli search "사업자"` (API #2 사용)

### Step 3: Swagger 파싱 + 스펙 캐시

- `core/swagger.rs` — 상세페이지 스크래핑 (swaggerUrl 추출), Swagger JSON 파싱 → ApiSpec 변환
- 로컬 캐시 (`cache/specs/{list_id}.json`)
- `cli/spec.rs`
- 검증: `korea-cli spec 15081808` → 파라미터/응답 스키마 출력 (API #1~6 전부)

### Step 4: API 호출 엔진

- `core/caller.rs` — ApiSpec 기반 HTTP 요청 빌드, 인증 주입, 응답 추출
- `InfuserRest` 프로토콜 핸들러 구현
- 구조화된 에러 응답 (action 필드 포함)
- `cli/call.rs`
- 검증: POST(#1) + GET 심플(#5) + GET 복잡(#3) 호출 테스트

### Step 5: MCP 서버

- `mcp/server.rs` — stdio JSON-RPC 2.0 (initialize, tools/list, tools/call)
- `mcp/tools.rs` — search_api, get_api_spec, call_api 도구 등록
- Core 함수 직접 호출 (CLI와 동일 코드패스)
- 검증: Claude Desktop에서 자연어 질의 → API 호출 → 응답

### Step 6: 카탈로그 번들링 + 마무리

- 빌드 시 catalog.json을 `include_str!`로 번들
- 첫 실행 시 번들 카탈로그 → 로컬 파일 추출
- README 업데이트 (설치, 사용법, MCP 설정)
- 통합 테스트 (search → spec → call E2E)

## 검증 시나리오 (E2E)

```bash
# 1. 키 설정
$ korea-cli config set api-key YOUR_KEY

# 2. 카탈로그 갱신
$ korea-cli update
# ✓ 17,174 오퍼레이션 수집 → ~3,000 서비스로 그룹핑

# 3. 검색
$ korea-cli search "사업자등록"
# [1] 국세청_사업자등록정보 (list_id: 15081808, 자동승인, ops: 진위확인, 상태조회)

# 4. 스펙 확인
$ korea-cli spec 15081808
# POST /status — 필수: b_no (array<string>)

# 5. 호출
$ korea-cli call 15081808 /status --param b_no='["1234567890"]'
# { "success": true, "data": [{ "b_no": "1234567890", ... }] }

# 6. MCP (Claude Desktop)
# 사용자: "1234567890 사업자번호가 유효한지 확인해줘"
# AI → search_api → get_api_spec → call_api → 자연어 응답
```

## 공공데이터포털 API 다양성 (참고)

| 유형               | 호스팅          | Swagger | MVP      |
| ------------------ | --------------- | ------- | -------- |
| A: Infuser         | api.odcloud.kr  | 완비    | **지원** |
| B: apis.data.go.kr | apis.data.go.kr | 일부    | Phase 2  |
| C: 외부 기관       | 각자 도메인     | 없음    | Phase 3  |
| D: SOAP            | 다양            | WSDL만  | Phase 3  |

`ApiProtocol` enum + `ResponseExtractor` 추상화로, 새 유형 추가 시 프로토콜 핸들러만 구현하면 됨.

## 메타 API 참고 정보

- 엔드포인트: `api.odcloud.kr/api/15077093/v1/open-data-list`
- 인증: Header `Authorization: Infuser {serviceKey}` 또는 Query `serviceKey`
- 페이지네이션: `page`, `perPage` (최대 1000)
- 일일 호출 제한: 1,000회 (개발계정)
- totalCount: 17,174 오퍼레이션
- 갱신 주기: 실시간
- Swagger URL 추출: 상세페이지 HTML에서 `var swaggerUrl = '...'` 파싱
