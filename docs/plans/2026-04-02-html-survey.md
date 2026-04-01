# HTML 스펙 추출 전수조사 (survey2) 구현 플랜

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 셀렉터 버그를 수정하고, 12K+ API에 대해 HTML 스펙 경로(pk/select/AJAX)의 실제 작동 현황을 전수조사하여, 모든 API의 스펙을 추출할 수 있는 범용 솔루션의 근거 데이터를 확보한다.

**Architecture:** 기존 `html_parser.rs`의 셀렉터 버그(`name=` → `id=`)를 수정하고, 새 바이너리 `src/bin/html_survey.rs`를 작성한다. 1차로 openapi.do 페이지에서 pk/select 옵션을 추출하고, pk가 있는 API에 대해 첫 번째 operation의 AJAX(`selectApiDetailFunction.do`)를 실제 호출하여 응답 구조 패턴을 분류한다. 기존 survey.json은 보존하고 별도 `data/html-survey.json`으로 출력한다.

**Tech Stack:** Rust, tokio, reqwest, scraper, serde_json, futures

---

## 배경 — 1차 전수조사에서 발견된 문제

| 현상 | 원인 |
|------|------|
| publicDataDetailPk 12,108건 전부 미탐지 | `html_parser.rs:159`가 `name=`을 찾지만 실제 HTML은 `id=` |
| login_required 99.8% 오탐 | 공통 레이아웃의 "login"+"session" 문자열 오탐 |
| HTML 스펙 폴백 전혀 미작동 | pk 미탐지 → AJAX 호출 자체가 불가 |
| selectApiDetailFunction.do AJAX 미구현 | build_bundle.rs에 HTML 경로 자체가 없음 |

### 수동 검증 결과 (2026-04-02)

- `curl`로 openapi.do 접근: **로그인 벽 없음**, HTTP 200, 완전한 HTML
- `id="publicDataDetailPk"` + `value="uddi:..."`: **존재함**
- `selectApiDetailFunction.do` AJAX: **Referer 헤더만 추가하면 비로그인으로 작동**
- AJAX 응답: 요청주소, 서비스URL, 파라미터 테이블 포함 (16KB)

---

## Task 1: html_parser.rs 셀렉터 버그 수정

**Files:**
- Modify: `src/core/html_parser.rs:157-176`
- Test: `src/core/html_parser.rs` (인라인 테스트)

**Step 1: 기존 테스트가 통과하는지 확인**

```bash
cargo test --lib html_parser -- --nocapture
```

Expected: 모든 기존 테스트 PASS (기존 테스트는 `name=` HTML을 사용하므로 통과)

**Step 2: 실제 HTML 구조에 맞는 실패 테스트 추가**

`src/core/html_parser.rs`의 `mod tests`에 추가:

```rust
#[test]
fn test_parse_openapi_page_id_attribute() {
    // 실제 data.go.kr HTML은 id= 사용 (name= 아님)
    let html = r#"
    <html><body>
        <input type="hidden" id="publicDataDetailPk"
               value="uddi:b295d381-f52d-4318-9191-96fe1fafff1f"/>
        <input type="hidden" id="publicDataPk" value="15061357"/>
        <select id="open_api_detail_select">
            <option value="25356">선물사일반현황조회</option>
            <option value="25357">선물사재무현황조회</option>
        </select>
    </body></html>
    "#;

    let info = parse_openapi_page(html).unwrap();
    assert_eq!(
        info.public_data_detail_pk,
        "uddi:b295d381-f52d-4318-9191-96fe1fafff1f"
    );
    assert_eq!(info.operations.len(), 2);
    assert_eq!(info.operations[0].seq_no, "25356");
    assert_eq!(info.operations[0].name, "선물사일반현황조회");
}
```

```bash
cargo test --lib html_parser::tests::test_parse_openapi_page_id_attribute -- --nocapture
```

Expected: **FAIL** — `publicDataDetailPk를 찾을 수 없습니다`

**Step 3: 셀렉터 수정 — `name=` → `id=` 우선, `name=` 폴백**

`src/core/html_parser.rs` `extract_public_data_detail_pk` 함수를 수정:

