# korea-cli

한국 공공데이터포털(data.go.kr) API를 자연어로 접근하는 CLI + MCP 서버.

## 기술 스택

- **언어**: Rust (edition 2024)
- **주요 크레이트**: clap (CLI), tokio (async), reqwest (HTTP), serde (직렬화)
- **배포**: 단일 바이너리 (cargo install, GitHub Releases)

## 프로젝트 구조

```
src/
├── main.rs        # CLI 엔트리포인트, clap 서브커맨드 정의
├── catalog/       # API 카탈로그 수집/인덱싱/검색
├── api/           # API 호출 엔진 (파라미터 매핑, 응답 정규화)
├── mcp/           # MCP 서버 (stdio JSON-RPC)
└── config/        # 설정 관리 (~/.config/korea-cli/config.toml)
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

## 빌드 & 테스트

```bash
cargo check          # 타입 체크
cargo test           # 테스트 실행
cargo clippy         # 린트
cargo fmt -- --check # 포매팅 확인
cargo run -- --help  # 실행
```
