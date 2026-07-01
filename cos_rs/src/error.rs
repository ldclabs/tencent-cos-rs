//! Error types returned by the SDK.

use reqwest::header::HeaderMap;
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error as ThisError;
use url::Url;

/// SDK result alias.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, ThisError)]
/// SDK error type.
pub enum Error {
    /// No base URL has been configured for the requested endpoint family.
    #[error("missing base URL for endpoint {0}")]
    MissingBaseUrl(&'static str),
    /// A configured base URL is malformed.
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(String),
    /// Caller supplied invalid SDK input.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Credentials or signature generation failed.
    #[error("authorization error: {0}")]
    Authorization(String),
    /// HTTP transport error from `reqwest`.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    /// URL parsing failed.
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    /// COS XML response deserialization failed.
    #[error("XML deserialize error: {0}")]
    XmlDeserialize(#[from] quick_xml::DeError),
    /// COS XML request serialization failed.
    #[error("XML serialize error: {0}")]
    XmlSerialize(String),
    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// Local file I/O failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Non-success COS XML API response.
    #[error("{0}")]
    Api(Box<ErrorResponse>),
    /// Non-success Vector JSON API response.
    #[error("{0}")]
    Vector(Box<VectorErrorResponse>),
    /// Client-side crypto or KMS operation failed.
    #[error("crypto error: {0}")]
    Crypto(String),
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename = "Error", rename_all = "PascalCase")]
pub struct ErrorBody {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub resource: String,
    #[serde(default, rename = "RequestId")]
    pub request_id: String,
    #[serde(default, rename = "TraceId")]
    pub trace_id: String,
}

#[derive(Debug, Clone)]
/// COS XML API error response.
pub struct ErrorResponse {
    /// HTTP method used for the failed request.
    pub method: Method,
    /// Final request URL.
    pub url: Url,
    /// HTTP status returned by COS.
    pub status: StatusCode,
    /// Response headers returned by COS.
    pub headers: HeaderMap,
    /// COS error code.
    pub code: String,
    /// COS error message.
    pub message: String,
    /// Resource name reported by COS.
    pub resource: String,
    /// COS request id, from XML body or `x-cos-request-id`.
    pub request_id: String,
    /// COS trace id, from XML body or `x-cos-trace-id`.
    pub trace_id: String,
    /// Raw error response body.
    pub body: bytes::Bytes,
}

impl ErrorResponse {
    pub(crate) fn from_response(
        method: Method,
        url: Url,
        status: StatusCode,
        headers: HeaderMap,
        body: bytes::Bytes,
    ) -> Self {
        let parsed = quick_xml::de::from_str::<ErrorBody>(std::str::from_utf8(&body).unwrap_or(""))
            .unwrap_or_default();
        let request_id = first_non_empty(
            parsed.request_id,
            header_str(&headers, "x-cos-request-id").unwrap_or_default(),
        );
        let trace_id = first_non_empty(
            parsed.trace_id,
            header_str(&headers, "x-cos-trace-id").unwrap_or_default(),
        );
        Self {
            method,
            url,
            status,
            headers,
            code: parsed.code,
            message: parsed.message,
            resource: parsed.resource,
            request_id,
            trace_id,
            body,
        }
    }

    /// Return whether the response status is `404 Not Found`.
    pub fn is_not_found(&self) -> bool {
        self.status == StatusCode::NOT_FOUND
    }
}

impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {} {}(Message: {}, RequestId: {}, TraceId: {})",
            self.method,
            self.url,
            self.status.as_u16(),
            self.code,
            self.message,
            self.request_id,
            self.trace_id
        )
    }
}

