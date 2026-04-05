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

/// Current bundle schema version. Increment when Bundle/CatalogEntry fields change.
/// New variant in SpecStatus must be appended at the end (postcard varint ordering).
pub const CURRENT_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub metadata: BundleMetadata,
    pub catalog: Vec<CatalogEntry>,
    pub specs: HashMap<String, ApiSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMetadata {
    pub version: String,
    pub schema_version: u32,
    pub api_count: usize,
    pub spec_count: usize,
    pub checksum: String,
}

/// Spec availability status for a catalog entry.
/// WARNING: New variants must be appended at the end — postcard uses variant index ordering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SpecStatus {
    Available,
    Skeleton,
    HtmlOnly,
    External,
    CatalogOnly,
    Unsupported,
    PartialStub, // 반드시 끝에 — postcard varint 순서 보존
}

/// classify 함수의 입력 — named fields로 인자 순서 혼동 방지
#[derive(Debug, Default)]
pub struct ClassificationHints<'a> {
    pub has_spec: bool,
    pub is_skeleton: bool,
    pub endpoint_url: &'a str,
    pub is_link_api: bool,
    pub is_partial: bool,
}

impl SpecStatus {
    pub fn is_callable(&self) -> bool {
        matches!(self, Self::Available | Self::PartialStub)
    }

    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Available => "API spec 사용 가능",
            Self::Skeleton => "Swagger spec이 비어있습니다. 외부 서비스 페이지에서 확인하세요.",
            Self::HtmlOnly => "spec 파싱 준비 중입니다. endpoint URL을 참고하세요.",
            Self::External => "외부 포탈에서 제공하는 API입니다.",
            Self::CatalogOnly => "카탈로그 정보만 있습니다.",
            Self::Unsupported => "REST가 아닌 프로토콜(WMS/WFS 등)입니다.",
            Self::PartialStub => "일부 operation만 수집됨 — 존재하는 operation은 호출 가능, 누락분은 다음 업데이트에서 복구 예정",
        }
    }

    /// Classify spec status based on available data.
    pub fn classify(hints: &ClassificationHints) -> Self {
        if hints.is_link_api {
            return Self::External;
        }
        if hints.is_partial && hints.has_spec {
            return Self::PartialStub;
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
        // Has a data.go.kr/odcloud URL but no spec
        Self::HtmlOnly
    }
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
    pub endpoint_url: String,
    pub spec_status: SpecStatus,
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
    /// PartialStub API에서 수집 실패한 operation의 사람 읽을 이름.
    /// Available API에서는 항상 빈 벡터.
    /// WARNING: postcard varint 순서 보존을 위해 반드시 맨 마지막 필드여야 함.
    pub missing_operations: Vec<String>,
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
    pub spec_status: SpecStatus,
    pub endpoint_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_entry() -> CatalogEntry {
        CatalogEntry {
            list_id: "12345".into(),
            title: "Test API".into(),
            description: "desc".into(),
            keywords: vec!["test".into()],
            org_name: "org".into(),
            category: "cat".into(),
            request_count: 100,
            endpoint_url: "https://apis.data.go.kr/test".into(),
            spec_status: SpecStatus::Available,
        }
    }

    fn make_test_metadata() -> BundleMetadata {
        BundleMetadata {
            version: "2026-04-01".into(),
            schema_version: CURRENT_SCHEMA_VERSION,
            api_count: 1,
            spec_count: 0,
            checksum: "abc".into(),
        }
    }

    #[test]
    fn test_spec_status_is_callable() {
        assert!(SpecStatus::Available.is_callable());
        assert!(SpecStatus::PartialStub.is_callable());
        assert!(!SpecStatus::Skeleton.is_callable());
        assert!(!SpecStatus::HtmlOnly.is_callable());
        assert!(!SpecStatus::External.is_callable());
        assert!(!SpecStatus::CatalogOnly.is_callable());
        assert!(!SpecStatus::Unsupported.is_callable());
    }

    #[test]
    fn test_spec_status_user_message() {
        assert!(!SpecStatus::Available.user_message().is_empty());
        assert!(SpecStatus::Skeleton.user_message().contains("비어있습니다"));
        assert!(SpecStatus::External.user_message().contains("외부"));
        assert!(SpecStatus::Unsupported.user_message().contains("WMS"));
    }

    #[test]
    fn test_spec_status_postcard_roundtrip() {
        let statuses = [
            SpecStatus::Available,
            SpecStatus::Skeleton,
            SpecStatus::HtmlOnly,
            SpecStatus::External,
            SpecStatus::CatalogOnly,
            SpecStatus::Unsupported,
            SpecStatus::PartialStub,
        ];
        for status in &statuses {
            let bytes = postcard::to_allocvec(status).unwrap();
            let decoded: SpecStatus = postcard::from_bytes(&bytes).unwrap();
            assert_eq!(&decoded, status);
        }
    }

    #[test]
    fn test_bundle_postcard_roundtrip() {
        let bundle = Bundle {
            metadata: make_test_metadata(),
            catalog: vec![make_test_entry()],
            specs: HashMap::new(),
        };
        let bytes = postcard::to_allocvec(&bundle).unwrap();
        let decoded: Bundle = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.catalog.len(), 1);
        assert_eq!(decoded.catalog[0].title, "Test API");
        assert_eq!(
            decoded.catalog[0].endpoint_url,
            "https://apis.data.go.kr/test"
        );
        assert_eq!(decoded.catalog[0].spec_status, SpecStatus::Available);
        assert_eq!(decoded.metadata.version, "2026-04-01");
        assert_eq!(decoded.metadata.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_bundle_zstd_roundtrip() {
        let bundle = Bundle {
            metadata: make_test_metadata(),
            catalog: vec![],
            specs: HashMap::new(),
        };
        let bytes = postcard::to_allocvec(&bundle).unwrap();
        let compressed = zstd::encode_all(bytes.as_slice(), 3).unwrap();
        let decompressed = zstd::decode_all(compressed.as_slice()).unwrap();
        let decoded: Bundle = postcard::from_bytes(&decompressed).unwrap();
        assert_eq!(decoded.metadata.version, "2026-04-01");
        assert_eq!(decoded.metadata.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_old_schema_bundle_fails_deserialization() {
        // Simulate v1 bundle (no schema_version, no endpoint_url/spec_status)
        // by serializing a different struct layout
        #[derive(Serialize)]
        struct OldMetadata {
            version: String,
            api_count: usize,
            spec_count: usize,
            checksum: String,
        }
        #[derive(Serialize)]
        struct OldEntry {
            list_id: String,
            title: String,
            description: String,
            keywords: Vec<String>,
            org_name: String,
            category: String,
            request_count: u32,
        }
        #[derive(Serialize)]
        struct OldBundle {
            metadata: OldMetadata,
            catalog: Vec<OldEntry>,
            specs: HashMap<String, ApiSpec>,
        }

        let old = OldBundle {
            metadata: OldMetadata {
                version: "old".into(),
                api_count: 0,
                spec_count: 0,
                checksum: "".into(),
            },
            catalog: vec![],
            specs: HashMap::new(),
        };
        let bytes = postcard::to_allocvec(&old).unwrap();
        // Deserializing as new Bundle should fail (schema mismatch)
        let result = postcard::from_bytes::<Bundle>(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_classify_available() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                has_spec: true,
                endpoint_url: "https://apis.data.go.kr/x",
                ..Default::default()
            }),
            SpecStatus::Available,
        );
    }

    #[test]
    fn test_classify_skeleton() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                is_skeleton: true,
                endpoint_url: "https://apihub.kma.go.kr/x",
                ..Default::default()
            }),
            SpecStatus::Skeleton,
        );
    }

    #[test]
    fn test_classify_unsupported_wms() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                endpoint_url: "https://example.kr/wms/service",
                ..Default::default()
            }),
            SpecStatus::Unsupported,
        );
    }

    #[test]
    fn test_classify_html_only() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                endpoint_url: "https://apis.data.go.kr/1360000/Weather",
                ..Default::default()
            }),
            SpecStatus::HtmlOnly,
        );
    }

    #[test]
    fn test_classify_external() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                endpoint_url: "https://apihub.kma.go.kr/api/typ01",
                ..Default::default()
            }),
            SpecStatus::External,
        );
    }

    #[test]
    fn test_classify_catalog_only() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints::default()),
            SpecStatus::CatalogOnly,
        );
    }

    #[test]
    fn test_classify_odcloud_no_spec() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                endpoint_url: "https://api.odcloud.kr/api/test",
                ..Default::default()
            }),
            SpecStatus::HtmlOnly,
        );
    }

    #[test]
    fn test_classify_link_api_returns_external() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                endpoint_url: "https://apis.data.go.kr/1360000/Weather",
                is_link_api: true,
                ..Default::default()
            }),
            SpecStatus::External,
        );
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

    #[test]
    fn test_partial_stub_is_callable() {
        assert!(SpecStatus::PartialStub.is_callable());
    }

    #[test]
    fn test_partial_stub_user_message() {
        let msg = SpecStatus::PartialStub.user_message();
        assert!(msg.contains("일부"));
        assert!(msg.contains("operation"));
    }

    #[test]
    fn test_classify_partial_stub() {
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                has_spec: true,
                is_partial: true,
                endpoint_url: "https://apis.data.go.kr/x",
                ..Default::default()
            }),
            SpecStatus::PartialStub,
        );
    }

    #[test]
    fn test_classify_partial_stub_link_api_takes_priority() {
        // LINK API는 is_partial보다 우선
        assert_eq!(
            SpecStatus::classify(&ClassificationHints {
                has_spec: true,
                is_partial: true,
                is_link_api: true,
                endpoint_url: "https://apis.data.go.kr/x",
                ..Default::default()
            }),
            SpecStatus::External,
        );
    }

    #[test]
    fn test_partial_stub_postcard_roundtrip() {
        let status = SpecStatus::PartialStub;
        let bytes = postcard::to_allocvec(&status).unwrap();
        let decoded: SpecStatus = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, status);
    }

    #[test]
    fn test_partial_stub_postcard_index() {
        // PartialStub은 variant index 6이어야 함 (0-indexed, 끝에 추가)
        let bytes = postcard::to_allocvec(&SpecStatus::PartialStub).unwrap();
        assert_eq!(bytes[0], 6);
    }

    #[test]
    fn test_schema_v4_constant() {
        assert_eq!(CURRENT_SCHEMA_VERSION, 4);
    }

    fn make_test_spec(missing: Vec<String>) -> ApiSpec {
        ApiSpec {
            list_id: "15000001".into(),
            base_url: "https://apis.data.go.kr/test".into(),
            protocol: ApiProtocol::DataGoKrRest,
            auth: AuthMethod::None,
            extractor: ResponseExtractor {
                data_path: vec![],
                error_check: ErrorCheck::HttpStatus,
                pagination: None,
                format: ResponseFormat::Xml,
            },
            operations: vec![],
            fetched_at: "2026-04-05".into(),
            missing_operations: missing,
        }
    }

    #[test]
    fn test_missing_operations_serialization_roundtrip() {
        let spec = make_test_spec(vec!["getFcstVersion".into(), "getMidFcst".into()]);
        let bytes = postcard::to_allocvec(&spec).unwrap();
        let decoded: ApiSpec = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(
            decoded.missing_operations,
            vec!["getFcstVersion".to_string(), "getMidFcst".to_string()]
        );
    }

    #[test]
    fn test_missing_operations_empty_default_roundtrip() {
        // Available API의 기본값 (빈 벡터) 직렬화/역직렬화 검증
        let spec = make_test_spec(vec![]);
        let bytes = postcard::to_allocvec(&spec).unwrap();
        let decoded: ApiSpec = postcard::from_bytes(&bytes).unwrap();
        assert!(decoded.missing_operations.is_empty());
    }

    #[test]
    fn test_api_spec_is_last_field() {
        // postcard는 필드 선언 순서로 직렬화. missing_operations를 맨 마지막에 추가했는지 검증.
        // v3 bytes(missing_operations 없음)를 v4 struct로 역직렬화하면 trailing data 부족으로 실패해야 함.
        #[derive(serde::Serialize)]
        struct ApiSpecV3 {
            list_id: String,
            base_url: String,
            protocol: ApiProtocol,
            auth: AuthMethod,
            extractor: ResponseExtractor,
            operations: Vec<Operation>,
            fetched_at: String,
            // missing_operations 없음 — v3 스키마
        }
        let v3 = ApiSpecV3 {
            list_id: "x".into(),
            base_url: "x".into(),
            protocol: ApiProtocol::DataGoKrRest,
            auth: AuthMethod::None,
            extractor: ResponseExtractor {
                data_path: vec![],
                error_check: ErrorCheck::HttpStatus,
                pagination: None,
                format: ResponseFormat::Json,
            },
            operations: vec![],
            fetched_at: "x".into(),
        };
        // ApiSpec 자체를 직접 직렬화/역직렬화 → v3 bytes는 v4 struct가 기대하는 trailing field 부족
        let bytes = postcard::to_allocvec(&v3).unwrap();
        let result = postcard::from_bytes::<ApiSpec>(&bytes);
        assert!(
            result.is_err(),
            "v3 ApiSpec bytes는 v4 ApiSpec struct로 역직렬화 실패해야 함 (trailing bytes 부족)"
        );
    }
}
