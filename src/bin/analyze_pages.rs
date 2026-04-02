//! HTML 패턴 발견 1단계: 크롤링된 HTML에서 가정 없이 모든 구조적 요소를 추출한다.
//! Usage: cargo run --bin analyze-pages [--pages-dir data/pages] [--output data/page_raw_signals.json]

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use regex::Regex;
use scraper::{Html, Node, Selector};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Data structures (Task 2)
// ---------------------------------------------------------------------------

/// 단일 HTML 파일에서 추출한 모든 구조적 신호
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PageSignals {
    file_size_bytes: usize,

    // A) DOM 요소
    tag_counts: BTreeMap<String, usize>,
    ids: Vec<String>,
    names: Vec<String>,
    classes: Vec<String>,
    data_attrs: BTreeMap<String, Vec<String>>, // data-xxx -> [values]
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
    value_type: String, // "string", "object", "number", "empty", "undefined"
    size: usize,        // 값의 바이트 크기
    value_preview: Option<String>, // 짧은 값(100자 이하)은 그대로, 긴 값은 None
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
    attr_type: String, // "name", "property", "http-equiv"
    key: String,       // name/property/http-equiv의 값
    content: String,
}

// ---------------------------------------------------------------------------
// DOM extraction (Task 3)
// ---------------------------------------------------------------------------

/// HTML에서 모든 구조적 신호를 추출한다.
fn extract_signals(html: &str) -> PageSignals {
    let mut signals = PageSignals {
        file_size_bytes: html.len(),
        ..Default::default()
    };

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
            *signals.tag_counts.entry(tag).or_insert(0) += 1;

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
                    signals
                        .data_attrs
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
                if val.is_empty() {
                    continue;
                }
                let text = opt.text().collect::<String>().trim().to_string();
                values.push(val);
                texts.push(text);
            }
            if !values.is_empty() || !select_id.is_empty() {
                let count = values.len();
                signals.options.push(OptionSignal {
                    select_id,
                    count,
                    values,
                    texts,
                });
            }
        }
    }

    // B) JavaScript 신호
    extract_js_signals(html, &mut signals);
    extract_script_signals(&document, &mut signals);

    // C) 메타 신호
    extract_meta_signals(&document, &mut signals);

    signals
}

// ---------------------------------------------------------------------------
// JS extraction (Task 4)
// ---------------------------------------------------------------------------

