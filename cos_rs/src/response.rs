//! Owned HTTP response wrapper returned by SDK operations.

use bytes::Bytes;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use url::Url;

#[derive(Debug, Clone)]
/// HTTP response with status, headers, final URL, and fully-buffered body.
pub struct Response {
    /// HTTP status returned by COS.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Fully buffered response body.
    pub body: Bytes,
    /// Final request URL.
    pub url: Url,
}

impl Response {
    /// Numeric HTTP status code.
    pub fn status_code(&self) -> u16 {
        self.status.as_u16()
    }

    /// Whether the HTTP status is in the 2xx range.
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Borrow the response body bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// Consume the response and return body bytes.
    pub fn into_bytes(self) -> Bytes {
        self.body
    }

    /// Interpret the response body as UTF-8 text.
    pub fn text(&self) -> std::result::Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }

    /// Read a response header as UTF-8.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}
