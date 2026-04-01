//! Catalog load, save, search, and update from meta API.

use crate::core::types::*;
use anyhow::Result;
use std::collections::HashMap;

/// Raw entry from meta API's open-data-list endpoint.
#[derive(Debug, serde::Deserialize)]
struct MetaApiEntry {
    list_id: String,
    list_title: Option<String>,
    #[allow(dead_code)]
    title: Option<String>,
    desc: Option<String>,
    keywords: Option<String>,
    org_nm: Option<String>,
    new_category_nm: Option<String>,
    end_point_url: Option<String>,
    data_format: Option<String>,
    is_confirmed_for_dev: Option<String>,
    is_charged: Option<String>,
    request_cnt: Option<u32>,
    updated_at: Option<String>,
    operation_nm: Option<String>,
    operation_seq: Option<String>,
    request_param_nm: Option<String>,
    request_param_nm_en: Option<String>,
}

/// Parse meta API JSON response and group operations by list_id.
pub fn parse_meta_response(json: &serde_json::Value) -> Result<Vec<ApiService>> {
    let entries: Vec<MetaApiEntry> = serde_json::from_value(
        json.get("data")
            .ok_or_else(|| anyhow::anyhow!("missing 'data' field"))?
            .clone(),
    )?;

    // Use IndexMap-like ordering via Vec to preserve insertion order
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<MetaApiEntry>> = HashMap::new();
    for entry in entries {
        let lid = entry.list_id.clone();
        if !groups.contains_key(&lid) {
            order.push(lid.clone());
        }
        groups.entry(lid).or_default().push(entry);
    }

    let mut services: Vec<ApiService> = Vec::new();
    for list_id in &order {
        let entries = groups.get(list_id).unwrap();
        let first = &entries[0];
        let operations: Vec<OperationSummary> = entries
            .iter()
            .map(|e| OperationSummary {
                id: e.operation_seq.clone().unwrap_or_default(),
                name: e.operation_nm.clone().unwrap_or_default(),
                request_params: parse_quoted_csv(&e.request_param_nm),
                request_params_en: parse_csv(&e.request_param_nm_en),
            })
            .collect();

        services.push(ApiService {
            list_id: first.list_id.clone(),
            title: first.list_title.clone().unwrap_or_default(),
            description: first.desc.clone().unwrap_or_default(),
            keywords: parse_csv(&first.keywords),
            org_name: first.org_nm.clone().unwrap_or_default(),
            category: first.new_category_nm.clone().unwrap_or_default(),
            endpoint_url: first.end_point_url.clone().unwrap_or_default(),
            data_format: first.data_format.clone().unwrap_or_default(),
            auto_approve: first.is_confirmed_for_dev.as_deref() == Some("Y"),
            is_free: first.is_charged.as_deref() == Some("무료"),
            request_count: first.request_cnt.unwrap_or(0),
            updated_at: first.updated_at.clone().unwrap_or_default(),
            operations,
        });
    }

    Ok(services)
}

