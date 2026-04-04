//! 번들에서 API 카탈로그 markdown 문서를 생성한다.
//!
//! Usage: cargo run --bin gen-catalog-docs -- --bundle data/bundle.zstd --output docs/api-catalog

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use korea_cli::core::types::{ApiSpec, Bundle, CatalogEntry, SpecStatus, CURRENT_SCHEMA_VERSION};

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
/// 한글/알파벳/숫자/하이픈/언더스코어만 유지, 나머지 하이픈으로 치환, 연속 하이픈 제거.
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

/// 기관명 → 파일명 변환 (sanitize + 빈 결과 fallback). [eval B-1]
/// render_readme와 main 양쪽에서 동일한 파일명을 생성하기 위해 공유.
fn org_safe_filename(org: &str, fallback_list_id: &str) -> String {
    let safe = sanitize_filename(org);
    if safe.is_empty() {
        format!("_org_{}", fallback_list_id)
    } else {
        safe
    }
}

/// markdown 테이블 셀에서 위험 문자 이스케이프. [B1]
fn escape_md_table(s: &str) -> String {
    s.replace('|', r"\|").replace('\r', "").replace('\n', " ")
}

/// 기관별 markdown 페이지 생성. title/description에 escape_md_table 적용 [B1].
fn render_org_page(
    org_name: &str,
    entries: &[&CatalogEntry],
    specs: &HashMap<String, ApiSpec>,
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
    let safe_heading = org_name.replace('\r', "").replace('\n', " ");
    md.push_str(&format!("# {}\n\n", safe_heading));
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
            let ops = specs
                .get(&e.list_id)
                .map(|s| s.operations.len())
                .unwrap_or(0);
            let title = escape_md_table(&e.title);
            let desc = escape_md_table(&e.description);
            let id_link = format!(
                "[{}](https://www.data.go.kr/data/{}/openapi.do)",
                e.list_id, e.list_id
            );
            md.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                title, id_link, desc, ops
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
            let desc = escape_md_table(&e.description);
            let id_link = format!(
                "[{}](https://www.data.go.kr/data/{}/openapi.do)",
                e.list_id, e.list_id
            );
            let url = &e.endpoint_url;
            let link = if url.trim().starts_with("http") {
                let safe_url = url.replace('>', "%3E"); // [eval B-2]
                format!("[링크](<{}>)", safe_url)
            } else {
                "—".into()
            };
            md.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                title, id_link, desc, link
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
            let id_link = format!(
                "[{}](https://www.data.go.kr/data/{}/openapi.do)",
                e.list_id, e.list_id
            );
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                title,
                id_link,
                e.spec_status.user_message()
            ));
        }
        md.push('\n');
    }

    md
}

/// 기관별 통계 [W8]
struct OrgStats {
    safe_filename: String, // [eval B-1] render_readme와 main의 파일명 일관성
    org: String,
    total: usize,
    available: usize,
    external: usize,
    ops: usize,
    total_requests: u64,
}

