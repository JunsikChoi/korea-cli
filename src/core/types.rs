//! All shared data types for korea-cli.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Catalog types (lightweight, for search) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub services: Vec<ApiService>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiService {
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub org_name: String,
    pub category: String,
    pub endpoint_url: String,
    pub data_format: String,
    pub auto_approve: bool,
    pub is_free: bool,
    pub request_count: u32,
    pub updated_at: String,
    pub operations: Vec<OperationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationSummary {
    pub id: String,
    pub name: String,
    pub request_params: Vec<String>,
    pub request_params_en: Vec<String>,
}

// ── Bundle types (pre-collected data for offline use) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub metadata: BundleMetadata,
    pub catalog: Vec<CatalogEntry>,
    pub specs: HashMap<String, ApiSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMetadata {
    pub version: String,
    pub api_count: usize,
    pub spec_count: usize,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub org_name: String,
    pub category: String,
    pub request_count: u32,
}

// ── API Spec types (detailed, from Swagger) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiProtocol {
    InfuserRest,
    DataGoKrRest,
    ExternalRest,
    Soap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSpec {
    pub list_id: String,
    pub base_url: String,
    pub protocol: ApiProtocol,
    pub auth: AuthMethod,
    pub extractor: ResponseExtractor,
    pub operations: Vec<Operation>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    QueryParam {
        name: String,
    },
    Header {
        name: String,
        prefix: String,
    },
    Both {
        query: String,
        header_name: String,
        header_prefix: String,
        prefer: AuthPreference,
    },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthPreference {
    Query,
    Header,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseExtractor {
    pub data_path: Vec<String>,
    pub error_check: ErrorCheck,
    pub pagination: Option<PaginationStyle>,
    pub format: ResponseFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCheck {
    HttpStatus,
    FieldEquals {
        path: Vec<String>,
        success_value: String,
        message_path: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaginationStyle {
    PagePerPage { page: String, per_page: String },
    NumOfRowsPageNo { rows: String, page_no: String },
    CursorBased { cursor_field: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseFormat {
    Json,
    Xml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub path: String,
    pub method: HttpMethod,
    pub summary: String,
    pub content_type: ContentType,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    pub response_fields: Vec<ResponseField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
    Json,
    Xml,
    FormUrlEncoded,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub description: String,
    pub location: ParamLocation,
    pub param_type: String,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamLocation {
    Query,
    Path,
    Header,
    Body,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    pub content_type: ContentType,
    pub fields: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseField {
    pub name: String,
    pub description: String,
    pub field_type: String,
}

// ── API response types ──

#[derive(Debug, Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
    pub message: Option<String>,
    pub action: Option<String>,
    pub raw_status: Option<u16>,
    pub metadata: Option<serde_json::Value>,
}

// ── Search result types ──

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub results: Vec<SearchEntry>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct SearchEntry {
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub org: String,
    pub category: String,
    pub popularity: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_postcard_roundtrip() {
        let entry = CatalogEntry {
            list_id: "12345".into(),
            title: "Test API".into(),
            description: "desc".into(),
            keywords: vec!["test".into()],
            org_name: "org".into(),
            category: "cat".into(),
            request_count: 100,
        };
        let bundle = Bundle {
            metadata: BundleMetadata {
                version: "2026-03-31".into(),
                api_count: 1,
                spec_count: 0,
                checksum: "abc".into(),
            },
            catalog: vec![entry],
            specs: HashMap::new(),
        };
        let bytes = postcard::to_allocvec(&bundle).unwrap();
        let decoded: Bundle = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.catalog.len(), 1);
        assert_eq!(decoded.catalog[0].title, "Test API");
        assert_eq!(decoded.metadata.version, "2026-03-31");
    }

    #[test]
    fn test_bundle_zstd_roundtrip() {
        let bundle = Bundle {
            metadata: BundleMetadata {
                version: "test".into(),
                api_count: 0,
                spec_count: 0,
                checksum: "".into(),
            },
            catalog: vec![],
            specs: HashMap::new(),
        };
        let bytes = postcard::to_allocvec(&bundle).unwrap();
        let compressed = zstd::encode_all(bytes.as_slice(), 3).unwrap();
        let decompressed = zstd::decode_all(compressed.as_slice()).unwrap();
        let decoded: Bundle = postcard::from_bytes(&decompressed).unwrap();
        assert_eq!(decoded.metadata.version, "test");
    }
}
