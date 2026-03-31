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

/// Load catalog from local JSON file.
pub fn load_catalog() -> Result<Catalog> {
    let path = crate::config::paths::catalog_file()?;
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(Catalog {
            services: Vec::new(),
            updated_at: String::new(),
        })
    }
}

/// Save catalog to local JSON file.
pub fn save_catalog(catalog: &Catalog) -> Result<()> {
    let path = crate::config::paths::catalog_file()?;
    let content = serde_json::to_string(catalog)?;
    std::fs::write(&path, content)?;
    Ok(())
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

/// Search catalog by query string, optional category filter, and result limit.
pub fn search_catalog(
    catalog: &Catalog,
    query: &str,
    category: Option<&str>,
    limit: usize,
) -> SearchResult {
    let query_lower = query.to_lowercase();
    let terms: Vec<&str> = query_lower.split_whitespace().collect();

    let mut scored: Vec<(u32, &ApiService)> = catalog
        .services
        .iter()
        .filter_map(|svc| {
            // Category filter
            if let Some(cat) = category {
                if !svc.category.contains(cat) {
                    return None;
                }
            }

            // Score: how many terms match across title, description, keywords, org
            let searchable = format!(
                "{} {} {} {}",
                svc.title.to_lowercase(),
                svc.description.to_lowercase(),
                svc.keywords.join(" ").to_lowercase(),
                svc.org_name.to_lowercase(),
            );

            let match_count = terms.iter().filter(|t| searchable.contains(*t)).count();
            if match_count > 0 {
                let score = (match_count as u32) * 100 + svc.request_count;
                Some((score, svc))
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
        .map(|(_, svc)| SearchEntry {
            list_id: svc.list_id.clone(),
            title: svc.title.clone(),
            description: svc.description.clone(),
            org: svc.org_name.clone(),
            operations: svc.operations.iter().map(|o| o.name.clone()).collect(),
            auto_approve: svc.auto_approve,
            popularity: svc.request_count,
        })
        .collect();

    SearchResult { results, total }
}
