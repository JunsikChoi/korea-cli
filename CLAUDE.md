# korea-cli

한국 공공데이터포털(data.go.kr) API를 자연어로 접근하는 CLI + MCP 서버.

## 기술 스택

- **언어**: Rust (edition 2021)
- **주요 크레이트**: clap (CLI), tokio (async), reqwest (HTTP), serde (직렬화), postcard (바이너리 직렬화), zstd (압축), scraper (HTML 파싱), quick-xml (XML 응답 파싱)
- **배포**: 단일 바이너리 (cargo install, GitHub Releases) — 12K+ API 번들 내장

## 프로젝트 구조

```
src/
├── main.rs        # CLI 엔트리포인트, clap 서브커맨드 정의
├── core/
│   ├── types.rs       # 타입 (Bundle, CatalogEntry, ApiSpec, SpecStatus, ClassificationHints 등)
│   ├── bundle.rs      # 번들 로드/해제, 오버라이드 체인, 스키마 버전 관리
│   ├── catalog.rs     # 카탈로그 검색, 메타 API 수집
│   ├── swagger.rs     # Swagger 파싱 (parse_swagger, extract_swagger_json)
│   ├── html_parser.rs # HTML 테이블 파서 (data.go.kr Gateway API)
│   └── caller.rs      # API 호출 엔진
├── mcp/           # MCP 서버 (stdio JSON-RPC)
├── config/        # 설정 관리 (~/.config/korea-cli/config.toml)
├── cli/           # CLI 서브커맨드 핸들러
└── bin/
    ├── build_bundle.rs  # 번들 생성 도구 (Swagger + Gateway AJAX 추출 + --retry-stubs)
    ├── verify_bundle.rs # 번들 schema_version 검증 (release CI gate, BundleMetadata peek)
    ├── survey.rs        # API 커버리지 서베이
    ├── html_survey.rs   # HTML 구조 서베이
    ├── crawl_pages.rs   # openapi.do 페이지 크롤러
    ├── analyze_pages.rs # HTML 구조 신호 추출기
    ├── summarize_signals.rs # 신호 빈도 분석 + 클러스터링
    └── gen_catalog_docs.rs  # API 카탈로그 markdown 문서 생성
.github/
└── workflows/
    ├── bundle-ci.yml    # 주 1회 번들 수집 + retry + Release 배포
    └── release.yml      # 바이너리 릴리즈 CI (4 플랫폼 크로스 빌드)
scripts/
└── publish.sh           # crates.io 배포 (번들 다운로드 → cargo publish)
build.rs                 # 번들 해결 3단계 (로컬 → BUNDLE_DOWNLOAD_URL → placeholder)
```

## 코딩 컨벤션

- `cargo clippy`와 `cargo fmt` 통과 필수
- 에러 처리: `anyhow::Result` (애플리케이션), `thiserror` (라이브러리 경계)
- 비동기: `tokio` 런타임, `reqwest`로 HTTP 호출
- 테스트: `#[cfg(test)] mod tests` 인라인 + `tests/` 통합 테스트

## 핵심 데이터 소스

- **메타 API**: `api.odcloud.kr/api/15077093/v1/open-data-list` (전체 API 목록)
- **Swagger 스펙**: 각 API 상세 페이지에서 OpenAPI 스펙 제공
- **CSV 목록**: `data.go.kr/assets/csvs/API개방리스트.csv`

## 커밋 컨벤션

```
feat: 새 기능
fix: 버그 수정
docs: 문서 변경
refactor: 리팩토링
test: 테스트 추가/수정
chore: 빌드, 설정 변경
```

## data/ 디렉토리 — 생성 파일 주의

`data/` 하위에는 바이너리 실행으로 생성되는 대용량 파일이 있다 (.gitignore 대상):

- `data/pages/` — 12K+ 크롤링 HTML (2GB)
- `data/page_raw_signals.json` — analyze-pages 출력 (250MB)
- `data/signal_summary.json` — summarize-signals 출력
- `data/survey.json`, `data/html-survey.json` 등

**worktree 정리 전 필수 확인**: worktree에서 생성한 `data/` 파일은 git에 추적되지 않으므로, `git worktree remove` 시 함께 삭제된다. worktree 정리 전에 반드시:
1. 메인 worktree에 없는 생성 파일이 있는지 확인 (`ls data/*.json`)
2. 있으면 메인 worktree로 복사 (`cp`)하거나 메인에서 재실행
3. symlink로 연결한 파일(`data/pages/`)은 원본이 메인에 있으므로 안전

## 빌드 & 테스트

```bash
cargo check          # 타입 체크
cargo test           # 테스트 실행
cargo clippy         # 린트
cargo fmt -- --check # 포매팅 확인
cargo run -- --help  # 실행
```
