//! HTML 패턴 발견 2단계: page_raw_signals.json에서 빈도 분석으로 패턴을 발견한다.
//!
//! analyze-pages가 추출한 raw signals를 읽어서:
//! 1. 각 요소(id, class, js_var 등)의 출현 빈도를 집계
//! 2. 전체 페이지에 공통이 아닌 "차별적 신호"를 식별
//! 3. js_var_types 기반으로 페이지를 클러스터링
//!
//! Usage: cargo run --bin summarize-signals [--input data/page_raw_signals.json] [--output data/signal_summary.json]

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ── PageSignals 타입 (analyze_pages.rs와 동일, 역직렬화용) ─────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PageSignals {
    file_size_bytes: usize,
    tag_counts: BTreeMap<String, usize>,
    ids: Vec<String>,
    names: Vec<String>,
    classes: Vec<String>,
    data_attrs: BTreeMap<String, Vec<String>>,
    th_texts: Vec<String>,
    options: Vec<OptionSignal>,
    js_vars: BTreeMap<String, JsVarInfo>,
    ajax_urls: Vec<String>,
    json_ld_types: Vec<String>,
    script_types: Vec<String>,
    meta_tags: Vec<MetaSignal>,
    link_rels: Vec<String>,
    title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsVarInfo {
    value_type: String,
    size: usize,
    value_preview: Option<String>,
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
    attr_type: String,
    key: String,
    content: String,
}

// ── Summary 데이터 구조 ────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct SignalSummary {
    total_files: usize,
    element_frequencies: ElementFrequencies,
    discriminating_signals: Vec<DiscriminatingSignal>,
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
    js_var_types: BTreeMap<String, usize>,
    ajax_urls: BTreeMap<String, usize>,
    json_ld_types: BTreeMap<String, usize>,
    script_types: BTreeMap<String, usize>,
    meta_keys: BTreeMap<String, usize>,
    link_rels: BTreeMap<String, usize>,
    select_ids: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscriminatingSignal {
    category: String,
    signal: String,
    count: usize,
    percentage: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Cluster {
    name: String,
    defining_signals: Vec<String>,
    count: usize,
    sample_files: Vec<String>,
}

// ── 빈도 집계 ──────────────────────────────────────────────────────────────

/// 모든 PageSignals를 순회하며 각 요소의 출현 파일 수를 집계한다.
/// 한 파일 내에서 같은 요소가 여러 번 나와도 1로 카운트한다.
fn count_frequencies(all_signals: &BTreeMap<String, PageSignals>) -> ElementFrequencies {
    let mut freq = ElementFrequencies::default();

    for signals in all_signals.values() {
        // ids: 파일 내 고유 값만 카운트
        for id in &signals.ids {
            *freq.ids.entry(id.clone()).or_insert(0) += 1;
        }

        // names
        for name in &signals.names {
            *freq.names.entry(name.clone()).or_insert(0) += 1;
        }

        // classes
        for class in &signals.classes {
            *freq.classes.entry(class.clone()).or_insert(0) += 1;
        }

        // data_attr_keys
        for key in signals.data_attrs.keys() {
            *freq.data_attr_keys.entry(key.clone()).or_insert(0) += 1;
        }

        // th_texts
        for th in &signals.th_texts {
            *freq.th_texts.entry(th.clone()).or_insert(0) += 1;
        }

        // js_var_names + js_var_types ("name:type" 조합)
        for (name, info) in &signals.js_vars {
            *freq.js_var_names.entry(name.clone()).or_insert(0) += 1;
            let type_key = format!("{}:{}", name, info.value_type);
            *freq.js_var_types.entry(type_key).or_insert(0) += 1;
        }

        // ajax_urls
        for url in &signals.ajax_urls {
            *freq.ajax_urls.entry(url.clone()).or_insert(0) += 1;
        }

        // json_ld_types
        for t in &signals.json_ld_types {
            *freq.json_ld_types.entry(t.clone()).or_insert(0) += 1;
        }

        // script_types
        for t in &signals.script_types {
            *freq.script_types.entry(t.clone()).or_insert(0) += 1;
        }

        // meta_keys: "attr_type:key" 형식
        for meta in &signals.meta_tags {
            let key = format!("{}:{}", meta.attr_type, meta.key);
            *freq.meta_keys.entry(key).or_insert(0) += 1;
        }

        // link_rels
        for rel in &signals.link_rels {
            *freq.link_rels.entry(rel.clone()).or_insert(0) += 1;
        }

        // select_ids (from options)
        for opt in &signals.options {
            *freq.select_ids.entry(opt.select_id.clone()).or_insert(0) += 1;
        }
    }

    freq
}

// ── 차별적 신호 식별 ───────────────────────────────────────────────────────

/// 전체 파일에 공통(count == total)이 아니고 존재하는(count > 0) 신호를 추출한다.
/// count 내림차순으로 정렬하여 반환한다.
fn find_discriminating_signals(
    freq: &ElementFrequencies,
    total: usize,
) -> Vec<DiscriminatingSignal> {
    let mut signals = Vec::new();

    let categories: Vec<(&str, &BTreeMap<String, usize>)> = vec![
        ("ids", &freq.ids),
        ("names", &freq.names),
        ("classes", &freq.classes),
        ("data_attr_keys", &freq.data_attr_keys),
        ("th_texts", &freq.th_texts),
        ("js_var_names", &freq.js_var_names),
        ("js_var_types", &freq.js_var_types),
        ("ajax_urls", &freq.ajax_urls),
        ("json_ld_types", &freq.json_ld_types),
        ("script_types", &freq.script_types),
        ("meta_keys", &freq.meta_keys),
        ("link_rels", &freq.link_rels),
        ("select_ids", &freq.select_ids),
    ];

    for (category, map) in categories {
        for (signal, &count) in map {
            if count > 0 && count < total {
                let percentage = (count as f64 / total as f64) * 100.0;
                signals.push(DiscriminatingSignal {
                    category: category.to_string(),
                    signal: signal.clone(),
                    count,
                    percentage,
                });
            }
        }
    }

    signals.sort_by(|a, b| b.count.cmp(&a.count));
    signals
}

// ── 클러스터링 ─────────────────────────────────────────────────────────────

/// js_var_types 기반 fingerprint로 페이지를 클러스터링한다.
///
/// 1. js_var_types에서 count > 10인 상위 20개를 key signals로 선정
/// 2. 각 파일에서 매칭되는 key signals로 fingerprint 생성 ("+"-join)
/// 3. fingerprint별로 파일을 그룹핑
/// 4. count 내림차순으로 정렬, 각 클러스터에 최대 3개 sample files
fn build_clusters(
    freq: &ElementFrequencies,
    all_signals: &BTreeMap<String, PageSignals>,
) -> Vec<Cluster> {
    // 1. key signals 선정: js_var_types에서 변별력 있는 것만 선택
    //    count > 10 이면서 전체의 95% 미만 (near-universal 제외)
    let total = all_signals.len();
    let upper_threshold = (total as f64 * 0.95) as usize;
    let mut jvt_entries: Vec<(&String, &usize)> = freq
        .js_var_types
        .iter()
        .filter(|(_, &count)| count > 10 && count < upper_threshold)
        .collect();
    jvt_entries.sort_by(|a, b| b.1.cmp(a.1));
    let key_signals: Vec<&str> = jvt_entries
        .iter()
        .take(20)
        .map(|(k, _)| k.as_str())
        .collect();

    // 2-3. 각 파일의 fingerprint 생성 & 그룹핑
    let mut cluster_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (filename, signals) in all_signals {
        // 이 파일이 가진 js_var_types 집합
        let file_jvt: std::collections::HashSet<String> = signals
            .js_vars
            .iter()
            .map(|(name, info)| format!("{}:{}", name, info.value_type))
            .collect();

        // key signals 중 매칭되는 것만 추출 (순서 유지)
        let matching: Vec<&str> = key_signals
            .iter()
            .filter(|&&s| file_jvt.contains(s))
            .copied()
            .collect();

        let fingerprint = if matching.is_empty() {
            "(none)".to_string()
        } else {
            matching.join("+")
        };

        cluster_map
            .entry(fingerprint)
            .or_default()
            .push(filename.clone());
    }

    // 4. Cluster 벡터 생성, count 내림차순 정렬
    let mut clusters: Vec<Cluster> = cluster_map
        .into_iter()
        .map(|(fingerprint, files)| {
            let defining_signals: Vec<String> =
                fingerprint.split('+').map(|s| s.to_string()).collect();
            let count = files.len();
            let sample_files: Vec<String> = files.iter().take(3).cloned().collect();
            Cluster {
                name: fingerprint,
                defining_signals,
                count,
                sample_files,
            }
        })
        .collect();

    clusters.sort_by(|a, b| b.count.cmp(&a.count));
    clusters
}

// ── CLI ─────────────────────────────────────────────────────────────────────

struct Config {
    input: PathBuf,
    output: PathBuf,
}

fn parse_args() -> Config {
    let mut input = PathBuf::from("data/page_raw_signals.json");
    let mut output = PathBuf::from("data/signal_summary.json");

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                i += 1;
                if i < args.len() {
                    input = PathBuf::from(&args[i]);
                }
            }
            "--output" => {
                i += 1;
                if i < args.len() {
                    output = PathBuf::from(&args[i]);
                }
            }
            other => {
                eprintln!("Unknown argument: {other}");
                eprintln!("Usage: summarize-signals [--input <path>] [--output <path>]");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Config { input, output }
}

fn main() -> Result<()> {
    let config = parse_args();

    eprintln!(
        "[summarize-signals] input: {}, output: {}",
        config.input.display(),
        config.output.display()
    );

    // 1. 입력 파일 로드
    eprintln!("[1/4] Loading raw signals...");
    let raw = std::fs::read_to_string(&config.input)
        .with_context(|| format!("Failed to read {}", config.input.display()))?;
    let all_signals: BTreeMap<String, PageSignals> =
        serde_json::from_str(&raw).context("Failed to parse page_raw_signals.json")?;

    let total_files = all_signals.len();
    eprintln!("  Loaded {total_files} files");

    // 2. 빈도 집계
    eprintln!("[2/4] Counting frequencies...");
    let freq = count_frequencies(&all_signals);
    eprintln!(
        "  ids: {}, classes: {}, js_var_types: {}, meta_keys: {}",
        freq.ids.len(),
        freq.classes.len(),
        freq.js_var_types.len(),
        freq.meta_keys.len()
    );

    // 3. 차별적 신호 식별
    eprintln!("[3/4] Finding discriminating signals...");
    let discriminating = find_discriminating_signals(&freq, total_files);
    eprintln!("  Found {} discriminating signals", discriminating.len());

    // 4. 클러스터링
    eprintln!("[4/4] Building clusters...");
    let clusters = build_clusters(&freq, &all_signals);
    eprintln!("  Found {} clusters", clusters.len());

    // 결과 조립 및 저장
    let summary = SignalSummary {
        total_files,
        element_frequencies: freq,
        discriminating_signals: discriminating,
        clusters,
    };

    let json =
        serde_json::to_string_pretty(&summary).context("Failed to serialize SignalSummary")?;

    if let Some(parent) = config.output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    std::fs::write(&config.output, &json)
        .with_context(|| format!("Failed to write {}", config.output.display()))?;

    eprintln!(
        "[done] Wrote {} ({} bytes)",
        config.output.display(),
        json.len()
    );

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// SignalSummary를 JSON으로 직렬화/역직렬화 라운드트립 검증
    #[test]
    fn test_summary_json_roundtrip() {
        let mut summary = SignalSummary::default();
        summary.total_files = 42;
        summary
            .element_frequencies
            .ids
            .insert("swaggerUi".to_string(), 30);
        summary
            .element_frequencies
            .js_var_types
            .insert("swaggerJson:object".to_string(), 25);
        summary.discriminating_signals.push(DiscriminatingSignal {
            category: "js_var_types".to_string(),
            signal: "swaggerJson:object".to_string(),
            count: 25,
            percentage: 59.52,
        });
        summary.clusters.push(Cluster {
            name: "swaggerJson:object".to_string(),
            defining_signals: vec!["swaggerJson:object".to_string()],
            count: 25,
            sample_files: vec!["15001234.html".to_string()],
        });

        let json = serde_json::to_string_pretty(&summary).unwrap();
        let deserialized: SignalSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total_files, 42);
        assert_eq!(deserialized.element_frequencies.ids["swaggerUi"], 30);
        assert_eq!(
            deserialized.element_frequencies.js_var_types["swaggerJson:object"],
            25
        );
        assert_eq!(deserialized.discriminating_signals.len(), 1);
        assert_eq!(deserialized.discriminating_signals[0].count, 25);
        assert_eq!(deserialized.clusters.len(), 1);
        assert_eq!(deserialized.clusters[0].count, 25);
    }

    /// 3개의 PageSignals로 빈도 집계 정확성 검증
    #[test]
    fn test_count_frequencies() {
        let mut all = BTreeMap::new();

        // 파일 1: id=alpha, js_var swaggerJson:object
        let mut s1 = PageSignals::default();
        s1.ids = vec!["alpha".to_string(), "beta".to_string()];
        s1.classes = vec!["common".to_string()];
        s1.js_vars.insert(
            "swaggerJson".to_string(),
            JsVarInfo {
                value_type: "object".to_string(),
                size: 100,
                value_preview: None,
            },
        );
        s1.options.push(OptionSignal {
            select_id: "sel1".to_string(),
            count: 2,
            values: vec![],
            texts: vec![],
        });
        all.insert("file1.html".to_string(), s1);

        // 파일 2: id=alpha, id=gamma, js_var tableData:array
        let mut s2 = PageSignals::default();
        s2.ids = vec!["alpha".to_string(), "gamma".to_string()];
        s2.classes = vec!["common".to_string(), "special".to_string()];
        s2.js_vars.insert(
            "tableData".to_string(),
            JsVarInfo {
                value_type: "array".to_string(),
                size: 50,
                value_preview: None,
            },
        );
        s2.th_texts = vec!["이름".to_string()];
        all.insert("file2.html".to_string(), s2);

        // 파일 3: id=beta, js_var swaggerJson:object + tableData:array
        let mut s3 = PageSignals::default();
        s3.ids = vec!["beta".to_string()];
        s3.classes = vec!["common".to_string()];
        s3.js_vars.insert(
            "swaggerJson".to_string(),
            JsVarInfo {
                value_type: "object".to_string(),
                size: 200,
                value_preview: None,
            },
        );
        s3.js_vars.insert(
            "tableData".to_string(),
            JsVarInfo {
                value_type: "array".to_string(),
                size: 30,
                value_preview: None,
            },
        );
        s3.options.push(OptionSignal {
            select_id: "sel1".to_string(),
            count: 3,
            values: vec![],
            texts: vec![],
        });
        all.insert("file3.html".to_string(), s3);

        let freq = count_frequencies(&all);

        // ids: alpha=2(file1,file2), beta=2(file1,file3), gamma=1(file2)
        assert_eq!(freq.ids["alpha"], 2);
        assert_eq!(freq.ids["beta"], 2);
        assert_eq!(freq.ids["gamma"], 1);

        // classes: common=3, special=1
        assert_eq!(freq.classes["common"], 3);
        assert_eq!(freq.classes["special"], 1);

        // js_var_names: swaggerJson=2, tableData=2
        assert_eq!(freq.js_var_names["swaggerJson"], 2);
        assert_eq!(freq.js_var_names["tableData"], 2);

        // js_var_types: swaggerJson:object=2, tableData:array=2
        assert_eq!(freq.js_var_types["swaggerJson:object"], 2);
        assert_eq!(freq.js_var_types["tableData:array"], 2);

        // th_texts: 이름=1
        assert_eq!(freq.th_texts["이름"], 1);

        // select_ids: sel1=2(file1, file3)
        assert_eq!(freq.select_ids["sel1"], 2);
    }

    /// 차별적 신호 필터링 검증: 공통(count==total)은 제외, 나머지만 포함
    #[test]
    fn test_find_discriminating_signals() {
        let total = 100;
        let mut freq = ElementFrequencies::default();

        // 공통 신호 (100/100): 제외 대상
        freq.ids.insert("commonId".to_string(), 100);

        // 드문 신호 (5/100): 포함 대상
        freq.ids.insert("rareId".to_string(), 5);

        // 고유 신호 (1/100): 포함 대상
        freq.classes.insert("uniqueClass".to_string(), 1);

        // 준-공통 신호 (80/100): 포함 대상
        freq.js_var_types.insert("config:object".to_string(), 80);

        // 또 다른 공통 신호 (100/100): 제외 대상
        freq.classes.insert("universalClass".to_string(), 100);

        let result = find_discriminating_signals(&freq, total);

        // commonId(100)와 universalClass(100)는 제외되어야 함
        assert!(
            !result.iter().any(|s| s.signal == "commonId"),
            "commonId (count==total) should be excluded"
        );
        assert!(
            !result.iter().any(|s| s.signal == "universalClass"),
            "universalClass (count==total) should be excluded"
        );

        // rareId, uniqueClass, config:object는 포함
        assert!(
            result.iter().any(|s| s.signal == "rareId"),
            "rareId should be included"
        );
        assert!(
            result.iter().any(|s| s.signal == "uniqueClass"),
            "uniqueClass should be included"
        );
        assert!(
            result.iter().any(|s| s.signal == "config:object"),
            "config:object should be included"
        );

        // count 내림차순 정렬 확인
        assert_eq!(result[0].signal, "config:object");
        assert_eq!(result[0].count, 80);
        assert!((result[0].percentage - 80.0).abs() < 0.01);

        assert_eq!(result[1].signal, "rareId");
        assert_eq!(result[1].count, 5);

        assert_eq!(result[2].signal, "uniqueClass");
        assert_eq!(result[2].count, 1);
        assert!((result[2].percentage - 1.0).abs() < 0.01);
    }
}