/// Parse comma-separated string: "사업자,국세청" -> ["사업자", "국세청"]
fn parse_csv(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse quoted CSV: "\"사업자등록번호\",\"개업일자\"" -> ["사업자등록번호", "개업일자"]
fn parse_quoted_csv(s: &Option<String>) -> Vec<String> {
    s.as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Fetch all services from meta API with pagination.
pub async fn fetch_all_services(api_key: &str) -> Result<Vec<ApiService>> {
    let client = reqwest::Client::new();
    let mut all_services: Vec<ApiService> = Vec::new();
    let mut page = 1;
    let per_page = 1000;

    loop {
        let url = format!(
            "https://api.odcloud.kr/api/15077093/v1/open-data-list?page={page}&perPage={per_page}"
        );
        let resp = client
            .get(&url)
            .header("Authorization", format!("Infuser {api_key}"))
            .send()
            .await?;

        let json: serde_json::Value = resp.json().await?;
        let total_count = json.get("totalCount").and_then(|v| v.as_u64()).unwrap_or(0);
        let current_count = json
            .get("currentCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let services = parse_meta_response(&json)?;
        all_services.extend(services);

        eprintln!(
            "  페이지 {page}: {current_count}건 (총 {total_count}건 중 {}건 수집)",
            all_services.len()
        );

        if current_count == 0 || (page * per_page) as u64 >= total_count {
            break;
        }
        page += 1;
    }

    // De-duplicate: merge operations for same list_id across pages
    let mut merged: HashMap<String, ApiService> = HashMap::new();
    for svc in all_services {
        merged
            .entry(svc.list_id.clone())
            .and_modify(|existing| {
                for op in &svc.operations {
                    if !existing.operations.iter().any(|e| e.id == op.id) {
                        existing.operations.push(op.clone());
                    }
                }
            })
            .or_insert(svc);
    }

    Ok(merged.into_values().collect())
}

/// Search bundle catalog entries by query string.
pub fn search_bundle_catalog(
    catalog: &[CatalogEntry],
    query: &str,
    category: Option<&str>,
    limit: usize,
) -> SearchResult {
    let query_lower = query.to_lowercase();
    let terms: Vec<&str> = query_lower.split_whitespace().collect();

    let mut scored: Vec<(u32, &CatalogEntry)> = catalog
        .iter()
        .filter_map(|entry| {
            if let Some(cat) = category {
                if !entry.category.contains(cat) {
                    return None;
                }
            }

            let searchable = format!(
                "{} {} {} {}",
                entry.title.to_lowercase(),
                entry.description.to_lowercase(),
                entry.keywords.join(" ").to_lowercase(),
                entry.org_name.to_lowercase(),
            );

            let match_count = terms.iter().filter(|t| searchable.contains(*t)).count();
            if match_count > 0 {
                let score = (match_count as u32) * 100 + entry.request_count;
                Some((score, entry))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    let total = scored.len();
    let results: Vec<SearchEntry> = scored
        .into_iter()
        .take(limit)
        .map(|(_, entry)| SearchEntry {
            list_id: entry.list_id.clone(),
            title: entry.title.clone(),
            description: entry.description.clone(),
            org: entry.org_name.clone(),
            category: entry.category.clone(),
            popularity: entry.request_count,
            spec_status: entry.spec_status,
            endpoint_url: entry.endpoint_url.clone(),
        })
        .collect();

    SearchResult { results, total }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_bundle_catalog() {
        let catalog = vec![
            CatalogEntry {
                list_id: "111".into(),
                title: "기상청 단기예보".into(),
                description: "날씨 예보 API".into(),
                keywords: vec!["기상".into(), "날씨".into()],
                org_name: "기상청".into(),
                category: "과학기술".into(),
                request_count: 500,
                endpoint_url: "https://apis.data.go.kr/weather".into(),
                spec_status: SpecStatus::Available,
            },
            CatalogEntry {
                list_id: "222".into(),
                title: "사업자등록 조회".into(),
                description: "사업자번호 진위확인".into(),
                keywords: vec!["사업자".into()],
                org_name: "국세청".into(),
                category: "산업경제".into(),
                request_count: 1000,
                endpoint_url: "https://api.odcloud.kr/nts".into(),
                spec_status: SpecStatus::Available,
            },
        ];

        let result = search_bundle_catalog(&catalog, "기상청", None, 10);
        assert_eq!(result.total, 1);
        assert_eq!(result.results[0].list_id, "111");
        assert_eq!(result.results[0].category, "과학기술");
        assert_eq!(result.results[0].spec_status, SpecStatus::Available);
        assert_eq!(
            result.results[0].endpoint_url,
            "https://apis.data.go.kr/weather"
        );
    }

    #[test]
    fn test_search_bundle_catalog_category_filter() {
        let catalog = vec![
            CatalogEntry {
                list_id: "111".into(),
                title: "기상청 API".into(),
                description: "".into(),
                keywords: vec![],
                org_name: "기상청".into(),
                category: "과학기술".into(),
                request_count: 100,
                endpoint_url: "".into(),
                spec_status: SpecStatus::CatalogOnly,
            },
            CatalogEntry {
                list_id: "222".into(),
                title: "기상 관련".into(),
                description: "".into(),
                keywords: vec![],
                org_name: "환경부".into(),
                category: "산업경제".into(),
                request_count: 200,
                endpoint_url: "".into(),
                spec_status: SpecStatus::CatalogOnly,
            },
        ];

        let result = search_bundle_catalog(&catalog, "기상", Some("과학기술"), 10);
        assert_eq!(result.total, 1);
        assert_eq!(result.results[0].list_id, "111");
    }

    #[test]
    fn test_search_bundle_catalog_scoring() {
        let catalog = vec![
            CatalogEntry {
                list_id: "111".into(),
                title: "사업자 조회".into(),
                description: "사업자 등록 상태 조회".into(),
                keywords: vec!["사업자".into()],
                org_name: "국세청".into(),
                category: "".into(),
                request_count: 100,
                endpoint_url: "".into(),
                spec_status: SpecStatus::Available,
            },
            CatalogEntry {
                list_id: "222".into(),
                title: "사업자 등록 확인".into(),
                description: "사업자 번호".into(),
                keywords: vec!["사업자".into(), "등록".into()],
                org_name: "국세청".into(),
                category: "".into(),
                request_count: 50,
                endpoint_url: "".into(),
                spec_status: SpecStatus::Skeleton,
            },
        ];

        let result = search_bundle_catalog(&catalog, "사업자 등록", None, 10);
        assert_eq!(result.total, 2);
    }
}