/// 요약 README.md 생성 (통계 + 기관별 목차)
fn render_readme(
    groups: &BTreeMap<String, Vec<&CatalogEntry>>,
    specs: &HashMap<String, ApiSpec>,
) -> String {
    let total_api: usize = groups.values().map(|v| v.len()).sum(); // [W2]
    let total_available: usize = groups
        .values()
        .flat_map(|v| v.iter())
        .filter(|e| e.spec_status == SpecStatus::Available)
        .count();
    let total_external: usize = groups
        .values()
        .flat_map(|v| v.iter())
        .filter(|e| e.spec_status == SpecStatus::External)
        .count();

    let mut md = String::new();
    md.push_str("# API 카탈로그\n\n");
    md.push_str(&format!(
        "> **{}개** 공공데이터 API | **{}개** 호출 가능 | **{}개** 외부 링크\n\n",
        total_api, total_available, total_external
    ));
    md.push_str("이 문서는 [korea-cli](../../README.md) 번들에서 자동 생성되었습니다.\n\n");
    md.push_str("- **호출 가능 (Available)**: `korea-cli call`로 직접 호출 가능한 API\n");
    md.push_str("- **외부 링크 (External)**: 외부 포탈에서 제공, 링크로 안내\n\n");

    // 기관별 통계 수집
    let mut org_stats: Vec<OrgStats> = groups
        .iter()
        .map(|(org, entries)| OrgStats {
            safe_filename: org_safe_filename(org, &entries[0].list_id), // [eval B-1]
            org: org.clone(),
            total: entries.len(),
            available: entries
                .iter()
                .filter(|e| e.spec_status == SpecStatus::Available)
                .count(),
            external: entries
                .iter()
                .filter(|e| e.spec_status == SpecStatus::External)
                .count(),
            ops: entries
                .iter()
                .filter_map(|e| specs.get(&e.list_id))
                .map(|s| s.operations.len())
                .sum(),
            total_requests: entries.iter().map(|e| e.request_count as u64).sum(), // [W3]
        })
        .collect();
    // request_count 내림차순, 동률 시 org_name으로 안정 정렬
    org_stats.sort_by(|a, b| {
        b.total_requests
            .cmp(&a.total_requests)
            .then(a.org.cmp(&b.org))
    });

    md.push_str("## 기관별 목록\n\n");
    md.push_str("| 기관 | 전체 | 호출 가능 | 외부 링크 | 오퍼레이션 |\n");
    md.push_str("|------|-----:|---------:|---------:|-----------:|\n");
    for s in &org_stats {
        let org_display = escape_md_table(&s.org);
        md.push_str(&format!(
            "| [{}](by-org/{}.md) | {} | {} | {} | {} |\n",
            org_display, s.safe_filename, s.total, s.available, s.external, s.ops
        ));
    }
    md.push('\n');

    md
}

fn main() -> Result<()> {
    let args = Args::parse();

    // 번들 로드
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

    eprintln!(
        "{} API 로드, {} specs",
        bundle.catalog.len(),
        bundle.specs.len()
    );

    // 그룹핑
    let groups = group_by_org(&bundle);
    eprintln!("{} 기관", groups.len());

    // 출력 디렉토리 생성
    let by_org_dir = args.output.join("by-org");
    std::fs::create_dir_all(&by_org_dir)?;

    // README.md 생성 [W2]
    let readme = render_readme(&groups, &bundle.specs);
    std::fs::write(args.output.join("README.md"), &readme)?;
    eprintln!("README.md 생성");

    // 기관별 파일 생성 — 공유 org_safe_filename + 충돌 감지 [eval B-1][eval W1]
    let mut seen_filenames: HashMap<String, String> = HashMap::new();
    for (org, entries) in &groups {
        let safe_name = org_safe_filename(org, &entries[0].list_id);
        if let Some(prev_org) = seen_filenames.get(&safe_name) {
            anyhow::bail!(
                "파일명 충돌: {:?}와 {:?} → {:?}.md",
                prev_org,
                org,
                safe_name
            );
        }
        seen_filenames.insert(safe_name.clone(), org.clone());
        let content = render_org_page(org, entries, &bundle.specs);
        let path = by_org_dir.join(format!("{}.md", safe_name));
        std::fs::write(&path, &content)?;
    }
    eprintln!("{} 기관별 파일 생성 완료", groups.len());
    eprintln!("출력: {:?}", args.output);

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
        assert_eq!(escape_md_table("CRLF\r\n포함"), "CRLF 포함");
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

    #[test]
    fn test_render_readme() {
        let bundle = make_test_bundle();
        let groups = group_by_org(&bundle);
        let content = render_readme(&groups, &bundle.specs);
        assert!(content.contains("# API 카탈로그"));
        assert!(content.contains("3")); // 총 API 수
        assert!(content.contains("기상청"));
        assert!(content.contains("국토교통부"));
        // 기관 테이블이 request_count 합 내림차순인지 확인
        let molit_pos = content.find("국토교통부").unwrap();
        let kma_pos = content.find("기상청").unwrap();
        // 국토교통부(2000) > 기상청(1500) → 국토교통부가 먼저
        assert!(molit_pos < kma_pos);
        // sanitize_filename이 링크에 적용되는지 [B3]
        assert!(content.contains("by-org/국토교통부.md"));
    }
}
