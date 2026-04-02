//! 번들에서 API 카탈로그 markdown 문서를 생성한다.
//!
//! Usage: cargo run --bin gen-catalog-docs -- --bundle data/bundle.zstd --output docs/api-catalog

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use korea_cli::core::types::{Bundle, CatalogEntry, SpecStatus, CURRENT_SCHEMA_VERSION};

#[derive(Parser)]
#[command(about = "번들에서 API 카탈로그 markdown 문서를 생성한다")]
struct Args {
    /// 번들 파일 경로
    #[arg(long, default_value = "data/bundle.zstd")]
    bundle: PathBuf,

    /// 출력 디렉토리
    #[arg(long, default_value = "docs/api-catalog")]
    output: PathBuf,
}

/// CatalogEntry를 org_name으로 그룹핑. 각 그룹 내 request_count 내림차순 정렬.
/// 빈 org_name은 "(기관 미상)"으로 대체. [W5]
fn group_by_org(bundle: &Bundle) -> BTreeMap<String, Vec<&CatalogEntry>> {
    let mut groups: BTreeMap<String, Vec<&CatalogEntry>> = BTreeMap::new();
    for entry in &bundle.catalog {
        let org = if entry.org_name.trim().is_empty() {
            "(기관 미상)".to_string()
        } else {
            entry.org_name.clone()
        };
        groups.entry(org).or_default().push(entry);
    }
    for entries in groups.values_mut() {
        entries.sort_by(|a, b| b.request_count.cmp(&a.request_count));
    }
    groups
}

/// 기관명을 파일시스템 안전한 이름으로 변환. [B2][B3]
/// 한글/알파벳/숫자/하이픈만 유지, 나머지 하이픈으로 치환, 연속 하이픈 제거.
fn sanitize_filename(s: &str) -> String {
    let replaced: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    replaced
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// markdown 테이블 셀에서 위험 문자 이스케이프. [B1]
fn escape_md_table(s: &str) -> String {
    s.replace('|', r"\|").replace('\n', " ")
}

/// 문자열을 max_len 이하로 자르고 '…' 추가
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len - 1).collect();
        format!("{}…", truncated)
    }
}

/// 기관별 markdown 페이지 생성. title/description에 escape_md_table 적용 [B1].
fn render_org_page(
    org_name: &str,
    entries: &[&CatalogEntry],
    specs: &std::collections::HashMap<String, korea_cli::core::types::ApiSpec>,
) -> String {
    let available: Vec<_> = entries
        .iter()
        .copied()
        .filter(|e| e.spec_status == SpecStatus::Available)
        .collect();
    let external: Vec<_> = entries
        .iter()
        .copied()
        .filter(|e| e.spec_status == SpecStatus::External)
        .collect();
    let other: Vec<_> = entries
        .iter()
        .copied()
        .filter(|e| !matches!(e.spec_status, SpecStatus::Available | SpecStatus::External))
        .collect();

    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", org_name));
    md.push_str(&format!(
        "> {} API | 호출 가능 {} | 외부 링크 {}\n\n",
        entries.len(),
        available.len(),
        external.len()
    ));

    // Available
    if !available.is_empty() {
        md.push_str(&format!(
            "## 호출 가능 (Available) — {}개\n\n",
            available.len()
        ));
        md.push_str("| API | ID | 설명 | 오퍼레이션 |\n");
        md.push_str("|-----|-----|------|----------|\n");
        for e in &available {
            let ops = specs.get(&e.list_id).map(|s| s.operations.len()).unwrap_or(0);
            let title = escape_md_table(&e.title);
            let desc = escape_md_table(&truncate(&e.description, 60));
            md.push_str(&format!(
                "| {} | `{}` | {} | {} |\n",
                title, e.list_id, desc, ops
            ));
        }
        md.push('\n');
    }

    // External
    if !external.is_empty() {
        md.push_str(&format!(
            "## 외부 링크 (External) — {}개\n\n",
            external.len()
        ));
        md.push_str("| API | ID | 설명 | 링크 |\n");
        md.push_str("|-----|-----|------|------|\n");
        for e in &external {
            let title = escape_md_table(&e.title);
            let desc = escape_md_table(&truncate(&e.description, 60));
            let url = &e.endpoint_url;
            let link = if url.starts_with("http") {
                format!("[링크]({})", url)
            } else {
                "—".into()
            };
            md.push_str(&format!(
                "| {} | `{}` | {} | {} |\n",
                title, e.list_id, desc, link
            ));
        }
        md.push('\n');
    }

    // Other (Skeleton, CatalogOnly 등 — 소수)
    if !other.is_empty() {
        md.push_str(&format!("## 기타 — {}개\n\n", other.len()));
        md.push_str("| API | ID | 상태 |\n");
        md.push_str("|-----|-----|------|\n");
        for e in &other {
            let title = escape_md_table(&e.title);
            md.push_str(&format!(
                "| {} | `{}` | {:?} |\n",
                title, e.list_id, e.spec_status
            ));
        }
        md.push('\n');
    }

    md
}

