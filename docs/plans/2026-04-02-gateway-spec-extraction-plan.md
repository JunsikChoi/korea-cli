# Gateway API 스펙 추출 구현 플랜

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Swagger가 없는 Gateway API(클러스터 3+7, ~3,187개)에서 AJAX 호출로 ApiSpec을 추출하여 Available API 수를 ~4,042 → ~7,229로 +79% 증가시킨다.

**Architecture:** 분류 우선 파이프라인 — HTML에서 타입을 먼저 판별(tyDetailCode, swaggerJson, select 존재 여부)한 뒤 해당 전략으로 추출. Gateway API(Pattern 3)는 API별 독립 reqwest Client로 쿠키 격리하여 `selectApiDetailFunction.do` AJAX 호출.

**Tech Stack:** Rust, scraper (HTML 파싱), reqwest (HTTP + cookie_store), tokio (async + Semaphore), regex

**Spec:** `docs/specs/2026-04-02-gateway-spec-extraction-design.md`

---

## Task 1: `types.rs` — classify를 `ClassificationHints` 구조체 기반으로 변경

> **감사 반영 [B1]**: bool 3개 positional 인자 → named fields 구조체. 인자 순서 혼동 방지 + 향후 확장 용이.

**Files:**
- Modify: `src/core/types.rs:90-116` (classify 함수)
- Modify: `src/core/types.rs:441-494` (기존 classify 테스트들)

**Step 1: `ClassificationHints` 구조체 + 테스트 추가 (RED)**

`src/core/types.rs`에 구조체 정의 + 기존 테스트를 구조체 기반으로 변경 + 새 테스트:

```rust
/// classify 함수의 입력 — named fields로 인자 순서 혼동 방지
#[derive(Debug, Default)]
pub struct ClassificationHints<'a> {
    pub has_spec: bool,
    pub is_skeleton: bool,
    pub endpoint_url: &'a str,
    pub is_link_api: bool,
}
```

기존 테스트를 모두 구조체 기반으로:

```rust
// test_classify_available (line 443)
SpecStatus::classify(&ClassificationHints {
    has_spec: true,
    endpoint_url: "https://apis.data.go.kr/x",
    ..Default::default()
}),

// test_classify_skeleton (line 451)
SpecStatus::classify(&ClassificationHints {
    is_skeleton: true,
    endpoint_url: "https://apihub.kma.go.kr/x",
    ..Default::default()
}),

// test_classify_unsupported_wms (line 459)
SpecStatus::classify(&ClassificationHints {
    endpoint_url: "https://example.kr/wms/service",
    ..Default::default()
}),

// test_classify_html_only (line 467)
SpecStatus::classify(&ClassificationHints {
    endpoint_url: "https://apis.data.go.kr/1360000/Weather",
    ..Default::default()
}),

// test_classify_external (line 475)
SpecStatus::classify(&ClassificationHints {
    endpoint_url: "https://apihub.kma.go.kr/api/typ01",
    ..Default::default()
}),

// test_classify_catalog_only (line 483)
SpecStatus::classify(&ClassificationHints::default()),

// test_classify_odcloud_no_spec (line 491)
SpecStatus::classify(&ClassificationHints {
    endpoint_url: "https://api.odcloud.kr/api/test",
    ..Default::default()
}),
```

새 테스트:

```rust
#[test]
fn test_classify_link_api_returns_external() {
    // is_link_api가 true면 스펙 유무와 무관하게 External
    assert_eq!(
        SpecStatus::classify(&ClassificationHints {
            endpoint_url: "https://apis.data.go.kr/1360000/Weather",
            is_link_api: true,
            ..Default::default()
        }),
        SpecStatus::External,
    );
    // has_spec이 true여도 is_link_api가 우선
    assert_eq!(
        SpecStatus::classify(&ClassificationHints {
            has_spec: true,
            endpoint_url: "https://apis.data.go.kr/x",
            is_link_api: true,
            ..Default::default()
        }),
        SpecStatus::External,
    );
}
```

**Step 2: `cargo test` 실행 — 컴파일 에러 확인**

Run: `cargo test --lib core::types::tests`
Expected: 컴파일 에러 — `ClassificationHints` 타입 없음, `classify` 시그니처 불일치

**Step 3: classify 시그니처 변경 + 구현**

`src/core/types.rs:90`의 `classify` 함수를 수정:

```rust
pub fn classify(hints: &ClassificationHints) -> Self {
    if hints.is_link_api {
        return Self::External;
    }
    if hints.has_spec {
        return Self::Available;
    }
    if hints.is_skeleton {
        return Self::Skeleton;
    }
    let url_lower = hints.endpoint_url.to_lowercase();
    if url_lower.contains("wms") || url_lower.contains("wfs") || url_lower.contains("wcs") {
        return Self::Unsupported;
    }
    if hints.endpoint_url.contains("apis.data.go.kr") {
        return Self::HtmlOnly;
    }
    if !hints.endpoint_url.is_empty()
        && !hints.endpoint_url.contains("data.go.kr")
        && !hints.endpoint_url.contains("api.odcloud.kr")
    {
        return Self::External;
    }
    if hints.endpoint_url.is_empty() {
        return Self::CatalogOnly;
    }
    Self::HtmlOnly
}
```