```rust
fn extract_public_data_detail_pk(document: &Html, raw_html: &str) -> Option<String> {
    // 1) id= 셀렉터 (실제 data.go.kr 구조)
    if let Ok(sel) = Selector::parse(r#"input#publicDataDetailPk"#) {
        if let Some(el) = document.select(&sel).next() {
            if let Some(val) = el.value().attr("value") {
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }

    // 2) name= 셀렉터 (하위 호환)
    if let Ok(sel) = Selector::parse(r#"input[name="publicDataDetailPk"]"#) {
        if let Some(el) = document.select(&sel).next() {
            if let Some(val) = el.value().attr("value") {
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }

    // 3) regex fallback (id= 또는 name= 모두 매칭)
    let re = regex::Regex::new(
        r#"(?s)(?:name|id)\s*=\s*["']?publicDataDetailPk["']?\s+value\s*=\s*["']([^"']+)["']"#,
    )
    .ok()?;
    re.captures(raw_html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}
```

**Step 4: 테스트 통과 확인**

```bash
cargo test --lib html_parser -- --nocapture
```

Expected: 모든 테스트 PASS (기존 `name=` 테스트 + 새 `id=` 테스트)

**Step 5: 커밋**

```bash
git add src/core/html_parser.rs
git commit -m "fix(html_parser): id= 셀렉터 지원 — publicDataDetailPk 탐지 복구"
```

---

## Task 2: html_survey 바이너리 — 구조체 + 페이지 분석

**Files:**
- Create: `src/bin/html_survey.rs`
- Modify: `Cargo.toml` (바이너리 등록)

**Step 1: Cargo.toml에 바이너리 등록**

`Cargo.toml` `[[bin]]` 섹션에 추가:

```toml
[[bin]]
name = "html-survey"
path = "src/bin/html_survey.rs"
```

**Step 2: 구조체 + 페이지 분석 함수 작성**

`src/bin/html_survey.rs` 생성. 핵심 구조:

```rust
/// 개별 API HTML 조사 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HtmlSurveyEntry {
    list_id: String,
    title: String,

    // 페이지 접근
    http_status: Option<u16>,
    fetch_error: Option<String>,
    page_size_bytes: usize,

    // Swagger 상태 (교차 비교용)
    has_swagger_json: bool,
    swagger_json_empty: bool,       // var swaggerJson = ``
    swagger_ops_count: Option<usize>,

    // HTML pk 탐지 (수정된 셀렉터)
    has_pk: bool,
    pk_value: Option<String>,
    pk_source: Option<String>,      // "id_attr", "name_attr", "regex"

    // select 옵션
    select_option_count: usize,
    select_options: Vec<SelectOption>,

    // AJAX 프로브 (첫 번째 operation)
    ajax_attempted: bool,
    ajax_status: Option<u16>,
    ajax_error: Option<String>,
    ajax_response_bytes: Option<usize>,
    ajax_has_request_url: bool,
    ajax_has_service_url: bool,
    ajax_request_url: Option<String>,
    ajax_service_url: Option<String>,
    ajax_param_count: usize,
    ajax_param_style: Option<String>,   // "data_attr", "td_fallback", "none"
    ajax_response_field_count: usize,
    ajax_is_error_page: bool,

    // 페이지 구조 분류
    page_pattern: String,
    anomalies: Vec<String>,
}
```

`analyze_page_html` 함수: openapi.do HTML에서 pk, select, swagger 상태를 추출.
pk 탐지 시 어떤 방법(id_attr, name_attr, regex)으로 찾았는지도 기록.

**Step 3: 컴파일 확인**

```bash
cargo check --bin html-survey
```

Expected: OK

**Step 4: 커밋**

```bash
git add src/bin/html_survey.rs Cargo.toml
git commit -m "feat(html-survey): 구조체 + 페이지 분석 함수"
```

---

## Task 3: html_survey — AJAX 프로브 + 메인 루프

**Files:**
- Modify: `src/bin/html_survey.rs`

**Step 1: AJAX 프로브 함수 작성**

