# HTML 패턴 발견 도구 구현 계획

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 12,108개 크롤링된 HTML에서 가정 없이 모든 구조적 요소를 추출하고, 빈도 분석으로 패턴을 발견한다.

**Architecture:** 2단계 파이프라인. 1단계(`analyze_pages.rs`)는 각 HTML에서 DOM/JS/메타 신호를 기계적으로 추출하여 `page_raw_signals.json` 생성. 2단계(`summarize_signals.rs`)는 빈도 분석으로 공통 템플릿을 걸러내고 변별 신호를 도출하여 `signal_summary.json` 생성. 기존 `crawl_pages.rs`와 동일한 바이너리 패턴.

**Tech Stack:** Rust, scraper (CSS selector DOM 파싱), regex (JS 변수/AJAX 추출), serde_json (출력), rayon (병렬 처리)

**설계 문서:** `docs/specs/2026-04-02-html-pattern-discovery-design.md`

**레퍼런스 코드:** `src/bin/crawl_pages.rs` (바이너리 구조), `src/bin/html_survey.rs` (HTML 분석 패턴)

---

## Task 1: 프로젝트 설정 — Cargo.toml + 빈 바이너리

**Files:**
- Modify: `Cargo.toml:49` (bin 엔트리 추가)
- Create: `src/bin/analyze_pages.rs`
- Create: `src/bin/summarize_signals.rs`

**Step 1: Cargo.toml에 두 바이너리 등록**

`Cargo.toml`의 `[[bin]]` 섹션 끝(49행 이후)에 추가:

```toml
[[bin]]
name = "analyze-pages"
path = "src/bin/analyze_pages.rs"

[[bin]]
name = "summarize-signals"
path = "src/bin/summarize_signals.rs"
```

**Step 2: 빈 바이너리 스켈레톤 작성**

`src/bin/analyze_pages.rs`:
```rust
//! HTML 패턴 발견 1단계: 크롤링된 HTML에서 가정 없이 모든 구조적 요소를 추출한다.
//! Usage: cargo run --bin analyze-pages [--pages-dir data/pages] [--output data/page_raw_signals.json]

fn main() {
    eprintln!("analyze-pages: not yet implemented");
}
```

`src/bin/summarize_signals.rs`:
```rust
//! HTML 패턴 발견 2단계: page_raw_signals.json에서 빈도 분석으로 패턴을 발견한다.
//! Usage: cargo run --bin summarize-signals [--input data/page_raw_signals.json] [--output data/signal_summary.json]

fn main() {
    eprintln!("summarize-signals: not yet implemented");
}
```

**Step 3: 빌드 확인**

Run: `cargo check --bin analyze-pages --bin summarize-signals`
Expected: 성공 (warning만 허용)

**Step 4: 커밋**

```bash
git add Cargo.toml src/bin/analyze_pages.rs src/bin/summarize_signals.rs
git commit -m "chore: analyze-pages, summarize-signals 바이너리 스켈레톤 등록"
```

---

## Task 2: 신호 데이터 구조 정의

**Files:**
- Modify: `src/bin/analyze_pages.rs`

**Step 1: 테스트 — PageSignals 직렬화 라운드트립**

`src/bin/analyze_pages.rs` 하단에 테스트 모듈 추가:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_signals_json_roundtrip() {
        let mut signals = PageSignals::default();
        signals.file_size_bytes = 168000;
        signals.tag_counts.insert("table".into(), 6);
        signals.ids.push("publicDataDetailPk".into());
        signals.js_vars.insert(
            "tyDetailCode".into(),
            JsVarInfo { value_type: "string".into(), size: 6, value_preview: Some("PRDE02".into()) },
        );

        let json = serde_json::to_string(&signals).unwrap();
        let parsed: PageSignals = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.file_size_bytes, 168000);
        assert_eq!(parsed.tag_counts["table"], 6);
        assert_eq!(parsed.ids[0], "publicDataDetailPk");
        assert_eq!(parsed.js_vars["tyDetailCode"].value_preview.as_deref(), Some("PRDE02"));
    }
}
```

**Step 2: 테스트 실패 확인**

Run: `cargo test --bin analyze-pages`
Expected: FAIL (PageSignals 등 타입 미정의)

**Step 3: 데이터 구조 구현**

`src/bin/analyze_pages.rs` 상단에 구조체 정의:

```rust
use std::collections::{BTreeMap, HashMap};
use serde::{Deserialize, Serialize};