**Step 4: `cargo test` 실행 — 타입 테스트 통과 확인**

Run: `cargo test --lib core::types::tests`
Expected: 컴파일 에러 — `build_bundle.rs`와 `survey.rs`의 호출이 아직 옛 시그니처

**Step 5: 호출부 수정 — build_bundle.rs**

`src/bin/build_bundle.rs:67-71`:

```rust
let spec_status = SpecStatus::classify(&ClassificationHints {
    has_spec: specs.contains_key(&svc.list_id),
    is_skeleton: skeleton_ids.contains(&svc.list_id),
    endpoint_url: &svc.endpoint_url,
    ..Default::default() // is_link_api — 추후 Pattern 3에서 true로 설정
});
```

**Step 6: 호출부 수정 — survey.rs**

`src/bin/survey.rs:266`:

```rust
korea_cli::core::types::SpecStatus::classify(&korea_cli::core::types::ClassificationHints {
    has_spec,
    is_skeleton,
    endpoint_url,
    ..Default::default()
})
```

**Step 7: `cargo test` + `cargo clippy` 전체 통과 확인**

Run: `cargo test && cargo clippy`
Expected: ALL PASS, no warnings

**Step 8: 커밋**

```bash
git add src/core/types.rs src/bin/build_bundle.rs src/bin/survey.rs
git commit -m "feat: classify를 ClassificationHints 구조체 기반으로 변경 — LINK API 분류 추가"
```

---

## Task 2: `html_parser.rs` — PageInfo에 tyDetailCode + publicDataPk 추가

**Files:**
- Modify: `src/core/html_parser.rs:12-16` (PageInfo 구조체)
- Modify: `src/core/html_parser.rs:35-49` (parse_openapi_page 함수)
- Modify: `src/core/html_parser.rs:157-188` (내부 헬퍼)
- Modify: `src/core/html_parser.rs:369-533` (테스트)

**Step 1: 테스트 추가 (RED)**

`src/core/html_parser.rs` 테스트 모듈에 추가:

```rust
#[test]
fn test_parse_openapi_page_extracts_ty_detail_code() {
    let html = r#"
    <html><body>
        <script>
            var tyDetailCode = 'PRDE04';
        </script>
        <input type="hidden" id="publicDataDetailPk" value="uddi:abc-123">
        <input type="hidden" id="publicDataPk" value="15061357">
    </body></html>
    "#;
    let info = parse_openapi_page(html).unwrap();
    assert_eq!(info.ty_detail_code.as_deref(), Some("PRDE04"));
    assert_eq!(info.public_data_pk.as_deref(), Some("15061357"));
}

#[test]
fn test_parse_openapi_page_no_ty_detail_code() {
    let html = r#"
    <html><body>
        <input type="hidden" name="publicDataDetailPk" value="uddi:12345-abcde">
        <select id="open_api_detail_select">
            <option value="1001">getWeather</option>
        </select>
    </body></html>
    "#;
    let info = parse_openapi_page(html).unwrap();
    assert!(info.ty_detail_code.is_none());
    assert!(info.public_data_pk.is_none());
}
```

**Step 2: `cargo test` — 컴파일 에러 확인**

Run: `cargo test --lib core::html_parser::tests`
Expected: FAIL — `ty_detail_code`, `public_data_pk` 필드 없음

**Step 3: PageInfo 확장 + 추출 로직 구현**

`PageInfo` 구조체에 필드 추가:

```rust
#[derive(Debug, Clone)]
pub struct PageInfo {
    pub public_data_detail_pk: String,
    pub public_data_pk: Option<String>,
    pub ty_detail_code: Option<String>,
    pub operations: Vec<OperationOption>,
}
```

`parse_openapi_page` 함수에서 추출:

```rust
pub fn parse_openapi_page(html: &str) -> Result<PageInfo> {
    let document = Html::parse_document(html);

    let pk = extract_public_data_detail_pk(&document, html)
        .context("publicDataDetailPk를 찾을 수 없습니다")?;
    let public_data_pk = extract_hidden_input_value(&document, "publicDataPk");
    let ty_detail_code = extract_ty_detail_code(html);
    let operations = extract_operation_options(&document);

    Ok(PageInfo {
        public_data_detail_pk: pk,
        public_data_pk,
        ty_detail_code,
        operations,
    })
}
```

새 헬퍼 함수들:

> **감사 반영 [W2]**: regex가 더블쿼트도 매칭 + `LazyLock`으로 재컴파일 방지.

```rust
fn extract_ty_detail_code(raw_html: &str) -> Option<String> {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"var\s+tyDetailCode\s*=\s*["']([^"']+)["']"#).unwrap()
    });
    RE.captures(raw_html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_hidden_input_value(document: &Html, id: &str) -> Option<String> {
    let selector = Selector::parse(&format!("input#{id}")).ok()?;
    document.select(&selector).next()
        .and_then(|el| el.value().attr("value"))
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}
```

더블쿼트 테스트도 추가 (Step 1의 테스트 모듈에):

