//! All shared data types for korea-cli.

use serde::{Deserialize, Serialize};

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
    pub operations: Vec<String>,
    pub auto_approve: bool,
    pub popularity: u32,
}