/// 단일 HTML 파일에서 추출한 모든 구조적 신호
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PageSignals {
    file_size_bytes: usize,

    // A) DOM 요소
    tag_counts: BTreeMap<String, usize>,
    ids: Vec<String>,
    names: Vec<String>,
    classes: Vec<String>,
    data_attrs: BTreeMap<String, Vec<String>>,  // data-xxx → [values]
    th_texts: Vec<String>,
    options: Vec<OptionSignal>,

    // B) JavaScript
    js_vars: BTreeMap<String, JsVarInfo>,
    ajax_urls: Vec<String>,
    json_ld_types: Vec<String>,
    script_types: Vec<String>,

    // C) 메타
    meta_tags: Vec<MetaSignal>,
    link_rels: Vec<String>,
    title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsVarInfo {
    value_type: String,   // "string", "object", "number", "empty", "undefined"
    size: usize,          // 값의 바이트 크기
    value_preview: Option<String>,  // 짧은 값(100자 이하)은 그대로, 긴 값은 None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OptionSignal {
    select_id: String,
    count: usize,
    values: Vec<String>,
    texts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaSignal {
    attr_type: String,    // "name", "property", "http-equiv"
    key: String,          // name/property/http-equiv의 값
    content: String,
}
```

**Step 4: 테스트 통과 확인**

Run: `cargo test --bin analyze-pages`
Expected: PASS

**Step 5: 커밋**

```bash
git add src/bin/analyze_pages.rs
git commit -m "feat: analyze-pages 신호 데이터 구조 정의 + 직렬화 테스트"
```

---

## Task 3: DOM 요소 추출기

**Files:**
- Modify: `src/bin/analyze_pages.rs`

**Step 1: 테스트 — 간단한 HTML에서 DOM 신호 추출**

```rust
#[test]
fn test_extract_dom_signals() {
    let html = r#"<html><head><title>테스트 API</title></head><body>
        <input type="hidden" id="publicDataDetailPk" name="pk" value="uddi:xxx">
        <input type="hidden" id="publicDataPk" value="15001234">
        <select id="open_api_detail_select">
            <option value="">선택</option>
            <option value="37400">조회 서비스</option>
            <option value="37401">등록 서비스</option>
        </select>
        <table class="dataset-table">
            <tr><th>항목명</th><th>타입</th><th>설명</th></tr>
            <tr data-paramtr-nm="serviceKey"><td>serviceKey</td><td>string</td><td>키</td></tr>
        </table>
        <div class="api-data-bx" data-code="API"></div>
    </body></html>"#;

    let signals = extract_signals(html);

    assert_eq!(signals.title, "테스트 API");
    assert!(signals.ids.contains(&"publicDataDetailPk".into()));
    assert!(signals.ids.contains(&"publicDataPk".into()));
    assert!(signals.ids.contains(&"open_api_detail_select".into()));
    assert!(signals.names.contains(&"pk".into()));
    assert!(signals.classes.contains(&"dataset-table".into()));
    assert!(signals.classes.contains(&"api-data-bx".into()));
    assert!(signals.th_texts.contains(&"항목명".into()));
    assert!(signals.th_texts.contains(&"타입".into()));
    assert!(signals.data_attrs.contains_key("data-paramtr-nm"));
    assert!(signals.data_attrs.contains_key("data-code"));
    assert_eq!(signals.options.len(), 1);
    assert_eq!(signals.options[0].select_id, "open_api_detail_select");
    assert_eq!(signals.options[0].count, 2); // 빈 value 제외
    assert!(*signals.tag_counts.get("table").unwrap_or(&0) >= 1);
    assert!(*signals.tag_counts.get("select").unwrap_or(&0) >= 1);
}
```

**Step 2: 테스트 실패 확인**

Run: `cargo test --bin analyze-pages test_extract_dom_signals`
Expected: FAIL (extract_signals 미정의)

**Step 3: DOM 추출 구현**

```rust
use scraper::{Html, Selector, Node};
use std::collections::HashSet;

/// HTML에서 모든 구조적 신호를 추출한다.
fn extract_signals(html: &str) -> PageSignals {
    let mut signals = PageSignals::default();
    signals.file_size_bytes = html.len();

    let document = Html::parse_document(html);

    // title
    if let Ok(sel) = Selector::parse("title") {
        if let Some(el) = document.select(&sel).next() {
            signals.title = el.text().collect::<String>().trim().to_string();
        }
    }

    // 태그 카운트 + 속성 수집
    let mut id_set = HashSet::new();
    let mut name_set = HashSet::new();
    let mut class_set = HashSet::new();

    for node in document.tree.nodes() {
        if let Node::Element(el) = node.value() {
            // tag count
            let tag = el.name().to_string();
            *signals.tag_counts.entry(tag.clone()).or_insert(0) += 1;

            // id
            if let Some(id) = el.attr("id") {
                if !id.is_empty() && id_set.insert(id.to_string()) {
                    signals.ids.push(id.to_string());
                }
            }

            // name
            if let Some(name) = el.attr("name") {
                if !name.is_empty() && name_set.insert(name.to_string()) {
                    signals.names.push(name.to_string());
                }
            }

            // class
            if let Some(classes) = el.attr("class") {
                for cls in classes.split_whitespace() {
                    if !cls.is_empty() && class_set.insert(cls.to_string()) {
                        signals.classes.push(cls.to_string());
                    }
                }
            }

            // data-* 속성
            for (key, val) in el.attrs() {
                if key.starts_with("data-") {
                    signals.data_attrs
                        .entry(key.to_string())
                        .or_default()
                        .push(val.to_string());
                }
            }
        }
    }

    // th 텍스트
    if let Ok(sel) = Selector::parse("th") {
        let mut th_set = HashSet::new();
        for el in document.select(&sel) {
            let text = el.text().collect::<String>().trim().to_string();
            if !text.is_empty() && th_set.insert(text.clone()) {
                signals.th_texts.push(text);
            }
        }
    }

    // select > option
    if let Ok(sel) = Selector::parse("select") {
        let option_sel = Selector::parse("option").unwrap();
        for select_el in document.select(&sel) {
            let select_id = select_el.value().attr("id").unwrap_or("").to_string();
            let mut values = Vec::new();
            let mut texts = Vec::new();
            for opt in select_el.select(&option_sel) {
                let val = opt.value().attr("value").unwrap_or("").to_string();
                if val.is_empty() { continue; }
                let text = opt.text().collect::<String>().trim().to_string();
                values.push(val);
                texts.push(text);
            }
            if !values.is_empty() || !select_id.is_empty() {
                let count = values.len();
                signals.options.push(OptionSignal { select_id, count, values, texts });
            }
        }
    }

    signals
}
```

**Step 4: 테스트 통과 확인**

Run: `cargo test --bin analyze-pages test_extract_dom_signals`
Expected: PASS

**Step 5: 커밋**

```bash
git add src/bin/analyze_pages.rs
git commit -m "feat: analyze-pages DOM 요소 추출기 구현"
```

---

## Task 4: JavaScript 신호 추출기

**Files:**
- Modify: `src/bin/analyze_pages.rs`

**Step 1: 테스트 — JS 변수, AJAX URL, JSON-LD 추출**

```rust
#[test]
fn test_extract_js_signals() {
    let html = r#"<html><head>
        <script type="application/ld+json">{"@type": "Dataset", "name": "test"}</script>
        <script type="text/javascript">
            var swaggerJson = `{"swagger":"2.0","paths":{}}`;
            var swaggerUrl = '';
            var publicDataPk = '15001234';
            var tyDetailCode = 'PRDE02';
            let someNumber = 42;
            const oprtinUrl = "http://apis.data.go.kr/test/api";
            $.ajax({ url: '/tcs/dss/selectApiDetailFunction.do' });
            $.ajax({ url: '/iim/api/myPageUrl.do' });
        </script>
    </head><body></body></html>"#;

    let signals = extract_signals(html);

    // JS 변수
    assert!(signals.js_vars.contains_key("swaggerJson"));
    assert!(signals.js_vars.contains_key("swaggerUrl"));
    assert!(signals.js_vars.contains_key("publicDataPk"));
    assert!(signals.js_vars.contains_key("tyDetailCode"));
    assert!(signals.js_vars.contains_key("oprtinUrl"));
    assert_eq!(signals.js_vars["tyDetailCode"].value_preview.as_deref(), Some("PRDE02"));
    assert_eq!(signals.js_vars["tyDetailCode"].value_type, "string");
    assert!(signals.js_vars["swaggerJson"].size > 0);

    // AJAX URLs
    assert!(signals.ajax_urls.contains(&"/tcs/dss/selectApiDetailFunction.do".to_string()));
    assert!(signals.ajax_urls.contains(&"/iim/api/myPageUrl.do".to_string()));

    // JSON-LD
    assert!(signals.json_ld_types.contains(&"Dataset".to_string()));

    // script types
    assert!(signals.script_types.contains(&"application/ld+json".to_string()));
    assert!(signals.script_types.contains(&"text/javascript".to_string()));
}

#[test]
fn test_js_var_undefined_and_empty() {
    let html = r#"<html><head><script>
        var swaggerJson = `undefined`;
        var swaggerUrl = '';
    </script></head><body></body></html>"#;

    let signals = extract_signals(html);
    assert_eq!(signals.js_vars["swaggerJson"].value_type, "undefined");
    assert_eq!(signals.js_vars["swaggerUrl"].value_type, "empty");
}
```

**Step 2: 테스트 실패 확인**

Run: `cargo test --bin analyze-pages test_extract_js`
Expected: FAIL (JS 추출 미구현, extract_signals가 js_vars를 채우지 않음)

**Step 3: JS 추출 구현**

`extract_signals` 함수에 JS 추출 로직 추가:

```rust
use regex::Regex;

// extract_signals 함수 내에 추가:

fn extract_js_signals(html: &str, signals: &mut PageSignals) {
    // JS 변수 선언: var/let/const xxx = ...
    // 값 유형: backtick string, quoted string, number, object, empty
    let var_re = Regex::new(
        r#"(?:var|let|const)\s+(\w+)\s*=\s*(?:(`(?:[^`\\]|\\.)*`)|'([^']*)'|"([^"]*)"|(\d+(?:\.\d+)?)|(\{[^;]*\})|([^;,\n]+))"#
    ).unwrap();

    for caps in var_re.captures_iter(html) {
        let name = caps[1].to_string();

        let (raw_value, value_type) = if let Some(m) = caps.get(2) {
            // backtick string
            let inner = &m.as_str()[1..m.as_str().len()-1]; // strip backticks
            let vtype = if inner.trim().is_empty() {
                "empty".to_string()
            } else if inner.trim() == "undefined" {
                "undefined".to_string()
            } else if inner.trim_start().starts_with('{') {
                "object".to_string()
            } else {
                "string".to_string()
            };
            (inner.to_string(), vtype)
        } else if let Some(m) = caps.get(3) {
            // single-quoted string
            let s = m.as_str();
            let vtype = if s.is_empty() { "empty" } else { "string" };
            (s.to_string(), vtype.to_string())
        } else if let Some(m) = caps.get(4) {
            // double-quoted string
            let s = m.as_str();
            let vtype = if s.is_empty() { "empty" } else { "string" };
            (s.to_string(), vtype.to_string())
        } else if let Some(m) = caps.get(5) {
            (m.as_str().to_string(), "number".to_string())
        } else if let Some(m) = caps.get(6) {
            (m.as_str().to_string(), "object".to_string())
        } else if let Some(m) = caps.get(7) {
            let s = m.as_str().trim();
            (s.to_string(), "other".to_string())
        } else {
            continue;
        };

        let size = raw_value.len();
        let value_preview = if size <= 100 {
            Some(raw_value.clone())
        } else {
            None
        };

        signals.js_vars.insert(name, JsVarInfo { value_type, size, value_preview });
    }

    // AJAX URLs: $.ajax({ url: '...' }) 또는 $.ajax({ url: "..." })
    let ajax_re = Regex::new(r#"\$\.ajax\s*\(\s*\{[^}]*url\s*:\s*['"]([^'"]+)['"]"#).unwrap();
    let mut ajax_set = HashSet::new();
    for caps in ajax_re.captures_iter(html) {
        let url = caps[1].to_string();
        if ajax_set.insert(url.clone()) {
            signals.ajax_urls.push(url);
        }
    }

    // fetch() 호출도 추가
    let fetch_re = Regex::new(r#"fetch\s*\(\s*['"]([^'"]+)['"]"#).unwrap();
    for caps in fetch_re.captures_iter(html) {
        let url = caps[1].to_string();
        if ajax_set.insert(url.clone()) {
            signals.ajax_urls.push(url);
        }
    }
}
```

**Step 4: JSON-LD + script type 추출**

`extract_signals` 함수에 추가:

```rust
fn extract_script_signals(document: &Html, html: &str, signals: &mut PageSignals) {
    if let Ok(sel) = Selector::parse("script") {
        let mut type_set = HashSet::new();
        for el in document.select(&sel) {
            // script type
            if let Some(stype) = el.value().attr("type") {
                if !stype.is_empty() && type_set.insert(stype.to_string()) {
                    signals.script_types.push(stype.to_string());
                }

                // JSON-LD 파싱
                if stype == "application/ld+json" {
                    let content = el.text().collect::<String>();
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(t) = json.get("@type").and_then(|v| v.as_str()) {
                            signals.json_ld_types.push(t.to_string());
                        }
                    }
                }
            }
        }
    }
}
```

`extract_signals`에서 호출:
```rust
extract_js_signals(html, &mut signals);
extract_script_signals(&document, html, &mut signals);
```

**Step 5: 테스트 통과 확인**

Run: `cargo test --bin analyze-pages test_extract_js`
Expected: PASS

**Step 6: 커밋**

```bash
git add src/bin/analyze_pages.rs
git commit -m "feat: analyze-pages JavaScript 신호 추출기 (변수, AJAX, JSON-LD)"
```

---

## Task 5: 메타 태그 추출기

**Files:**
- Modify: `src/bin/analyze_pages.rs`

**Step 1: 테스트 — meta, link 추출**

```rust
#[test]
fn test_extract_meta_signals() {
    let html = r#"<html><head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width">
        <meta property="og:title" content="공기질 API">
        <meta http-equiv="X-UA-Compatible" content="IE=edge">
        <link rel="stylesheet" href="/css/app.min.css">
        <link rel="canonical" href="https://www.data.go.kr/data/15095367/openapi.do">
    </head><body></body></html>"#;

    let signals = extract_signals(html);

    assert!(signals.meta_tags.iter().any(|m| m.key == "viewport"));
    assert!(signals.meta_tags.iter().any(|m| m.key == "og:title" && m.content == "공기질 API"));
    assert!(signals.meta_tags.iter().any(|m| m.attr_type == "http-equiv" && m.key == "X-UA-Compatible"));
    assert!(signals.link_rels.contains(&"stylesheet".to_string()));
    assert!(signals.link_rels.contains(&"canonical".to_string()));
}
```

**Step 2: 테스트 실패 확인**

Run: `cargo test --bin analyze-pages test_extract_meta_signals`
Expected: FAIL

**Step 3: 메타 추출 구현**

```rust
fn extract_meta_signals(document: &Html, signals: &mut PageSignals) {
    // meta 태그
    if let Ok(sel) = Selector::parse("meta") {
        for el in document.select(&sel) {
            let (attr_type, key) = if let Some(name) = el.value().attr("name") {
                ("name".to_string(), name.to_string())
            } else if let Some(prop) = el.value().attr("property") {
                ("property".to_string(), prop.to_string())
            } else if let Some(he) = el.value().attr("http-equiv") {
                ("http-equiv".to_string(), he.to_string())
            } else {
                continue;
            };
            let content = el.value().attr("content").unwrap_or("").to_string();
            signals.meta_tags.push(MetaSignal { attr_type, key, content });
        }
    }

    // link rel
    if let Ok(sel) = Selector::parse("link") {
        let mut rel_set = HashSet::new();
        for el in document.select(&sel) {
            if let Some(rel) = el.value().attr("rel") {
                if !rel.is_empty() && rel_set.insert(rel.to_string()) {
                    signals.link_rels.push(rel.to_string());
                }
            }
        }
    }
}
```

**Step 4: 테스트 통과 확인**

Run: `cargo test --bin analyze-pages test_extract_meta_signals`
Expected: PASS

**Step 5: 커밋**

```bash
git add src/bin/analyze_pages.rs
git commit -m "feat: analyze-pages 메타 태그 추출기 (meta, link)"
```

---

## Task 6: 실제 HTML 파일 통합 테스트

**Files:**
- Modify: `src/bin/analyze_pages.rs`

**Step 1: 실제 HTML 파일로 테스트**

`data/pages/` 디렉토리가 존재할 때만 실행되는 통합 테스트:

```rust
#[test]
fn test_extract_from_real_html() {
    // 실제 크롤링 데이터가 있을 때만 실행
    let pages_dir = std::path::Path::new("data/pages");
    if !pages_dir.exists() {
        eprintln!("SKIP: data/pages/ not found");
        return;
    }

    // 작은 파일 (에러 페이지 가능)
    let small_files: Vec<_> = std::fs::read_dir(pages_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.metadata().map(|m| m.len() < 5000).unwrap_or(false))
        .take(2)
        .collect();

    // 일반 파일
    let normal_files: Vec<_> = std::fs::read_dir(pages_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.metadata().map(|m| m.len() > 100_000 && m.len() < 200_000).unwrap_or(false))
        .take(3)
        .collect();

    for entry in small_files.iter().chain(normal_files.iter()) {
        let html = std::fs::read_to_string(entry.path()).unwrap();
        let signals = extract_signals(&html);

        // 기본 검증: 파닉 없이 실행, 크기 일치
        assert_eq!(signals.file_size_bytes, html.len());
        // title은 항상 있어야 함 (에러 페이지도 title 있음)
        assert!(!signals.title.is_empty(), "title empty for {:?}", entry.path());

        eprintln!(
            "  {} — tags:{}, ids:{}, js_vars:{}, classes:{}",
            entry.path().file_name().unwrap().to_string_lossy(),
            signals.tag_counts.len(),
            signals.ids.len(),
            signals.js_vars.len(),
            signals.classes.len(),
        );
    }
}
```

**Step 2: 테스트 실행**

Run: `cargo test --bin analyze-pages test_extract_from_real_html -- --nocapture`
Expected: PASS (파닉 없이, 각 파일에서 신호 추출 로그 출력)

**Step 3: 발견된 이슈 수정 (있으면)**

실제 HTML을 파싱하면 예상 못한 엣지 케이스가 나올 수 있다.
regex가 매칭 실패하거나, 특이한 DOM 구조에서 패닉이 발생하면 여기서 수정.

**Step 4: 커밋**

```bash
git add src/bin/analyze_pages.rs
git commit -m "test: analyze-pages 실제 HTML 통합 테스트"
```

---

## Task 7: CLI + 전체 실행 엔진 (analyze_pages.rs)

**Files:**
- Modify: `src/bin/analyze_pages.rs`

**Step 1: CLI 인자 파싱 + main 함수 구현**

`crawl_pages.rs`의 인자 파싱 패턴을 따라:

```rust
use std::path::PathBuf;
use std::time::Instant;

struct Config {
    pages_dir: PathBuf,
    output_path: PathBuf,
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();

    let pages_dir = args.iter()
        .position(|a| a == "--pages-dir")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/pages"));

    let output_path = args.iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/page_raw_signals.json"));

    Config { pages_dir, output_path }
}

fn main() -> anyhow::Result<()> {
    let config = parse_args();
    let start = Instant::now();

    // Step 1: HTML 파일 목록 수집
    eprintln!("=== Step 1/2: HTML 파일 스캔 ===");
    let mut html_files: Vec<_> = std::fs::read_dir(&config.pages_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|ext| ext == "html").unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    html_files.sort();
    let total = html_files.len();
    eprintln!("  {} HTML 파일 발견", total);

    // Step 2: 각 파일 분석
    eprintln!("\n=== Step 2/2: 신호 추출 ===");
    let mut results: BTreeMap<String, PageSignals> = BTreeMap::new();

    for (i, path) in html_files.iter().enumerate() {
        let list_id = path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let html = std::fs::read_to_string(path)?;
        let signals = extract_signals(&html);

        if (i + 1) % 1000 == 0 || i + 1 == total {
            eprintln!(
                "  [{}/{}] {} — tags:{}, ids:{}, js_vars:{}",
                i + 1, total, list_id,
                signals.tag_counts.len(),
                signals.ids.len(),
                signals.js_vars.len(),
            );
        }

        results.insert(list_id, signals);
    }

    // Step 3: JSON 출력
    let json = serde_json::to_string_pretty(&results)?;
    std::fs::write(&config.output_path, &json)?;

    let elapsed = start.elapsed();
    eprintln!(
        "\n=== 완료: {}개 분석, {}초 경과 → {} ===",
        results.len(),
        elapsed.as_secs(),
        config.output_path.display()
    );

    Ok(())
}
```

**Step 2: 소규모 실행 테스트**

`data/pages/`에서 처음 10개 파일로 테스트:

Run: `ls data/pages/*.html | head -10 | while read f; do cp "$f" /tmp/test_pages/; done && cargo run --bin analyze-pages -- --pages-dir /tmp/test_pages --output /tmp/test_signals.json`

Expected: JSON 파일 생성, 10개 엔트리 포함

검증: `jq 'keys | length' /tmp/test_signals.json` → 10
검증: `jq '.[keys[0]] | keys' /tmp/test_signals.json` → 모든 필드 존재 확인

**Step 3: 전체 12,108개 실행**

Run: `cargo run --release --bin analyze-pages`

Expected: `data/page_raw_signals.json` 생성 (수 분 내 완료)

**Step 4: 커밋**

```bash
git add src/bin/analyze_pages.rs
git commit -m "feat: analyze-pages CLI + 전체 실행 엔진"
```

---

## Task 8: 빈도 분석기 데이터 구조 (summarize_signals.rs)

**Files:**
- Modify: `src/bin/summarize_signals.rs`

**Step 1: 테스트 — 빈도 집계 라운드트립**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_json_roundtrip() {
        let mut summary = SignalSummary::default();
        summary.total_files = 3;
        summary.element_frequencies.ids.insert("publicDataDetailPk".into(), 3);
        summary.element_frequencies.ids.insert("special_id".into(), 1);

        let json = serde_json::to_string(&summary).unwrap();
        let parsed: SignalSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_files, 3);
        assert_eq!(parsed.element_frequencies.ids["publicDataDetailPk"], 3);
    }
}
```

**Step 2: 테스트 실패 확인**

Run: `cargo test --bin summarize-signals`
Expected: FAIL

**Step 3: 구조체 정의**

```rust
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct SignalSummary {
    total_files: usize,

    /// 각 신호 종류별 빈도: 요소값 → 몇 개 파일에서 출현했는가
    element_frequencies: ElementFrequencies,

    /// 변별력 있는 신호: 전체 파일 수와 다르고, 1개 이상인 것
    discriminating_signals: Vec<DiscriminatingSignal>,

    /// 변별 신호 조합으로 그룹핑한 클러스터
    clusters: Vec<Cluster>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ElementFrequencies {
    ids: BTreeMap<String, usize>,
    names: BTreeMap<String, usize>,
    classes: BTreeMap<String, usize>,
    data_attr_keys: BTreeMap<String, usize>,
    th_texts: BTreeMap<String, usize>,
    js_var_names: BTreeMap<String, usize>,
    js_var_types: BTreeMap<String, usize>,  // "swaggerJson:object" → count
    ajax_urls: BTreeMap<String, usize>,
    json_ld_types: BTreeMap<String, usize>,
    script_types: BTreeMap<String, usize>,
    meta_keys: BTreeMap<String, usize>,
    link_rels: BTreeMap<String, usize>,
    select_ids: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscriminatingSignal {
    category: String,   // "ids", "js_vars", "th_texts", ...
    signal: String,     // 요소명 또는 "name:type" 조합
    count: usize,
    percentage: f64,    // count / total_files * 100
}

#[derive(Debug, Serialize, Deserialize)]
struct Cluster {
    name: String,
    defining_signals: Vec<String>,  // 이 클러스터를 정의하는 신호 조합
    count: usize,
    sample_files: Vec<String>,      // 대표 파일 3개
}
```

**Step 4: 테스트 통과 확인**

Run: `cargo test --bin summarize-signals`
Expected: PASS

**Step 5: 커밋**

```bash
git add src/bin/summarize_signals.rs
git commit -m "feat: summarize-signals 빈도 분석 데이터 구조 정의"
```

---

## Task 9: 빈도 집계 로직

**Files:**
- Modify: `src/bin/summarize_signals.rs`

**Step 1: 테스트 — 3개 PageSignals에서 빈도 집계**

```rust
#[test]
fn test_count_frequencies() {
    // 3개 가상 PageSignals
    let mut all: BTreeMap<String, PageSignals> = BTreeMap::new();

    let mut s1 = PageSignals::default();
    s1.ids = vec!["pk".into(), "select".into()];
    s1.js_vars.insert("tyDetailCode".into(), JsVarInfo { value_type: "string".into(), size: 6, value_preview: Some("PRDE02".into()) });
    s1.js_vars.insert("swaggerJson".into(), JsVarInfo { value_type: "object".into(), size: 5000, value_preview: None });

    let mut s2 = PageSignals::default();
    s2.ids = vec!["pk".into()];
    s2.js_vars.insert("tyDetailCode".into(), JsVarInfo { value_type: "string".into(), size: 6, value_preview: Some("PRDE04".into()) });

    let mut s3 = PageSignals::default();
    s3.ids = vec!["pk".into(), "select".into()];
    s3.js_vars.insert("tyDetailCode".into(), JsVarInfo { value_type: "string".into(), size: 6, value_preview: Some("PRDE02".into()) });

    all.insert("1".into(), s1);
    all.insert("2".into(), s2);
    all.insert("3".into(), s3);

    let freqs = count_frequencies(&all);

    assert_eq!(freqs.ids["pk"], 3);     // 3/3 전부
    assert_eq!(freqs.ids["select"], 2); // 2/3
    assert_eq!(freqs.js_var_names["tyDetailCode"], 3);
    assert_eq!(freqs.js_var_names["swaggerJson"], 1);
    // js_var_types: "tyDetailCode:string" 3, "swaggerJson:object" 1
    assert_eq!(freqs.js_var_types["tyDetailCode:string"], 3);
    assert_eq!(freqs.js_var_types["swaggerJson:object"], 1);
}
```

**Step 2: 테스트 실패 확인**

Run: `cargo test --bin summarize-signals test_count_frequencies`
Expected: FAIL

**Step 3: count_frequencies 구현**

```rust
// PageSignals, JsVarInfo 등은 analyze_pages.rs와 동일하게 정의 (또는 공유 모듈)
// 여기서는 page_raw_signals.json을 읽어서 역직렬화하므로 동일 구조체 필요

fn count_frequencies(all: &BTreeMap<String, PageSignals>) -> ElementFrequencies {
    let mut freqs = ElementFrequencies::default();

    for signals in all.values() {
        // ids: 각 파일에서 고유 id 출현 여부 (파일당 1회만 카운트)
        for id in &signals.ids {
            *freqs.ids.entry(id.clone()).or_insert(0) += 1;
        }
        for name in &signals.names {
            *freqs.names.entry(name.clone()).or_insert(0) += 1;
        }
        for cls in &signals.classes {
            *freqs.classes.entry(cls.clone()).or_insert(0) += 1;
        }
        for key in signals.data_attrs.keys() {
            *freqs.data_attr_keys.entry(key.clone()).or_insert(0) += 1;
        }
        for th in &signals.th_texts {
            *freqs.th_texts.entry(th.clone()).or_insert(0) += 1;
        }
        for (name, info) in &signals.js_vars {
            *freqs.js_var_names.entry(name.clone()).or_insert(0) += 1;
            let type_key = format!("{}:{}", name, info.value_type);
            *freqs.js_var_types.entry(type_key).or_insert(0) += 1;
        }
        for url in &signals.ajax_urls {
            *freqs.ajax_urls.entry(url.clone()).or_insert(0) += 1;
        }
        for t in &signals.json_ld_types {
            *freqs.json_ld_types.entry(t.clone()).or_insert(0) += 1;
        }
        for t in &signals.script_types {
            *freqs.script_types.entry(t.clone()).or_insert(0) += 1;
        }
        for m in &signals.meta_tags {
            *freqs.meta_keys.entry(m.key.clone()).or_insert(0) += 1;
        }
        for rel in &signals.link_rels {
            *freqs.link_rels.entry(rel.clone()).or_insert(0) += 1;
        }
        for opt in &signals.options {
            if !opt.select_id.is_empty() {
                *freqs.select_ids.entry(opt.select_id.clone()).or_insert(0) += 1;
            }
        }
    }

    freqs
}
```

**Step 4: 테스트 통과 확인**

Run: `cargo test --bin summarize-signals test_count_frequencies`
Expected: PASS

**Step 5: 커밋**

```bash
git add src/bin/summarize_signals.rs
git commit -m "feat: summarize-signals 빈도 집계 로직"
```

---

## Task 10: 변별 신호 추출 + 클러스터링

**Files:**
- Modify: `src/bin/summarize_signals.rs`

**Step 1: 테스트 — 변별 신호 필터링**

```rust
#[test]
fn test_find_discriminating_signals() {
    let mut freqs = ElementFrequencies::default();
    freqs.ids.insert("common_id".into(), 100);    // 전체
    freqs.ids.insert("rare_id".into(), 5);         // 일부
    freqs.ids.insert("unique_id".into(), 1);       // 이상치

    let discriminating = find_discriminating_signals(&freqs, 100);

    // common_id는 전체에 있으므로 제외
    assert!(!discriminating.iter().any(|d| d.signal == "common_id"));
    // rare_id, unique_id는 변별 신호
    assert!(discriminating.iter().any(|d| d.signal == "rare_id"));
    assert!(discriminating.iter().any(|d| d.signal == "unique_id"));
}
```

**Step 2: 테스트 실패 확인**

Run: `cargo test --bin summarize-signals test_find_discriminating`
Expected: FAIL

**Step 3: 변별 신호 추출 구현**

```rust
fn find_discriminating_signals(freqs: &ElementFrequencies, total: usize) -> Vec<DiscriminatingSignal> {
    let mut signals = Vec::new();
    let threshold = total;  // total과 같으면 공통 → 제외

    let categories: Vec<(&str, &BTreeMap<String, usize>)> = vec![
        ("ids", &freqs.ids),
        ("names", &freqs.names),
        ("classes", &freqs.classes),
        ("data_attr_keys", &freqs.data_attr_keys),
        ("th_texts", &freqs.th_texts),
        ("js_var_names", &freqs.js_var_names),
        ("js_var_types", &freqs.js_var_types),
        ("ajax_urls", &freqs.ajax_urls),
        ("json_ld_types", &freqs.json_ld_types),
        ("script_types", &freqs.script_types),
        ("meta_keys", &freqs.meta_keys),
        ("link_rels", &freqs.link_rels),
        ("select_ids", &freqs.select_ids),
    ];

    for (category, map) in categories {
        for (signal, &count) in map {
            if count < threshold && count > 0 {
                signals.push(DiscriminatingSignal {
                    category: category.to_string(),
                    signal: signal.clone(),
                    count,
                    percentage: (count as f64 / total as f64) * 100.0,
                });
            }
        }
    }

    // count 내림차순 정렬
    signals.sort_by(|a, b| b.count.cmp(&a.count));
    signals
}
```

**Step 4: 간단한 클러스터링 — js_var_types 기반 그룹핑**

핵심 변별 신호(tyDetailCode 값 + swaggerJson 유무)의 조합으로 클러스터 생성:

```rust
fn build_clusters(
    all: &BTreeMap<String, PageSignals>,
    discriminating: &[DiscriminatingSignal],
) -> Vec<Cluster> {
    // 각 파일의 핵심 변별 신호 조합을 fingerprint로 만들기
    // 상위 변별 신호(가장 많은 파일에 영향을 주는 것)를 기준으로 함
    let key_signals: Vec<&str> = discriminating.iter()
        .filter(|d| d.category == "js_var_types" && d.count > 10)
        .map(|d| d.signal.as_str())
        .take(20)  // 상위 20개로 제한
        .collect();

    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (list_id, signals) in all {
        let mut fingerprint_parts: Vec<String> = Vec::new();

        for ks in &key_signals {
            // "varname:type" 형식에서 varname과 type 추출
            let parts: Vec<&str> = ks.splitn(2, ':').collect();
            if parts.len() == 2 {
                let has = signals.js_vars.get(parts[0])
                    .map(|info| info.value_type == parts[1])
                    .unwrap_or(false);
                if has {
                    fingerprint_parts.push(ks.to_string());
                }
            }
        }

        let fingerprint = if fingerprint_parts.is_empty() {
            "_no_js_vars_".to_string()
        } else {
            fingerprint_parts.join("+")
        };

        groups.entry(fingerprint).or_default().push(list_id.clone());
    }

    // 그룹을 Cluster로 변환
    let mut clusters: Vec<Cluster> = groups.into_iter()
        .map(|(fingerprint, mut files)| {
            let count = files.len();
            files.sort();
            let sample_files = files.iter().take(3).cloned().collect();
            Cluster {
                name: format!("cluster_{}", fingerprint.replace('+', "_").chars().take(50).collect::<String>()),
                defining_signals: fingerprint.split('+').map(|s| s.to_string()).collect(),
                count,
                sample_files,
            }
        })
        .collect();

    clusters.sort_by(|a, b| b.count.cmp(&a.count));
    clusters
}
```

**Step 5: 테스트 통과 확인**

Run: `cargo test --bin summarize-signals`
Expected: PASS

**Step 6: 커밋**

```bash
git add src/bin/summarize_signals.rs
git commit -m "feat: summarize-signals 변별 신호 추출 + 클러스터링"
```

---

## Task 11: CLI + 전체 실행 엔진 (summarize_signals.rs)

**Files:**
- Modify: `src/bin/summarize_signals.rs`

**Step 1: main 함수 구현**

```rust
use std::path::PathBuf;
use std::time::Instant;

struct Config {
    input_path: PathBuf,
    output_path: PathBuf,
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();

    let input_path = args.iter()
        .position(|a| a == "--input")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/page_raw_signals.json"));

    let output_path = args.iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/signal_summary.json"));

    Config { input_path, output_path }
}

fn main() -> anyhow::Result<()> {
    let config = parse_args();
    let start = Instant::now();

    // Step 1: page_raw_signals.json 로드
    eprintln!("=== Step 1/3: 신호 데이터 로드 ===");
    let raw = std::fs::read_to_string(&config.input_path)?;
    let all: BTreeMap<String, PageSignals> = serde_json::from_str(&raw)?;
    let total = all.len();
    eprintln!("  {} 파일 로드 완료", total);

    // Step 2: 빈도 집계
    eprintln!("\n=== Step 2/3: 빈도 분석 ===");
    let freqs = count_frequencies(&all);
    eprintln!("  ids: {} 종류", freqs.ids.len());
    eprintln!("  js_var_names: {} 종류", freqs.js_var_names.len());
    eprintln!("  classes: {} 종류", freqs.classes.len());

    // Step 3: 변별 신호 + 클러스터링
    eprintln!("\n=== Step 3/3: 패턴 발견 ===");
    let discriminating = find_discriminating_signals(&freqs, total);
    eprintln!("  변별 신호: {} 개 (전체에 있는 공통 요소 제외)", discriminating.len());

    let clusters = build_clusters(&all, &discriminating);
    eprintln!("  클러스터: {} 개", clusters.len());
    for c in &clusters {
        eprintln!("    {} — {} files — {:?}", c.name, c.count, c.defining_signals);
    }

    // 출력
    let summary = SignalSummary {
        total_files: total,
        element_frequencies: freqs,
        discriminating_signals: discriminating,
        clusters,
    };

    let json = serde_json::to_string_pretty(&summary)?;
    std::fs::write(&config.output_path, &json)?;

    let elapsed = start.elapsed();
    eprintln!(
        "\n=== 완료: {}초 → {} ===",
        elapsed.as_secs(),
        config.output_path.display()
    );

    Ok(())
}
```

**Step 2: 전체 파이프라인 실행**

```bash
# 1단계: 신호 추출 (사전에 Task 7에서 완료)
# data/page_raw_signals.json 이 이미 존재해야 함

# 2단계: 빈도 분석
cargo run --release --bin summarize-signals
```

Expected: `data/signal_summary.json` 생성

검증:
```bash
jq '.total_files' data/signal_summary.json                    # → 12108
jq '.clusters | length' data/signal_summary.json               # → 몇 개 클러스터
jq '.discriminating_signals | length' data/signal_summary.json # → 변별 신호 수
jq '.clusters[] | "\(.count) \(.name)"' data/signal_summary.json  # 클러스터 분포
```

**Step 3: 커밋**

```bash
git add src/bin/summarize_signals.rs
git commit -m "feat: summarize-signals CLI + 전체 파이프라인 실행"
```

---

## Task 12: 전체 파이프라인 실행 + 결과 검증

**Files:**
- 실행만 (새 코드 없음)
- `data/page_raw_signals.json` (생성됨, .gitignore 대상)
- `data/signal_summary.json` (생성됨, .gitignore 대상)

**Step 1: .gitignore에 데이터 파일 추가**

`.gitignore`에 추가:
```
data/page_raw_signals.json
data/signal_summary.json
```

**Step 2: 1단계 실행**

```bash
cargo run --release --bin analyze-pages
```

Expected: `data/page_raw_signals.json` 생성 (12,108개 엔트리, 수 분 소요)

검증:
```bash
jq 'keys | length' data/page_raw_signals.json  # → 12108
jq '.[keys[0]] | keys' data/page_raw_signals.json  # → 모든 필드 존재
```

**Step 3: 2단계 실행**

```bash
cargo run --release --bin summarize-signals
```

Expected: `data/signal_summary.json` 생성

**Step 4: 결과 검증**

결과물을 훑어보고 기대와 일치하는지 확인:

```bash
# 클러스터 분포 확인
jq -r '.clusters[] | "\(.count)\t\(.name)\t\(.defining_signals | join(", "))"' data/signal_summary.json | sort -rn

# 이상치 신호 확인 (출현 1-10회)
jq '[.discriminating_signals[] | select(.count <= 10)]' data/signal_summary.json

# 공통 템플릿 요소 확인 (전체 12108에 출현)
jq '[.element_frequencies.ids | to_entries[] | select(.value >= 12100)] | length' data/signal_summary.json
```

**Step 5: 커밋**

```bash
git add .gitignore
git commit -m "chore: .gitignore에 분석 결과 데이터 파일 추가"
```

---

## Task 13: devlog 업데이트

**Files:**
- Modify: `docs/devlogs/current.md`

**Step 1: devlog에 작업 결과 기록**

실행 결과 (소요 시간, 출력 파일 크기, 클러스터 수, 주요 발견사항)를 기록.

**Step 2: 커밋**

```bash
git add docs/devlogs/current.md
git commit -m "docs: devlog — HTML 패턴 발견 도구 구현 + 실행 결과"
```