```rust
#[test]
fn test_parse_openapi_page_extracts_ty_detail_code_double_quotes() {
    let html = r#"
    <html><body>
        <script>
            var tyDetailCode = "PRDE04";
        </script>
        <input type="hidden" id="publicDataDetailPk" value="uddi:abc-123">
    </body></html>
    "#;
    let info = parse_openapi_page(html).unwrap();
    assert_eq!(info.ty_detail_code.as_deref(), Some("PRDE04"));
}
```

**Step 4: `cargo test` — 통과 확인**

Run: `cargo test --lib core::html_parser::tests`
Expected: ALL PASS

**Step 5: 커밋**

```bash
git add src/core/html_parser.rs
git commit -m "feat: PageInfo에 ty_detail_code + public_data_pk 추출 추가"
```

---

## Task 3: `html_parser.rs` — ParsedOperation에 summary 추출 추가

**Files:**
- Modify: `src/core/html_parser.rs:28-32` (ParsedOperation)
- Modify: `src/core/html_parser.rs:52-66` (parse_operation_detail)
- Modify: `src/core/html_parser.rs:93-131` (build_api_spec)

**Step 1: 테스트 추가 (RED)**

```rust
#[test]
fn test_parse_operation_detail_extracts_summary() {
    let html = r#"
    <div id="open-api-detail-result">
        <h4 class="tit">초단기실황조회</h4>
        <p><strong>요청주소</strong> https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0/getUltraSrtNcst</p>
        <p><strong>서비스URL</strong> https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0</p>
        <table>
            <tr data-paramtr-nm="serviceKey" data-paramtr-division="필수" data-paramtr-dc="인증키">
                <td>1</td><td>serviceKey</td><td>string</td><td>100</td><td>필수</td><td>인증키</td>
            </tr>
        </table>
    </div>
    "#;

    let op = parse_operation_detail(html).unwrap();
    assert_eq!(op.summary, "초단기실황조회");
}

#[test]
fn test_parse_operation_detail_no_summary() {
    let html = r#"
    <div>
        <p><strong>요청주소</strong> https://apis.data.go.kr/test/getItems</p>
        <p><strong>서비스URL</strong> https://apis.data.go.kr/test</p>
    </div>
    "#;
    let op = parse_operation_detail(html).unwrap();
    assert!(op.summary.is_empty());
}
```

**Step 2: `cargo test` — 컴파일 에러 확인**

Run: `cargo test --lib core::html_parser::tests`
Expected: FAIL — `summary` 필드 없음

**Step 3: 구현**

`ParsedOperation`에 `summary` 추가:

```rust
#[derive(Debug, Clone)]
pub struct ParsedOperation {
    pub request_url: String,
    pub service_url: String,
    pub summary: String,
    pub parameters: Vec<Parameter>,
    pub response_fields: Vec<ResponseField>,
}
```

`parse_operation_detail`에서 추출:

```rust
pub fn parse_operation_detail(html: &str) -> Result<ParsedOperation> {
    let document = Html::parse_fragment(html);

    let request_url = extract_labeled_url(&document, "요청주소").unwrap_or_default();
    let service_url = extract_labeled_url(&document, "서비스URL").unwrap_or_default();
    let summary = extract_summary(&document);
    let parameters = extract_request_params(&document);
    let response_fields = extract_response_fields(&document);

    Ok(ParsedOperation {
        request_url,
        service_url,
        summary,
        parameters,
        response_fields,
    })
}
```

헬퍼:

```rust
fn extract_summary(document: &Html) -> String {
    let sel = match Selector::parse("h4.tit") {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    document.select(&sel).next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default()
}
```

`build_api_spec`에서 summary 활용 — `src/core/html_parser.rs:124` 부근:

```rust
// 변경 전:
summary: String::new(),

// 변경 후:
summary: op.summary.clone(),
```

**Step 4: `cargo test` — 통과 확인**

Run: `cargo test --lib core::html_parser::tests`
Expected: ALL PASS

**Step 5: 커밋**

```bash
git add src/core/html_parser.rs
git commit -m "feat: ParsedOperation에 summary 추출 추가"
```

---

## Task 4: `html_parser.rs` — 응답 필드 파싱 개선 (h4 기반 테이블 분리)

**Files:**
- Modify: `src/core/html_parser.rs:320-366` (extract_response_fields)

**Step 1: 테스트 추가 (RED)**

현재 `extract_response_fields`는 `<tr>` 내 "출력결과" 텍스트로 섹션을 감지하지만, 실제 HTML은 `<h4>출력결과</h4>` 다음 별도 `<table>`. 실제 구조를 반영한 테스트:

