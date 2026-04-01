# korea-cli

> 한국 공공데이터포털(data.go.kr)의 수천 개 API를 자연어로 접근하는 CLI + MCP 서버

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)

## 왜 만들었나

[공공데이터포털](https://www.data.go.kr)에는 수천 개의 무료 오픈 API가 있습니다. 미세먼지, 날씨, 부동산, 교통, 위생등급... 유용한 데이터가 이미 공개되어 있지만, 실제로 활용하는 사람은 많지 않습니다.

- API마다 파라미터명, 응답 구조, 인코딩이 전부 다릅니다
- 하나 연동하려면 문서를 읽고 신청하고 테스트하는 데 시간이 걸립니다
- 개발자가 아니면 접근 자체가 어렵습니다

**korea-cli는 이 문제를 LLM으로 해결합니다.**

공공데이터포털의 전체 API 카탈로그를 자동으로 수집하고 추상화하여, 자연어 한 줄로 원하는 데이터에 접근할 수 있게 합니다.

## 사용 예시

### CLI로 직접 사용

```bash
# API 검색 (12,000+ API 번들 내장, 즉시 사용 가능)
$ korea-cli search "사업자등록"
# → 오프라인 카탈로그에서 즉시 검색

# API 상세 스펙 조회
$ korea-cli spec 15081808
# → 파라미터, 인증 방식, 엔드포인트 확인

# API 직접 호출
$ korea-cli call 15081808 /status --param 'b_no=["1234567890"]'
# → JSON 응답 반환
```

### MCP 서버로 AI 도구에 연결

Claude Desktop, Cursor 등에서 MCP 서버로 연결하면 AI가 한국 공공데이터를 직접 활용합니다:

```json
{
  "mcpServers": {
    "korea": {
      "command": "korea-cli",
      "args": ["mcp"]
    }
  }
}
```

## 특징

| 특징 | 설명 |
|------|------|
| **번들 내장** | 12,000+ API 카탈로그 + Swagger 스펙이 바이너리에 내장. 설치 즉시 오프라인 검색/스펙 조회 가능 |
| **자연어 접근** | 상황을 설명하면 적절한 API를 찾아 호출하고 결과를 정리 |
| **MCP 서버** | Claude Desktop, Cursor 등 AI 도구에서 tool로 바로 사용 |
| **활용신청 안내** | 필요한 API의 신청 절차와 URL까지 안내 |
| **단일 바이너리** | Rust로 빌드. Node.js, Python 등 런타임 설치 불필요 |

## 설치

### GitHub Releases

[Releases](https://github.com/JunsikChoi/korea-cli/releases)에서 OS에 맞는 바이너리를 다운로드하세요.

```bash
# macOS (Apple Silicon)
curl -LO https://github.com/JunsikChoi/korea-cli/releases/latest/download/korea-cli-aarch64-apple-darwin.tar.gz

# Linux (x86_64)
curl -LO https://github.com/JunsikChoi/korea-cli/releases/latest/download/korea-cli-x86_64-unknown-linux-gnu.tar.gz
```

### Cargo로 설치

```bash
cargo install korea-cli
```

## 시작하기

### 1. 공공데이터포털 API 키 발급

1. [data.go.kr](https://www.data.go.kr) 회원가입 (무료)
2. 사용하려는 API의 활용신청 (대부분 자동승인)
3. 마이페이지에서 인증키 확인

> korea-cli가 어떤 API를 신청해야 하는지 안내해드립니다. 먼저 설치하고 질문해보세요.

### 2. API 키 설정

```bash
korea-cli config set api-key YOUR_API_KEY
```

### 3. 사용

```bash
# API 검색 (번들 내장, 바로 사용 가능)
korea-cli search "날씨"
korea-cli search "공항" --category "교통"

# API 스펙 확인
korea-cli spec <list_id>

# API 호출
korea-cli call <list_id> <operation> --param key=value

# 최신 번들로 업데이트 (선택)
korea-cli update

# MCP 서버로 AI 도구 연동
korea-cli mcp
```

## 작동 방식

```
사용자 (자연어)
    │
    ├── CLI: korea-cli "서울 미세먼지"
    └── MCP: AI 도구가 tool로 호출
            │
            ▼
    ┌──────────────────────────┐
    │      korea-cli (Rust)     │
    │                           │
    │  ┌─────────────────────┐  │
    │  │ API 카탈로그 + 검색  │  │ ← 메타 API로 자동 수집
    │  └─────────────────────┘  │
    │  ┌─────────────────────┐  │
    │  │ API 호출 엔진       │  │ ← 파라미터 매핑 + 호출 + 정규화
    │  └─────────────────────┘  │
    │  ┌─────────────────────┐  │
    │  │ MCP 서버 (JSON-RPC) │  │ ← Claude, Cursor 등 연동
    │  └─────────────────────┘  │
    └──────────────────────────┘
            │
            ▼
    공공데이터포털 API (data.go.kr)
```

## 프로젝트 구조

```
korea-cli/
├── src/
│   ├── main.rs          # CLI 엔트리포인트 + 서브커맨드
│   ├── core/
│   │   ├── types.rs     # 타입 (Bundle, CatalogEntry, ApiSpec 등)
│   │   ├── bundle.rs    # 번들 로드/해제, 오버라이드 체인
│   │   ├── catalog.rs   # 카탈로그 검색, 메타 API 수집
│   │   ├── swagger.rs   # Swagger 파싱 (parse_swagger, extract_swagger_json)
│   │   └── caller.rs    # API 호출 엔진
│   ├── mcp/             # MCP 서버 (stdio JSON-RPC)
│   ├── cli/             # CLI 서브커맨드 핸들러
│   ├── config/          # 설정 관리 (API 키, 환경변수)
│   └── bin/
│       └── build_bundle.rs  # 번들 생성 도구 (릴리스용)
├── build.rs             # 개발용 placeholder 번들 자동 생성
├── tests/               # 통합 테스트
├── docs/
│   ├── roadmap/         # 장기 로드맵 (Phase 1~3)
│   ├── devlogs/         # 개발 로그
│   └── superpowers/     # 설계 스펙 & 구현 플랜
│       ├── specs/       # 브레인스토밍 결과 (설계 문서)
│       └── plans/       # 구현 계획 (태스크별 체크박스)
├── website/             # koreacli.com 소스
├── Cargo.toml
└── LICENSE              # MIT
```

## 문서

| 문서 | 용도 |
|------|------|
| [Phase 1 로드맵](docs/roadmap/phase1-mvp.md) | 장기 마일스톤 체크리스트 (Phase 1~3) |
| [MVP 설계 스펙](docs/specs/2026-03-31-phase1-mvp-design.md) | 아키텍처, 데이터 모델, MCP 도구 설계 |
| [MVP 구현 플랜](docs/plans/2026-03-31-phase1-mvp.md) | 태스크별 상세 구현 단계 (TDD 기반) |

## 로드맵

[Phase 1: MVP](docs/roadmap/phase1-mvp.md)를 참고하세요.

## 기여

기여를 환영합니다! [이슈](https://github.com/JunsikChoi/korea-cli/issues)를 확인하거나 PR을 보내주세요.

## 라이선스

[MIT](LICENSE)
