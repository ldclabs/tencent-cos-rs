//! Error types returned by the SDK.

use reqwest::header::HeaderMap;
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error as ThisError;
use url::Url;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, ThisError)]
/// SDK error type.
pub enum Error {
    #[error("missing base URL for endpoint {0}")]
    MissingBaseUrl(&'static str),
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("authorization error: {0}")]
    Authorization(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("XML deserialize error: {0}")]
    XmlDeserialize(#[from] quick_xml::DeError),
    #[error("XML serialize error: {0}")]
    XmlSerialize(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Api(Box<ErrorResponse>),
    #[error("{0}")]
    Vector(Box<VectorErrorResponse>),
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
    pub method: Method,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub code: String,
    pub message: String,
    pub resource: String,
    pub request_id: String,
    pub trace_id: String,
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
#[serde(rename_all = "camelCase")]
pub struct VectorValidateField {
    pub message: String,
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
    pub method: Method,
    pub url: Url,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub code: String,
    pub message: String,
    pub field_list: Vec<VectorValidateField>,
    pub request_id: String,
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