```rust
#[test]
fn test_extract_response_fields_h4_based() {
    // 실제 data.go.kr AJAX 응답 구조: h4 + table 분리
    let html = r#"
    <div id="open-api-detail-result">
        <h4>요청변수(Request Parameter)</h4>
        <table>
            <tr><th>순번</th><th>항목명(영문)</th><th>타입</th><th>크기</th><th>항목구분</th><th>항목설명</th></tr>
            <tr><td>1</td><td>serviceKey</td><td>string</td><td>100</td><td>필수</td><td>인증키</td></tr>
        </table>
        <h4>출력결과(Response Element)</h4>
        <table>
            <tr><th>순번</th><th>항목명(영문)</th><th>타입</th><th>크기</th><th>항목설명</th></tr>
            <tr><td>1</td><td>resultCode</td><td>string</td><td>2</td><td>결과코드</td></tr>
            <tr><td>2</td><td>resultMsg</td><td>string</td><td>50</td><td>결과메시지</td></tr>
            <tr><td>3</td><td>baseDate</td><td>string</td><td>8</td><td>발표일자</td></tr>
        </table>
    </div>
    "#;

    let document = Html::parse_fragment(html);
    let fields = extract_response_fields(&document);
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].name, "resultCode");
    assert_eq!(fields[0].description, "결과코드");
    assert_eq!(fields[1].name, "resultMsg");
    assert_eq!(fields[2].name, "baseDate");
}
```

주의: `extract_response_fields`는 현재 `fn` (비공개). 이 테스트는 모듈 내부에서만 접근 가능하므로 `#[cfg(test)] mod tests` 안에 놓는다.

**Step 2: `cargo test` — 실패 확인**

Run: `cargo test --lib core::html_parser::tests::test_extract_response_fields_h4_based`
Expected: FAIL — 현재 로직은 `<tr>` 내에서 "출력결과" 텍스트를 찾으므로 `<h4>` 기반 구조에서 응답 필드를 못 찾음

**Step 3: extract_response_fields 개선**

```rust
fn extract_response_fields(document: &Html) -> Vec<ResponseField> {
    // Strategy 1: h4 기반 — "출력결과" h4 다음의 table에서 추출
    if let Some(fields) = extract_response_fields_by_h4(document) {
        if !fields.is_empty() {
            return fields;
        }
    }

    // Strategy 2 (fallback): 기존 tr 스캔 방식 — 레거시 호환
    extract_response_fields_by_tr_scan(document)
}

fn extract_response_fields_by_h4(document: &Html) -> Option<Vec<ResponseField>> {
    use scraper::node::Node;

    let h4_sel = Selector::parse("h4").ok()?;

    for h4 in document.select(&h4_sel) {
        let text = h4.text().collect::<String>();
        if !text.contains("출력결과") {
            continue;
        }

        // h4 다음 형제 노드에서 첫 번째 <table> 찾기
        let mut sibling = h4.next_sibling();
        while let Some(node) = sibling {
            if let Node::Element(ref el) = node.value() {
                if el.name() == "table" {
                    // 감사 반영 [B2]: `?` 대신 `if let` — wrap이 None이어도 계속 탐색
                    if let Some(table_el) = scraper::ElementRef::wrap(node) {
                        return Some(parse_table_rows_as_response_fields(table_el));
                    }
                }
            }
            sibling = node.next_sibling();
        }
    }
    None
}

fn parse_table_rows_as_response_fields(table: scraper::ElementRef) -> Vec<ResponseField> {
    let tr_sel = match Selector::parse("tr") { Ok(s) => s, Err(_) => return vec![] };
    let td_sel = match Selector::parse("td") { Ok(s) => s, Err(_) => return vec![] };

    let mut fields = Vec::new();
    for row in table.select(&tr_sel) {
        let cells: Vec<String> = row
            .select(&td_sel)
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();

        // 최소 3컬럼: 순번, 항목명, ... , 설명
        if cells.len() >= 3 {
            let name = &cells[1];
            if name.is_empty() || name == "항목명(영문)" || name == "항목명" {
                continue;
            }
            let description = cells.last().cloned().unwrap_or_default();
            fields.push(ResponseField {
                name: name.clone(),
                description,
                field_type: "string".to_string(),
            });
        }
    }
    fields
}

fn extract_response_fields_by_tr_scan(document: &Html) -> Vec<ResponseField> {
    // 기존 extract_response_fields 로직 그대로 (lines 320-366의 현재 코드)
    let td_sel = match Selector::parse("td") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let tr_sel = match Selector::parse("tr") {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut fields = Vec::new();
    let mut in_response_section = false;

    for row in document.select(&tr_sel) {
        let cells: Vec<String> = row
            .select(&td_sel)
            .map(|td| td.text().collect::<String>().trim().to_string())
            .collect();

        let row_text = row.text().collect::<String>();
        if row_text.contains("출력결과") || row_text.contains("응답메시지") {
            in_response_section = true;
            continue;
        }

        if in_response_section && cells.len() >= 3 {
            let name = &cells[1];
            if name.is_empty() || name == "항목명(영문)" || name == "항목명" {
                continue;
            }
            let description = if cells.len() >= 6 {
                cells[5].clone()
            } else {
                cells.last().cloned().unwrap_or_default()
            };

            fields.push(ResponseField {
                name: name.clone(),
                description,
                field_type: "string".to_string(),
            });
        }
    }
    fields
}
```

**Step 4: `cargo test` — 통과 확인**

Run: `cargo test --lib core::html_parser::tests`
Expected: ALL PASS (새 테스트 + 기존 테스트 모두)

**Step 5: `cargo clippy`**

Run: `cargo clippy`
Expected: no warnings

**Step 6: 커밋**

