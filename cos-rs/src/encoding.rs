//! Request option helpers and COS URI encoding utilities.

use crate::error::{Error, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use url::Url;

#[derive(Debug, Clone, Default)]
/// Accumulated query parameters, raw subresources, and headers for a request.
pub struct RequestOptions {
    pub query: Vec<(String, String)>,
    pub raw_query: Vec<String>,
    pub headers: HeaderMap,
}

impl RequestOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn query(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.query.push((key.into(), value.to_string()));
        self
    }

    pub fn raw_query(mut self, value: impl Into<String>) -> Self {
        let value = value.into();
        if !value.is_empty() {
            self.raw_query.push(value);
        }
        self
    }

    pub fn header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self> {
        insert_header(&mut self.headers, key.as_ref(), value.as_ref())?;
        Ok(self)
    }

    pub fn merge(mut self, other: RequestOptions) -> Self {
        self.query.extend(other.query);
        self.raw_query.extend(other.raw_query);
        self.headers.extend(other.headers);
        self
    }
}

pub trait QueryOptions {
    /// Apply query parameters into a request option bag.
    fn apply_query(&self, options: &mut RequestOptions) -> Result<()>;
}

pub trait HeaderOptions {
    /// Apply headers into a request option bag.
    fn apply_headers(&self, options: &mut RequestOptions) -> Result<()>;
}

impl QueryOptions for RequestOptions {
    fn apply_query(&self, options: &mut RequestOptions) -> Result<()> {
        options.query.extend(self.query.clone());
        options.raw_query.extend(self.raw_query.clone());
        Ok(())
    }
}

impl HeaderOptions for RequestOptions {
    fn apply_headers(&self, options: &mut RequestOptions) -> Result<()> {
        options.headers.extend(self.headers.clone());
        Ok(())
    }
}

impl QueryOptions for () {
    fn apply_query(&self, _options: &mut RequestOptions) -> Result<()> {
        Ok(())
    }
}

impl HeaderOptions for () {
    fn apply_headers(&self, _options: &mut RequestOptions) -> Result<()> {
        Ok(())
    }
}

pub fn query_from_serialize<T: Serialize>(value: &T) -> Result<Vec<(String, String)>> {
    let encoded = serde_urlencoded::to_string(value)
        .map_err(|e| Error::InvalidInput(format!("query serialize failed: {e}")))?;
    let pairs = url::form_urlencoded::parse(encoded.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    Ok(pairs)
}

pub fn append_query(url: &mut Url, options: &RequestOptions) {
    let mut fragments = Vec::new();
    if let Some(existing) = url.query()
        && !existing.is_empty()
    {
        fragments.push(existing.to_owned());
    }
    fragments.extend(options.raw_query.iter().cloned());
    if !options.query.is_empty() {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in &options.query {
            serializer.append_pair(key, value);
        }
        let encoded = serializer.finish();
        if !encoded.is_empty() {
            fragments.push(encoded);
        }
    }
    if fragments.is_empty() {
        url.set_query(None);
    } else {
        url.set_query(Some(&fragments.join("&")));
    }
}

/// Percent-encode an object key.
pub fn encode_key(key: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(key.len());
    for &b in key.as_bytes() {
        if keep_slash && b == b'/' {
            out.push('/');
        } else {
            push_encoded(&mut out, b);
        }
    }
    out
}

/// Percent-encode a single URL component using COS-compatible escaping.
pub fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &b in value.as_bytes() {
        push_encoded(&mut out, b);
    }
    out
}

pub(crate) fn safe_sign_encode(value: &str) -> String {
    encode_component(value)
}

fn push_encoded(out: &mut String, b: u8) {
    if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
        out.push(b as char);
    } else {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        out.push('%');
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
}

pub(crate) fn insert_header(headers: &mut HeaderMap, key: &str, value: &str) -> Result<()> {
    let name = HeaderName::from_bytes(key.as_bytes())
        .map_err(|e| Error::InvalidInput(format!("invalid header name {key}: {e}")))?;
    let value = HeaderValue::from_str(value)
        .map_err(|e| Error::InvalidInput(format!("invalid header value for {key}: {e}")))?;
    headers.insert(name, value);
    Ok(())
}

/// Go SDK compatibility helper for optional booleans.
#[allow(non_snake_case)]
pub fn Bool(value: bool) -> Option<bool> {
    Some(value)
}