/// 인라인 JavaScript에서 변수 선언과 AJAX URL을 추출한다.
fn extract_js_signals(html: &str, signals: &mut PageSignals) {
    // JS 변수 선언: var/let/const xxx = ...
    // 값 유형: backtick string, quoted string, number, object, empty
    let var_re = Regex::new(
        r#"(?:var|let|const)\s+(\w+)\s*=\s*(?:(`(?:[^`\\]|\\.)*`)|'([^']*)'|"([^"]*)"|(\d+(?:\.\d+)?)|(\{[^;]*\})|([^;,\n]+))"#,
    )
    .unwrap();

    for caps in var_re.captures_iter(html) {
        let name = caps[1].to_string();

        let (raw_value, value_type) = if let Some(m) = caps.get(2) {
            // backtick string
            let s = m.as_str();
            let inner = &s[1..s.len() - 1]; // strip backticks
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
            // number
            (m.as_str().to_string(), "number".to_string())
        } else if let Some(m) = caps.get(6) {
            // object (curly braces)
            (m.as_str().to_string(), "object".to_string())
        } else if let Some(m) = caps.get(7) {
            // other
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

        signals.js_vars.insert(
            name,
            JsVarInfo {
                value_type,
                size,
                value_preview,
            },
        );
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

/// script 태그에서 JSON-LD 타입과 script type 속성을 추출한다.
fn extract_script_signals(document: &Html, signals: &mut PageSignals) {
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

// ---------------------------------------------------------------------------
// Meta extraction (Task 5)
// ---------------------------------------------------------------------------

/// meta 태그와 link rel을 추출한다.
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
            signals.meta_tags.push(MetaSignal {
                attr_type,
                key,
                content,
            });
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

// ---------------------------------------------------------------------------
// CLI + main (Task 7)
// ---------------------------------------------------------------------------

struct Config {
    pages_dir: PathBuf,
    output_path: PathBuf,
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();

    let pages_dir = args
        .iter()
        .position(|a| a == "--pages-dir")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/pages"));

    let output_path = args
        .iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/page_raw_signals.json"));

    Config {
        pages_dir,
        output_path,
    }
}

fn main() -> anyhow::Result<()> {
    let config = parse_args();
    let start = Instant::now();

    // Step 1: HTML 파일 목록 수집
    eprintln!("=== Step 1/2: HTML 파일 스캔 ===");
    let mut html_files: Vec<_> = std::fs::read_dir(&config.pages_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "html")
                .unwrap_or(false)
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
        let list_id = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let html = std::fs::read_to_string(path)?;
        let signals = extract_signals(&html);

        if (i + 1) % 1000 == 0 || i + 1 == total {
            eprintln!(
                "  [{}/{}] {} — tags:{}, ids:{}, js_vars:{}",
                i + 1,
                total,
                list_id,
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
        "\n=== 완료: {}개 분석, {}초 경과 -> {} ===",
        results.len(),
        elapsed.as_secs(),
        config.output_path.display()
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests (Task 6)
// ---------------------------------------------------------------------------

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
            JsVarInfo {
                value_type: "string".into(),
                size: 6,
                value_preview: Some("PRDE02".into()),
            },
        );

        let json = serde_json::to_string(&signals).unwrap();
        let parsed: PageSignals = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.file_size_bytes, 168000);
        assert_eq!(parsed.tag_counts["table"], 6);
        assert_eq!(parsed.ids[0], "publicDataDetailPk");
        assert_eq!(
            parsed.js_vars["tyDetailCode"].value_preview.as_deref(),
            Some("PRDE02")
        );
    }

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
        assert_eq!(
            signals.js_vars["tyDetailCode"].value_preview.as_deref(),
            Some("PRDE02")
        );
        assert_eq!(signals.js_vars["tyDetailCode"].value_type, "string");
        assert!(signals.js_vars["swaggerJson"].size > 0);

        // AJAX URLs
        assert!(signals
            .ajax_urls
            .contains(&"/tcs/dss/selectApiDetailFunction.do".to_string()));
        assert!(signals
            .ajax_urls
            .contains(&"/iim/api/myPageUrl.do".to_string()));

        // JSON-LD
        assert!(signals.json_ld_types.contains(&"Dataset".to_string()));

        // script types
        assert!(signals
            .script_types
            .contains(&"application/ld+json".to_string()));
        assert!(signals
            .script_types
            .contains(&"text/javascript".to_string()));
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
        assert!(signals
            .meta_tags
            .iter()
            .any(|m| m.key == "og:title" && m.content == "공기질 API"));
        assert!(signals
            .meta_tags
            .iter()
            .any(|m| m.attr_type == "http-equiv" && m.key == "X-UA-Compatible"));
        assert!(signals.link_rels.contains(&"stylesheet".to_string()));
        assert!(signals.link_rels.contains(&"canonical".to_string()));
    }

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
            .filter(|e| {
                e.metadata()
                    .map(|m| m.len() > 100_000 && m.len() < 200_000)
                    .unwrap_or(false)
            })
            .take(3)
            .collect();

        for entry in small_files.iter().chain(normal_files.iter()) {
            let html = std::fs::read_to_string(entry.path()).unwrap();
            let signals = extract_signals(&html);

            // 기본 검증: 패닉 없이 실행, 크기 일치
            assert_eq!(signals.file_size_bytes, html.len());
            // title은 항상 있어야 함 (에러 페이지도 title 있음)
            assert!(
                !signals.title.is_empty(),
                "title empty for {:?}",
                entry.path()
            );

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
}