```bash
git add src/core/html_parser.rs
git commit -m "fix: extract_response_fields를 h4 기반으로 개선 — 실제 AJAX HTML 구조 대응"
```

---

## Task 5: `html_parser.rs` — 빈 요청주소 + service_url fallback

> **감사 반영 [W6]**: Task 3 (summary 필드 추가) 이후에 실행해야 한다. 테스트 fixture가 `summary` 필드를 사용하므로 Task 3 없이는 컴파일 에러.

**Files:**
- Modify: `src/core/html_parser.rs:93-131` (build_api_spec의 Operation 생성 로직)

**Step 1: 테스트 추가 (RED)**

```rust
#[test]
fn test_build_api_spec_empty_request_url_uses_service_url() {
    // 요청주소가 비어있고 서비스URL만 있는 케이스
    let ops = vec![ParsedOperation {
        request_url: "".into(),
        service_url: "https://apis.data.go.kr/1360000/VilageFcstInfoService_2.0".into(),
        summary: "초단기실황조회".into(),
        parameters: vec![Parameter {
            name: "serviceKey".into(),
            description: "인증키".into(),
            location: ParamLocation::Query,
            param_type: "string".into(),
            required: true,
            default: None,
        }],
        response_fields: vec![],
    }];

    let spec = build_api_spec("15084084", &ops);
    assert!(spec.is_some(), "빈 request_url이어도 service_url이 있으면 Operation 생성");
    let spec = spec.unwrap();
    assert_eq!(spec.operations.len(), 1);
    assert_eq!(spec.operations[0].path, "/");
    assert_eq!(spec.operations[0].summary, "초단기실황조회");
}
```

**Step 2: `cargo test` — 실패 확인**

Run: `cargo test --lib core::html_parser::tests::test_build_api_spec_empty_request_url_uses_service_url`
Expected: FAIL — 현재 코드는 `path.is_empty() && op.request_url.is_empty()`일 때 `return None` (line 105-106)

**Step 3: build_api_spec 수정**

`src/core/html_parser.rs` lines 93-131의 `.filter_map` 클로저를 수정:

```rust
let operations: Vec<Operation> = parsed_ops
    .iter()
    .filter_map(|op| {
        // request_url이 있으면 base_url 기준으로 path 추출
        // 없으면 service_url을 사용하고 path는 "/"
        let path = if !op.request_url.is_empty() && !base_url.is_empty() {
            op.request_url
                .strip_prefix(&base_url)
                .unwrap_or(&op.request_url)
                .to_string()
        } else if !op.request_url.is_empty() {
            op.request_url.clone()
        } else if !op.service_url.is_empty() {
            // 빈 요청주소 + 서비스URL만 있는 경우 → path "/"
            "/".to_string()
        } else {
            return None; // 둘 다 비어있으면 skip
        };

        let final_path = if path.is_empty() { "/".to_string() } else { path };

        let params: Vec<Parameter> = op
            .parameters
            .iter()
            .filter(|p| !p.name.eq_ignore_ascii_case("serviceKey"))
            .cloned()
            .collect();

        Some(Operation {
            path: final_path,
            method: HttpMethod::Get,
            summary: op.summary.clone(),
            content_type: ContentType::None,
            parameters: params,
            request_body: None,
            response_fields: op.response_fields.clone(),
        })
    })
    .collect();
```

**Step 4: `cargo test` — 통과 확인**

Run: `cargo test --lib core::html_parser::tests`
Expected: ALL PASS

**Step 5: 커밋**

```bash
git add src/core/html_parser.rs
git commit -m "fix: 빈 요청주소일 때 service_url fallback — Operation 누락 방지"
```

---

## Task 6: `build_bundle.rs` — BuildConfig에 AJAX 파라미터 추가

**Files:**
- Modify: `src/bin/build_bundle.rs:19-25` (BuildConfig)
- Modify: `src/bin/build_bundle.rs:225-246` (parse_args)

**Step 1: BuildConfig에 필드 추가**

```rust
#[derive(Clone)]
struct BuildConfig {
    api_key: String,
    output: String,
    concurrency: usize,
    delay_ms: u64,
    ajax_concurrency: usize,
    ajax_delay_ms: u64,
}
```

**Step 2: parse_args 확장**

```rust
let ajax_concurrency: usize = get_arg(&args, "--ajax-concurrency")
    .and_then(|s| s.parse().ok())
    .unwrap_or(10);
let ajax_delay_ms: u64 = get_arg(&args, "--ajax-delay")
    .and_then(|s| s.parse().ok())
    .unwrap_or(50);

Ok(BuildConfig {
    api_key,
    output,
    concurrency,
    delay_ms,
    ajax_concurrency,
    ajax_delay_ms,
})
```

**Step 3: `cargo check --bin build-bundle` — 컴파일 확인**

Run: `cargo check --bin build-bundle`
Expected: PASS (경고 가능 — unused fields는 다음 Task에서 사용)

**Step 4: 커밋**

```bash
git add src/bin/build_bundle.rs
git commit -m "feat: BuildConfig에 ajax_concurrency, ajax_delay_ms 파라미터 추가"
```

---

## Task 7: `build_bundle.rs` — 분류 우선 파이프라인 + Pattern 3 통합