impl std::error::Error for ErrorResponse {}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Field-level validation error returned by the Vector JSON API.
#[serde(rename_all = "camelCase")]
pub struct VectorValidateField {
    /// Field-level validation error message.
    pub message: String,
    /// JSON path or field path reported by Vector.
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VectorBody {
    #[serde(default)]
    message: String,
    #[serde(default)]
    field_list: Vec<VectorValidateField>,
}

#[derive(Debug, Clone)]
/// Vector JSON API error response.
pub struct VectorErrorResponse {
    /// HTTP method used for the failed request.
    pub method: Method,
    /// Final request URL.
    pub url: Url,
    /// HTTP status returned by Vector.
    pub status: StatusCode,
    /// Response headers returned by Vector.
    pub headers: HeaderMap,
    /// Vector error code, usually from `x-cos-error-code`.
    pub code: String,
    /// Vector error message.
    pub message: String,
    /// Field-level validation errors returned by Vector.
    pub field_list: Vec<VectorValidateField>,
    /// COS request id, usually from `x-cos-request-id`.
    pub request_id: String,
    /// Raw error response body.
    pub body: bytes::Bytes,
}

impl VectorErrorResponse {
    pub(crate) fn from_response(
        method: Method,
        url: Url,
        status: StatusCode,
        headers: HeaderMap,
        body: bytes::Bytes,
    ) -> Self {
        let parsed = serde_json::from_slice::<VectorBody>(&body).unwrap_or(VectorBody {
            message: String::from_utf8_lossy(&body).into_owned(),
            field_list: Vec::new(),
        });
        Self {
            method,
            url,
            status,
            code: header_str(&headers, "x-cos-error-code").unwrap_or_default(),
            request_id: header_str(&headers, "x-cos-request-id").unwrap_or_default(),
            headers,
            message: parsed.message,
            field_list: parsed.field_list,
            body,
        }
    }
}

impl std::fmt::Display for VectorErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {} {}(Message: {}, RequestId: {})",
            self.method,
            self.url,
            self.status.as_u16(),
            self.code,
            self.message,
            self.request_id
        )?;
        for field in &self.field_list {
            write!(f, ", Field({}: {})", field.path, field.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for VectorErrorResponse {}

pub(crate) fn header_str(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
}

fn first_non_empty(a: String, b: String) -> String {
    if a.is_empty() { b } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use reqwest::header::HeaderValue;

    #[test]
    fn error_response_parses_cos_xml_body_like_go_sdk() {
        let headers = HeaderMap::new();
        let body = Bytes::from_static(
            br#"<?xml version='1.0' encoding='utf-8' ?>
<Error>
    <Code>BucketAlreadyExists</Code>
    <Message>The requested bucket name is not available.</Message>
    <Resource>testdelete-1253846586.cos.ap-guangzhou.myqcloud.com</Resource>
    <RequestId>NTk0NTRjZjZfNTViMjM1XzlkMV9hZTZh</RequestId>
    <TraceId>trace-1</TraceId>
</Error>"#,
        );

        let err = ErrorResponse::from_response(
            Method::GET,
            Url::parse("https://service.cos.myqcloud.com/test_409").unwrap(),
            StatusCode::CONFLICT,
            headers,
            body,
        );

        assert_eq!(err.code, "BucketAlreadyExists");
        assert_eq!(err.message, "The requested bucket name is not available.");
        assert_eq!(err.request_id, "NTk0NTRjZjZfNTViMjM1XzlkMV9hZTZh");
        assert_eq!(err.trace_id, "trace-1");
        assert!(
            err.to_string()
                .contains("GET https://service.cos.myqcloud.com/test_409: 409")
        );
    }

    #[test]
    fn error_response_falls_back_to_request_id_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-cos-request-id",
            HeaderValue::from_static("header-request-id"),
        );
        headers.insert(
            "x-cos-trace-id",
            HeaderValue::from_static("header-trace-id"),
        );
        let body = Bytes::from_static(
            br#"<Error><Code>NoSuchKey</Code><Message>The specified key does not exist.</Message></Error>"#,
        );

        let err = ErrorResponse::from_response(
            Method::HEAD,
            Url::parse("https://example-1250000000.cos.ap-guangzhou.myqcloud.com/test_404")
                .unwrap(),
            StatusCode::NOT_FOUND,
            headers,
            body,
        );

        assert!(err.is_not_found());
        assert_eq!(err.code, "NoSuchKey");
        assert_eq!(err.request_id, "header-request-id");
        assert_eq!(err.trace_id, "header-trace-id");
    }

    #[test]
    fn vector_error_response_parses_json_body_and_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-cos-error-code",
            HeaderValue::from_static("ValidationException"),
        );
        headers.insert("x-cos-request-id", HeaderValue::from_static("req-123"));
        let body = Bytes::from_static(
            br#"{
                "message": "VectorBucketName is invalid",
                "fieldList": [
                    {
                        "message": "VectorBucketName should match pattern",
                        "path": "/vectorBucketName"
                    }
                ]
            }"#,
        );

        let err = VectorErrorResponse::from_response(
            Method::POST,
            Url::parse("https://vectors.ap-guangzhou.coslake.com/CreateVectorBucket").unwrap(),
            StatusCode::BAD_REQUEST,
            headers,
            body,
        );

        assert_eq!(err.code, "ValidationException");
        assert_eq!(err.request_id, "req-123");
        assert_eq!(err.message, "VectorBucketName is invalid");
        assert_eq!(err.field_list[0].path, "/vectorBucketName");
        assert!(
            err.to_string()
                .contains("Field(/vectorBucketName: VectorBucketName should match pattern)")
        );
    }
}