`probe_ajax` 함수: pk와 첫 번째 operation seq_no로 `/tcs/dss/selectApiDetailFunction.do` POST 호출.
- Referer 헤더 필수: `https://www.data.go.kr/data/{list_id}/openapi.do`
- 응답에서 요청주소/서비스URL/파라미터 추출 시도
- 에러 페이지 감지: `<title>에러` 또는 `요청하신 페이지를 찾을 수 없습니다` 포함 여부

```rust
async fn probe_ajax(
    client: &reqwest::Client,
    list_id: &str,
    pk: &str,
    first_seq_no: &str,
) -> AjaxProbeResult { ... }
```

**Step 2: 메인 루프 — 2-phase 조사**

```
Phase 1: openapi.do 페이지 fetch + HTML 분석 (전체 12K)
Phase 2: pk가 있고 select 옵션이 있는 API에 대해 AJAX 프로브 (예상 ~6K)
```

기존 survey.rs 패턴 재활용: `futures::stream::buffer_unordered`, 500건마다 진행 로그, `--resume` 지원.

CLI args:
```
--api-key KEY       # 메타 API 키 (또는 DATA_GO_KR_API_KEY 환경변수)
--output PATH       # 출력 JSON (기본: data/html-survey.json)
--concurrency N     # 동시 요청 수 (기본: 5)
--delay MS          # 요청 간 딜레이 (기본: 100)
--resume            # 기존 결과 이어하기
--phase1-only       # AJAX 프로브 생략 (페이지 분석만)
```

**Step 3: 요약 출력 함수**

조사 완료 후 콘솔에 출력:

```
[pk 탐지]
  id= 발견: NNNN
  name= 발견: NNNN
  regex 발견: NNNN
  미발견: NNNN

[select 옵션]
  1+ 옵션: NNNN
  0 옵션: NNNN

[AJAX 프로브]
  성공 (요청주소+파라미터): NNNN
  성공 (부분): NNNN
  에러 페이지: NNNN
  HTTP 에러: NNNN
  미시도: NNNN

[교차 분석]
  Swagger 있음 + pk 있음: NNNN (중복 — 이미 Available)
  Swagger 없음 + pk+AJAX 성공: NNNN (← 신규 추출 가능!)
  Swagger 없음 + pk 없음: NNNN (추출 불가)

[page_pattern 분포]
  swagger_full: NNNN
  swagger_empty_html_available: NNNN
  html_only: NNNN
  ...
```

**Step 4: 빌드 + 실행 테스트**

```bash
cargo build --bin html-survey
# 빌드 성공 확인
```

**Step 5: 커밋**

```bash
git add src/bin/html_survey.rs
git commit -m "feat(html-survey): AJAX 프로브 + 메인 루프 + 요약 출력"
```

---

## Task 4: 전수조사 실행 + 결과 보고서

**Step 1: 전수조사 실행 (Phase 1 + Phase 2)**

```bash
cargo run --bin html-survey -- --api-key $DATA_GO_KR_API_KEY --output data/html-survey.json
```

예상 소요: 30-40분 (12K 페이지 + ~6K AJAX 프로브)

**Step 2: 결과 분석 + 보고서 작성**

`data/html-survey-report.md` 생성:
- pk 탐지율 (id/name/regex별)
- select 옵션 분포
- AJAX 성공률 및 응답 패턴 분류
- 신규 추출 가능 API 수 (Swagger 없음 + AJAX 성공)
- 페이지 템플릿 변형 목록
- anomaly 패턴 분석
- 범용 솔루션 제안

**Step 3: 커밋**

```bash
git add data/html-survey.json data/html-survey-report.md
git commit -m "feat: HTML 스펙 전수조사 결과 + 보고서"
```

---

## 조사 완료 후 다음 단계 (이 플랜 범위 밖)

조사 결과에 따라:
1. `build_bundle.rs`에 HTML 폴백 경로 추가 (Swagger 실패 → pk+AJAX → parse_operation_detail)
2. `html_parser.rs`의 `parse_operation_detail`을 AJAX 응답 변동성에 맞게 보강
3. 번들 리빌드 + 커버리지 검증