이 Task가 핵심이다. `fetch_single_spec`을 확장하고 `collect_specs`에 AJAX Semaphore를 추가한다.

**Files:**
- Modify: `src/bin/build_bundle.rs:136-191` (collect_specs)
- Modify: `src/bin/build_bundle.rs:193-223` (fetch_single_spec)
- Add import: `korea_cli::core::html_parser`

**Step 1: import 추가**

`src/bin/build_bundle.rs` 상단에:

```rust
use korea_cli::core::html_parser::{parse_openapi_page, parse_operation_detail, build_api_spec};
```

**Step 2: `SpecResult` 열거형 정의**

`fetch_single_spec`이 분류 정보도 반환하도록 내부 타입 추가:

> **감사 반영 [W3]**: `#[derive(Debug)]` 추가.

```rust
/// fetch_single_spec 결과 — 스펙 또는 분류 힌트
#[derive(Debug)]
enum SpecResult {
    /// 스펙 추출 성공 (is_gateway: Pattern 3 경유 여부)
    Spec { spec: ApiSpec, is_gateway: bool },
    /// 스펙 없음 — 분류 힌트와 bail 이유
    Bail { is_link_api: bool, reason: String },
}
```

**Step 3: collect_specs 시그니처 변경**

`collect_specs`가 `SpecResult`를 반환하도록 변경하고, `main`에서 분류 시 `is_link_api` 활용:

```rust
async fn collect_specs(
    services: &[ApiService],
    config: &BuildConfig,
) -> Vec<(String, SpecResult)> {
    let client = Arc::new(
        reqwest::Client::builder()
            .user_agent("korea-cli-builder/0.1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to build HTTP client"),
    );

    let ajax_semaphore = Arc::new(tokio::sync::Semaphore::new(config.ajax_concurrency));
    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));
    let gateway_count = Arc::new(AtomicUsize::new(0));
    let total = services.len();

    let results: Vec<(String, SpecResult)> = stream::iter(services.iter())
        .map(|svc| {
            let client = client.clone();
            let list_id = svc.list_id.clone();
            let delay_ms = config.delay_ms;
            let ajax_delay_ms = config.ajax_delay_ms;
            let ajax_sem = ajax_semaphore.clone();
            let sc = success_count.clone();
            let fc = fail_count.clone();
            let gc = gateway_count.clone();

            async move {
                let result = fetch_single_spec(&client, &list_id, &ajax_sem, ajax_delay_ms).await;

                // 감사 반영 [W7]: gateway_count를 SpecResult::Spec의 is_gateway 플래그로 카운팅
                match &result {
                    SpecResult::Spec { is_gateway, .. } => {
                        sc.fetch_add(1, Ordering::Relaxed);
                        if *is_gateway {
                            gc.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    SpecResult::Bail { reason, .. } => {
                        fc.fetch_add(1, Ordering::Relaxed);
                        let done = sc.load(Ordering::Relaxed) + fc.load(Ordering::Relaxed);
                        if done <= 20 || done % 500 == 0 {
                            eprintln!("  SKIP {list_id}: {reason}");
                        }
                    }
                }

                let done = sc.load(Ordering::Relaxed) + fc.load(Ordering::Relaxed);
                if done % 500 == 0 {
                    eprintln!(
                        "  진행: {done}/{total} ({} OK, {} FAIL, {} Gateway)",
                        sc.load(Ordering::Relaxed),
                        fc.load(Ordering::Relaxed),
                        gc.load(Ordering::Relaxed),
                    );
                }

                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                (list_id, result)
            }
        })
        .buffer_unordered(config.concurrency)
        .collect()
        .await;

    results
}
```

**Step 4: fetch_single_spec 확장 — 분류 우선 파이프라인**

```rust
async fn fetch_single_spec(
    client: &reqwest::Client,
    list_id: &str,
    ajax_semaphore: &tokio::sync::Semaphore,
    ajax_delay_ms: u64,
) -> SpecResult {
    let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");
    let html = match client.get(&page_url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(text) => text,
            Err(e) => return SpecResult::Bail {
                is_link_api: false,
                reason: format!("페이지 본문 읽기 실패: {e}"),
            },
        },
        Err(e) => return SpecResult::Bail {
            is_link_api: false,
            reason: format!("페이지 요청 실패: {e}"),
        },
    };

    // ① 타입 판별: tyDetailCode로 LINK API 즉시 분류
    let page_info = parse_openapi_page(&html).ok();
    if let Some(ref info) = page_info {
        if info.ty_detail_code.as_deref() == Some("PRDE04") {
            return SpecResult::Bail {
                is_link_api: true,
                reason: "LINK API (PRDE04)".into(),
            };
        }
    }

    // ② Pattern 1: inline swaggerJson
    if let Some(json) = extract_swagger_json(&html) {
        return match parse_swagger(list_id, &json) {
            Ok(spec) => SpecResult::Spec { spec, is_gateway: false },
            Err(e) => SpecResult::Bail {
                is_link_api: false,
                reason: format!("Swagger 파싱 실패: {e}"),
            },
        };
    }

    // ③ Pattern 2: external swaggerUrl
    if let Some(url) = extract_swagger_url(&html) {
        let spec_result = async {
            let spec_json: serde_json::Value = client
                .get(&url)
                .send()
                .await?
                .json()
                .await?;
            parse_swagger(list_id, &spec_json)
        }
        .await;
        return match spec_result {
            Ok(spec) => SpecResult::Spec { spec, is_gateway: false },
            Err(e) => SpecResult::Bail {
                is_link_api: false,
                reason: format!("Swagger URL 실패: {e}"),
            },
        };
    }

    // ④ Pattern 3: Gateway API (select 있음 → AJAX)
    if let Some(ref info) = page_info {
        if !info.operations.is_empty() {
            return fetch_gateway_spec(client, list_id, info, ajax_semaphore, ajax_delay_ms).await;
        }
    }

    // ⑤ bail — 어떤 패턴에도 매칭 안 됨
    SpecResult::Bail {
        is_link_api: false,
        reason: "swaggerJson/swaggerUrl/Gateway 모두 없음".into(),
    }
}
```

