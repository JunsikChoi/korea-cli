# data.go.kr openapi.do 페이지 크롤러 설계

**날짜**: 2026-04-02
**목적**: 12K+ API의 openapi.do 페이지 HTML 원본을 로컬에 저장. 수집과 분석을 분리하여, 분석 로직 변경 시 재크롤링 불필요.

## 배경

두 차례 전수조사(survey.rs, html_survey.rs)가 확증 편향으로 실패. "예상 패턴이 있는지 확인"이 아닌 "페이지에 실제로 뭐가 있는지 발견"하려면 원본 HTML이 필요.

## 결정 사항

| 항목 | 결정 |
|------|------|
| 위치 | `src/bin/crawl_pages.rs` (별도 바이너리) |
| list_id 소스 | 메타 API (`catalog.rs` 재사용) |
| 출력 | `data/pages/{list_id}.html` |
| 실패 로그 | `data/crawl_failures.json` |
| 기본값 | concurrency=5, delay=100ms |
| resume | `--resume` 플래그, 기존 파일 스킵 |
| 진행 표시 | 매 건 한 줄 로그 |
| 새 의존성 | 없음 |

## CLI 인터페이스

```bash
cargo run --bin crawl_pages -- \
  --concurrency 5 \
  --delay 100 \
  --resume
```

## 실행 흐름

1. 메타 API로 list_id 전체 수집 (~12K)
2. `--resume`이면 `data/pages/` 스캔 → 이미 있는 list_id 제외
3. `stream::iter` + `buffer_unordered(concurrency)`로 병렬 다운로드
   - `GET https://www.data.go.kr/data/{list_id}/openapi.do`
   - 200 OK → `data/pages/{list_id}.html`로 저장
   - 실패 → 메모리에 수집
4. 매 건 진행 로그 출력
5. 완료 후 실패 목록 `data/crawl_failures.json`으로 저장
6. 최종 요약 출력 (총/성공/실패/스킵)

## 핵심 로직

**HTTP 클라이언트:**
```rust
reqwest::Client::builder()
    .user_agent("korea-cli-crawler/0.1.0")
    .timeout(Duration::from_secs(15))
    .build()
```

**단일 페이지 다운로드:**
```rust
async fn download_page(client: &Client, list_id: &str, out_dir: &Path)
    -> Result<usize, CrawlError>
```
- URL 구성 → GET → 상태 체크 → `tokio::fs::write` → 바이트 수 반환

**실패 엔트리:**
```rust
struct CrawlFailure {
    list_id: String,
    status: Option<u16>,
    error: String,
}
```

**resume 로직:**
- `data/pages/` 디렉토리 스캔, 파일명(확장자 제거)을 `HashSet`으로 구성
- 전체 list_id에서 기존 파일 제외

**카운터:** `AtomicUsize` (done, failed)

## 진행 로그 형식

```
[1/12080] 15001234.html (45KB) — 1 done, 0 failed, 12079 remaining
[3/12080] FAIL 15009999 (404 Not Found) — 2 done, 1 failed, 12077 remaining
```

Resume 시:
```
Resume mode: 3,421 files already exist, 8,659 remaining
```

최종 요약:
```
Crawl complete in 26m 43s
  Total:   12,080
  Skipped: 3,421 (resume)
  Success: 8,512
  Failed:  147
  Failures saved to data/crawl_failures.json
```