fn main() -> Result<()> {
    let args = Args::parse();

    let bytes = std::fs::read(&args.bundle)?;
    let bundle = korea_cli::core::bundle::decompress_and_deserialize(&bytes)?;

    // 스키마 버전 체크 [W1]
    if bundle.metadata.schema_version != CURRENT_SCHEMA_VERSION {
        anyhow::bail!(
            "번들 스키마 버전 불일치: {} (현재: {})",
            bundle.metadata.schema_version,
            CURRENT_SCHEMA_VERSION
        );
    }

    let groups = group_by_org(&bundle);
    eprintln!("{} 기관, {} API", groups.len(), bundle.catalog.len());

    Ok(())
}

#[cfg(test)]
mod tests {
    use korea_cli::core::types::{
        ApiProtocol, ApiSpec, AuthMethod, Bundle, BundleMetadata, CatalogEntry, ErrorCheck,
        ResponseExtractor, ResponseFormat, SpecStatus, CURRENT_SCHEMA_VERSION,
    };
    use std::collections::HashMap;

    fn make_test_bundle() -> Bundle {
        let catalog = vec![
            CatalogEntry {
                list_id: "100".into(),
                title: "날씨 API".into(),
                description: "날씨 조회".into(),
                keywords: vec!["날씨".into()],
                org_name: "기상청".into(),
                category: "기상".into(),
                request_count: 1000,
                endpoint_url: "https://apis.data.go.kr/weather".into(),
                spec_status: SpecStatus::Available,
            },
            CatalogEntry {
                list_id: "200".into(),
                title: "대기질 API".into(),
                description: "대기질 조회".into(),
                keywords: vec!["대기".into()],
                org_name: "기상청".into(),
                category: "기상".into(),
                request_count: 500,
                endpoint_url: "https://apihub.kma.go.kr/air".into(),
                spec_status: SpecStatus::External,
            },
            CatalogEntry {
                list_id: "300".into(),
                title: "부동산 API".into(),
                description: "실거래가 조회".into(),
                keywords: vec!["부동산".into()],
                org_name: "국토교통부".into(),
                category: "부동산".into(),
                request_count: 2000,
                endpoint_url: "https://apis.data.go.kr/realestate".into(),
                spec_status: SpecStatus::Available,
            },
        ];
        let mut specs = HashMap::new();
        specs.insert(
            "100".into(),
            ApiSpec {
                list_id: "100".into(),
                base_url: "https://apis.data.go.kr/weather".into(),
                protocol: ApiProtocol::DataGoKrRest,
                auth: AuthMethod::None,
                extractor: ResponseExtractor {
                    data_path: vec![],
                    error_check: ErrorCheck::HttpStatus,
                    pagination: None,
                    format: ResponseFormat::Json,
                },
                operations: vec![],
                fetched_at: "2026-04-03".into(),
            },
        );
        Bundle {
            metadata: BundleMetadata {
                version: "test".into(),
                schema_version: CURRENT_SCHEMA_VERSION,
                api_count: 3,
                spec_count: 1,
                checksum: "test".into(),
            },
            catalog,
            specs,
        }
    }

    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("기상청"), "기상청");
        assert_eq!(sanitize_filename("국토교통부 산하"), "국토교통부-산하");
        assert_eq!(sanitize_filename("기획재정부/국세청"), "기획재정부-국세청");
        assert_eq!(
            sanitize_filename("서울특별시 (강남구)"),
            "서울특별시-강남구"
        );
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn test_escape_md_table() {
        assert_eq!(escape_md_table("GET | POST"), r"GET \| POST");
        assert_eq!(escape_md_table("줄바꿈\n포함"), "줄바꿈 포함");
    }

    #[test]
    fn test_group_by_org() {
        let bundle = make_test_bundle();
        let groups = group_by_org(&bundle);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["기상청"].len(), 2);
        assert_eq!(groups["국토교통부"].len(), 1);
    }

    #[test]
    fn test_group_by_org_sorted_by_request_count() {
        let bundle = make_test_bundle();
        let groups = group_by_org(&bundle);
        let kma = &groups["기상청"];
        // request_count 내림차순: 날씨(1000) > 대기질(500)
        assert_eq!(kma[0].list_id, "100");
        assert_eq!(kma[1].list_id, "200");
    }

    #[test]
    fn test_render_org_page() {
        let bundle = make_test_bundle();
        let groups = group_by_org(&bundle);
        let content = render_org_page("기상청", &groups["기상청"], &bundle.specs);
        assert!(content.contains("# 기상청"));
        assert!(content.contains("날씨 API"));
        assert!(content.contains("대기질 API"));
        assert!(content.contains("호출 가능")); // Available 섹션
        assert!(content.contains("외부 링크")); // External 섹션
        assert!(content.contains("apihub.kma.go.kr")); // External URL 표시
    }

    #[test]
    fn test_render_org_page_available_only() {
        let bundle = make_test_bundle();
        let groups = group_by_org(&bundle);
        let content = render_org_page("국토교통부", &groups["국토교통부"], &bundle.specs);
        assert!(content.contains("# 국토교통부"));
        assert!(content.contains("부동산 API"));
        assert!(!content.contains("## 외부 링크")); // External 없으면 섹션 헤더 미표시
    }
}