**Step 5: fetch_gateway_spec 함수 구현**

```rust
async fn fetch_gateway_spec(
    shared_client: &reqwest::Client,
    list_id: &str,
    page_info: &korea_cli::core::html_parser::PageInfo,
    ajax_semaphore: &tokio::sync::Semaphore,
    ajax_delay_ms: u64,
) -> SpecResult {
    // API별 독립 Client 생성 (쿠키 격리)
    let ajax_client = match reqwest::Client::builder()
        .user_agent("korea-cli-builder/0.1.0")
        .timeout(std::time::Duration::from_secs(15))
        .cookie_store(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => return SpecResult::Bail {
            is_link_api: false,
            reason: format!("AJAX client 생성 실패: {e}"),
        },
    };

    // 감사 반영 [W1]: 쿠키 획득을 위해 페이지 재요청 + 응답 본문 소비 + 상태 확인
    let page_url = format!("https://www.data.go.kr/data/{list_id}/openapi.do");
    match ajax_client.get(&page_url).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                return SpecResult::Bail {
                    is_link_api: false,
                    reason: format!("쿠키 획득 HTTP {}", resp.status()),
                };
            }
            // 응답 본문 소비하여 연결 정리
            let _ = resp.bytes().await;
        }
        Err(e) => {
            return SpecResult::Bail {
                is_link_api: false,
                reason: format!("쿠키 획득 실패: {e}"),
            };
        }
    }

    let public_data_pk = page_info.public_data_pk.as_deref().unwrap_or(list_id);
    let detail_pk = &page_info.public_data_detail_pk;

    let mut parsed_ops = Vec::new();
    let total_ops = page_info.operations.len();

    for op in &page_info.operations {
        // 글로벌 AJAX 동시 요청 제한
        let _permit = match ajax_semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => break,
        };

        let form = [
            ("oprtinSeqNo", op.seq_no.as_str()),
            ("publicDataDetailPk", detail_pk),
            ("publicDataPk", public_data_pk),
        ];

        let ajax_result = ajax_client
            .post("https://www.data.go.kr/tcs/dss/selectApiDetailFunction.do")
            .form(&form)
            .send()
            .await;

        // 감사 반영 [B3]: permit을 sleep 동안 의도적으로 보유한다.
        // 이유: sleep 전에 drop하면 다른 API가 즉시 acquire하여 서버에 burst 발생.
        // permit 보유 상태에서 sleep하면 실효 처리량 = ajax_concurrency / (RTT + delay).
        // 이것이 rate limiting 의도에 부합한다.
        tokio::time::sleep(std::time::Duration::from_millis(ajax_delay_ms)).await;
        drop(_permit);

        match ajax_result {
            Ok(resp) => {
                if let Ok(html) = resp.text().await {
                    match parse_operation_detail(&html) {
                        Ok(detail) => parsed_ops.push(detail),
                        Err(e) => eprintln!("  PARTIAL SKIP {list_id}/{}: {e}", op.seq_no),
                    }
                }
            }
            Err(e) => eprintln!("  PARTIAL SKIP {list_id}/{}: {e}", op.seq_no),
        }
    }

    if parsed_ops.is_empty() {
        return SpecResult::Bail {
            is_link_api: false,
            reason: format!("Gateway AJAX 전부 실패 (0/{total_ops} ops)"),
        };
    }

    if parsed_ops.len() < total_ops {
        eprintln!(
            "  PARTIAL: {}/{total_ops} operations ({list_id})",
            parsed_ops.len()
        );
    }

    match build_api_spec(list_id, &parsed_ops) {
        Some(spec) => SpecResult::Spec { spec, is_gateway: true },
        None => SpecResult::Bail {
            is_link_api: false,
            reason: format!("Gateway build_api_spec 실패 ({}/{total_ops} ops)", parsed_ops.len()),
        },
    }
}
```

**Step 6: main 함수 — SpecResult 기반으로 분류 로직 수정**

`src/bin/build_bundle.rs`의 `main` 함수에서 `collect_specs` 결과 처리 변경:

```rust
// Step 2: Collect specs (기존 코드 전체 교체)
eprintln!(
    "\n=== Step 2/4: spec 수집 (API 동시 {}건, AJAX 동시 {}건) ===",
    config.concurrency, config.ajax_concurrency
);
let all_results = collect_specs(&services, &config).await;

let mut specs: HashMap<String, ApiSpec> = HashMap::new();
let mut skeleton_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
let mut link_api_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

for (id, result) in all_results {
    match result {
        SpecResult::Spec { spec, .. } => {
            if spec.operations.is_empty() {
                skeleton_ids.insert(id);
            } else {
                specs.insert(id, spec);
            }
        }
        SpecResult::Bail { is_link_api: true, .. } => {
            link_api_ids.insert(id);
        }
        SpecResult::Bail { .. } => {}
    }
}
eprintln!(
    "  {}/{} spec ({:.1}%), skeleton {}, link_api {}",
    specs.len(),
    services.len(),
    (specs.len() as f64 / services.len() as f64) * 100.0,
    skeleton_ids.len(),
    link_api_ids.len(),
);

// Step 3: ClassificationHints로 classify
let catalog: Vec<CatalogEntry> = services
    .iter()
    .map(|svc| {
        let spec_status = SpecStatus::classify(&ClassificationHints {
            has_spec: specs.contains_key(&svc.list_id),
            is_skeleton: skeleton_ids.contains(&svc.list_id),
            endpoint_url: &svc.endpoint_url,
            is_link_api: link_api_ids.contains(&svc.list_id),
        });
        CatalogEntry {
            list_id: svc.list_id.clone(),
            title: svc.title.clone(),
            description: svc.description.clone(),
            keywords: svc.keywords.clone(),
            org_name: svc.org_name.clone(),
            category: svc.category.clone(),
            request_count: svc.request_count,
            endpoint_url: svc.endpoint_url.clone(),
            spec_status,
        }
    })
    .collect();
```

**Step 7: `cargo check --bin build-bundle` — 컴파일 확인**

Run: `cargo check --bin build-bundle`
Expected: PASS

**Step 8: `cargo clippy --bin build-bundle` — 린트 확인**

Run: `cargo clippy --bin build-bundle`
Expected: no errors

**Step 9: 커밋**

```bash
git add src/bin/build_bundle.rs
git commit -m "feat: 분류 우선 파이프라인 + Gateway API Pattern 3 AJAX 추출 통합"
```

---

## Task 8: 전체 빌드 + 테스트 통과 확인

**Files:** 없음 (검증만)

**Step 1: 전체 테스트**

Run: `cargo test`
Expected: ALL PASS

**Step 2: 전체 린트**

Run: `cargo clippy`
Expected: no errors, no warnings

**Step 3: 포매팅**

Run: `cargo fmt -- --check`
Expected: clean

**Step 4: 커밋 (필요 시)**

포매팅/린트 수정이 있었다면:

```bash
git add -A
git commit -m "chore: clippy + fmt 정리"
```

---

## Task 9: 소규모 E2E 검증 (수동)

**목적:** 실제 data.go.kr에서 Gateway API 3-5개를 Pattern 3으로 처리하여 E2E 확인.

**Step 1: 검증용 소규모 빌드 실행**

```bash
# 테스트할 Gateway API list_id 예시 (클러스터 3에서)
cargo run --bin build-bundle -- --api-key $DATA_GO_KR_API_KEY --concurrency 1 --ajax-concurrency 2 --ajax-delay 200 --output data/test-bundle.zstd
```

**Step 2: 결과 확인 포인트**

- `spec_status 분포`에서 `Available` 수가 이전보다 유의미하게 증가했는지
- `External` 카운트가 새로 등장했는지 (LINK API 분류)
- `PARTIAL` 로그가 있다면 비율 확인
- Gateway에서 추출된 spec의 operations 개수가 합리적인지

**Step 3: 추출된 스펙으로 실제 API 호출 테스트**

```bash
# 번들에서 Gateway 추출 API 하나를 골라 호출 테스트
cargo run -- call <list_id> <operation> --api-key $DATA_GO_KR_API_KEY
```

---

## 의존성 그래프

```
Task 1 (types.rs classify)
    └─> Task 7 (build_bundle.rs - classify 호출)

Task 2 (PageInfo 확장)
    └─> Task 7 (parse_openapi_page 사용)

Task 3 (summary 추출)
    └─> Task 7 (build_api_spec 사용)

Task 4 (응답 필드 파싱)
    └─> Task 7 (parse_operation_detail 사용)

Task 5 (빈 요청주소 fallback)
    └─> Task 7 (build_api_spec 사용)

Task 6 (BuildConfig 확장)
    └─> Task 7 (ajax_concurrency/ajax_delay_ms 사용)

Tasks 1-6 (병렬 가능) ──> Task 7 (통합) ──> Task 8 (검증) ──> Task 9 (E2E)
```

> **감사 반영 [S1]**: Tasks 3+5는 `build_api_spec` 동일 코드를 수정하므로 순차 실행 필수 (병렬 시 merge conflict).

Tasks 1, 2, 4, 6은 서로 독립적이므로 병렬 가능. **Task 3 → Task 5는 순차** (summary 필드 의존 + 동일 코드 수정). Task 7은 1-6 모두 완료 후 진행.
