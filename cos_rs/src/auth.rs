//! COS credential providers and Authorization V5 signing helpers.

use crate::encoding::{RequestOptions, safe_sign_encode};
use crate::error::{Error, Result};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::Method;
use reqwest::header::{HOST, HeaderMap, HeaderValue};
use sha1::{Digest, Sha1};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

type HmacSha1 = Hmac<Sha1>;

const SHA1_SIGN_ALGORITHM: &str = "sha1";
const DEFAULT_AUTH_EXPIRE: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, PartialEq, Eq)]
/// COS credential triplet.
pub struct Credential {
    /// Tencent Cloud SecretId.
    pub secret_id: String,
    /// Tencent Cloud SecretKey.
    pub secret_key: String,
    /// Optional STS session token for temporary credentials.
    pub session_token: Option<String>,
}

impl Credential {
    /// Create long-term credentials without a session token.
    pub fn new(secret_id: impl Into<String>, secret_key: impl Into<String>) -> Self {
        Self {
            secret_id: secret_id.into(),
            secret_key: secret_key.into(),
            session_token: None,
        }
    }

    /// Create temporary credentials with an STS session token.
    pub fn with_token(
        secret_id: impl Into<String>,
        secret_key: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            secret_id: secret_id.into(),
            secret_key: secret_key.into(),
            session_token: Some(token.into()),
        }
    }
}

#[async_trait]
/// Async source of COS credentials.
pub trait CredentialProvider: Send + Sync {
    /// Return the credential used to sign a request.
    async fn credential(&self) -> Result<Credential>;
}

pub type DynCredentialProvider = Arc<dyn CredentialProvider>;

#[derive(Debug, Clone)]
/// Credential provider that always returns the same credential.
pub struct StaticCredentialProvider {
    credential: Credential,
}

impl StaticCredentialProvider {
    /// Create a provider that clones and returns `credential` for every request.
    pub fn new(credential: Credential) -> Self {
        Self { credential }
    }
}

#[async_trait]
impl CredentialProvider for StaticCredentialProvider {
    async fn credential(&self) -> Result<Credential> {
        Ok(self.credential.clone())
    }
}

#[derive(Debug, Clone)]
/// Credential provider backed by environment variables.
///
/// Defaults:
/// - `COS_SECRETID`
/// - `COS_SECRETKEY`
/// - `COS_SESSION_TOKEN` (optional)
pub struct EnvCredentialProvider {
    secret_id_var: String,
    secret_key_var: String,
    token_var: String,
}

impl Default for EnvCredentialProvider {
    fn default() -> Self {
        Self {
            secret_id_var: "COS_SECRETID".to_owned(),
            secret_key_var: "COS_SECRETKEY".to_owned(),
            token_var: "COS_SESSION_TOKEN".to_owned(),
        }
    }
}

#[async_trait]
impl CredentialProvider for EnvCredentialProvider {
    async fn credential(&self) -> Result<Credential> {
        let secret_id = std::env::var(&self.secret_id_var)
            .map_err(|_| Error::Authorization(format!("{} is not set", self.secret_id_var)))?;
        let secret_key = std::env::var(&self.secret_key_var)
            .map_err(|_| Error::Authorization(format!("{} is not set", self.secret_key_var)))?;
        let session_token = std::env::var(&self.token_var)
            .ok()
            .filter(|v| !v.is_empty());
        Ok(Credential {
            secret_id,
            secret_key,
            session_token,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Signing time windows used by COS Authorization V5.
pub struct AuthTime {
    /// Inclusive Unix timestamp at which the Authorization header becomes valid.
    pub sign_start_time: i64,
    /// Unix timestamp at which the Authorization header expires.
    pub sign_end_time: i64,
    /// Inclusive Unix timestamp at which the derived signing key becomes valid.
    pub key_start_time: i64,
    /// Unix timestamp at which the derived signing key expires.
    pub key_end_time: i64,
}

impl AuthTime {
    /// Create a signing window starting at the current system time.
    pub fn new(expire: Duration) -> Self {
        let start = now_unix();
        let end = start + expire.as_secs() as i64;
        Self {
            sign_start_time: start,
            sign_end_time: end,
            key_start_time: start,
            key_end_time: end,
        }
    }

    /// Create a deterministic signing window for tests or presigned URLs.
    pub fn fixed(start: i64, end: i64) -> Self {
        Self {
            sign_start_time: start,
            sign_end_time: end,
            key_start_time: start,
            key_end_time: end,
        }
    }

    fn sign_string(&self) -> String {
        format!("{};{}", self.sign_start_time, self.sign_end_time)
    }

    fn key_string(&self) -> String {
        format!("{};{}", self.key_start_time, self.key_end_time)
    }
}

/// Add `Authorization` and optional session token headers to an existing map.
pub fn add_authorization_headers(
    credential: &Credential,
    method: &Method,
    url: &Url,
    headers: &mut HeaderMap,
    auth_time: Option<&AuthTime>,
    sign_host: bool,
) -> Result<()> {
    if credential.secret_id.starts_with(' ') || credential.secret_id.ends_with(' ') {
        return Err(Error::Authorization("SecretID is invalid".to_owned()));
    }
    if credential.secret_key.starts_with(' ') || credential.secret_key.ends_with(' ') {
        return Err(Error::Authorization("SecretKey is invalid".to_owned()));
    }
    if let Some(token) = &credential.session_token
        && !token.is_empty()
    {
        headers.insert(
            "x-cos-security-token",
            HeaderValue::from_str(token)
                .map_err(|e| Error::Authorization(format!("invalid session token: {e}")))?,
        );
    }
    let auth_time = auth_time
        .cloned()
        .unwrap_or_else(|| AuthTime::new(DEFAULT_AUTH_EXPIRE));
    let value = authorization(
        &credential.secret_id,
        &credential.secret_key,
        method,
        url,
        headers,
        &auth_time,
        sign_host,
    )?;
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&value)
            .map_err(|e| Error::Authorization(format!("invalid authorization header: {e}")))?,
    );
    Ok(())
}

/// Generate a COS Authorization V5 header value.
pub fn authorization(
    secret_id: &str,
    secret_key: &str,
    method: &Method,
    url: &Url,
    headers: &mut HeaderMap,
    auth_time: &AuthTime,
    sign_host: bool,
) -> Result<String> {
    if sign_host {
        let host = url
            .host_str()
            .ok_or_else(|| Error::Authorization("URL has no host".to_owned()))?;
        let host = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        };
        headers.insert(
            HOST,
            HeaderValue::from_str(&host)
                .map_err(|e| Error::Authorization(format!("invalid host header: {e}")))?,
        );
    }

    let sign_time = auth_time.sign_string();
    let key_time = auth_time.key_string();
    let sign_key = hmac_hex(secret_key.as_bytes(), key_time.as_bytes());
    let (format_headers, signed_header_list) = format_headers(headers);
    let (format_parameters, signed_parameter_list) = format_parameters(url);
    let format_string = format!(
        "{}\n{}\n{}\n{}\n",
        method.as_str().to_ascii_lowercase(),
        if url.path().is_empty() {
            "/"
        } else {
            url.path()
        },
        format_parameters,
        format_headers
    );
    let mut hasher = Sha1::new();
    hasher.update(format_string.as_bytes());
    let string_to_sign = format!(
        "{}\n{}\n{}\n",
        SHA1_SIGN_ALGORITHM,
        key_time,
        hex::encode(hasher.finalize())
    );
    let signature = hmac_hex(sign_key.as_bytes(), string_to_sign.as_bytes());
    Ok(format!(
        "q-sign-algorithm={}&q-ak={}&q-sign-time={}&q-key-time={}&q-header-list={}&q-url-param-list={}&q-signature={}",
        SHA1_SIGN_ALGORITHM,
        secret_id,
        sign_time,
        key_time,
        signed_header_list.join(";"),
        signed_parameter_list.join(";"),
        signature
    ))
}

pub(crate) async fn maybe_authorize(
    provider: Option<DynCredentialProvider>,
    method: &Method,
    url: &Url,
    options: &mut RequestOptions,
) -> Result<()> {
    if let Some(provider) = provider {
        let credential = provider.credential().await?;
        add_authorization_headers(&credential, method, url, &mut options.headers, None, true)?;
    }
    Ok(())
}

fn format_parameters(url: &Url) -> (String, Vec<String>) {
    let mut pairs = Vec::<(String, String)>::new();
    let mut signed = Vec::<String>::new();
    for (key, value) in url.query_pairs() {
        let key = safe_sign_encode(key.as_ref()).to_ascii_lowercase();
        let value = safe_sign_encode(value.as_ref());
        signed.push(key.clone());
        pairs.push((key, value));
    }
    signed.sort();
    pairs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let formatted = pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    (formatted, signed)
}

fn format_headers(headers: &HeaderMap) -> (String, Vec<String>) {
    let mut pairs = Vec::<(String, String)>::new();
    let mut signed = Vec::<String>::new();
    for (key, value) in headers {
        let key = key.as_str().to_ascii_lowercase();
        if is_sign_header(&key) {
            let encoded_key = safe_sign_encode(&key).to_ascii_lowercase();
            let encoded_value = safe_sign_encode(value.to_str().unwrap_or_default());
            signed.push(encoded_key.clone());
            pairs.push((encoded_key, encoded_value));
        }
    }
    signed.sort();
    pairs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let formatted = pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    (formatted, signed)
}

fn is_sign_header(key: &str) -> bool {
    if key.starts_with("x-cos-") || key.starts_with("x-ci-") {
        return true;
    }
    matches!(
        key,
        "host"
            | "range"
            | "cache-control"
            | "content-disposition"
            | "content-encoding"
            | "content-type"
            | "content-length"
            | "content-md5"
            | "transfer-encoding"
            | "expect"
            | "expires"
            | "if-match"
            | "if-modified-since"
            | "if-none-match"
            | "if-unmodified-since"
            | "origin"
            | "access-control-request-method"
            | "access-control-request-headers"
            | "pic-operations"
    )
}

fn hmac_hex(key: &[u8], msg: &[u8]) -> String {
    let mut mac = <HmacSha1 as hmac::digest::KeyInit>::new_from_slice(key)
        .expect("HMAC accepts any key size");
    mac.update(msg);
    hex::encode(mac.finalize().into_bytes())
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_matches_go_fixture() {
        let expect = "q-sign-algorithm=sha1&q-ak=QmFzZTY0IGlzIGEgZ2VuZXJp&q-sign-time=1480932292;1481012292&q-key-time=1480932292;1481012292&q-header-list=host;x-cos-content-sha1;x-cos-stroage-class&q-url-param-list=&q-signature=ce4ac0ecbcdb30538b3fee0a97cc6389694ce53a";
        let mut headers = HeaderMap::new();
        headers.insert(
            HOST,
            HeaderValue::from_static("testbucket-125000000.cos.ap-guangzhou.myqcloud.com"),
        );
        headers.insert(
            "x-cos-content-sha1",
            HeaderValue::from_static("db8ac1c259eb89d4a131b253bacfca5f319d54f2"),
        );
        headers.insert("x-cos-stroage-class", HeaderValue::from_static("nearline"));
        let url = Url::parse("http://testbucket-125000000.cos.ap-guangzhou.myqcloud.com/testfile2")
            .unwrap();
        let auth = authorization(
            "QmFzZTY0IGlzIGEgZ2VuZXJp",
            "AKIDZfbOA78asKUYBcXFrJD0a1ICvR98JM",
            &Method::PUT,
            &url,
            &mut headers,
            &AuthTime::fixed(1480932292, 1481012292),
            true,
        )
        .unwrap();
        assert_eq!(auth, expect);
    }

    #[test]
    fn add_authorization_headers_rejects_space_padded_credentials() {
        let url = Url::parse("https://example-1250000000.cos.ap-guangzhou.myqcloud.com/").unwrap();
        let mut headers = HeaderMap::new();
        let err = add_authorization_headers(
            &Credential::new(" test", "sk"),
            &Method::GET,
            &url,
            &mut headers,
            Some(&AuthTime::fixed(1, 2)),
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("SecretID is invalid"));

        let err = add_authorization_headers(
            &Credential::new("ak", "sk "),
            &Method::GET,
            &url,
            &mut headers,
            Some(&AuthTime::fixed(1, 2)),
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("SecretKey is invalid"));
    }

    #[test]
    fn add_authorization_headers_adds_session_token_and_host() {
        let url =
            Url::parse("https://example-1250000000.cos.ap-guangzhou.myqcloud.com/test").unwrap();
        let mut headers = HeaderMap::new();

        add_authorization_headers(
            &Credential::with_token("ak", "sk", "token"),
            &Method::GET,
            &url,
            &mut headers,
            Some(&AuthTime::fixed(1, 2)),
            true,
        )
        .unwrap();

        assert_eq!(
            headers.get(HOST).unwrap(),
            "example-1250000000.cos.ap-guangzhou.myqcloud.com"
        );
        assert_eq!(headers.get("x-cos-security-token").unwrap(), "token");
        let authorization = headers.get("authorization").unwrap().to_str().unwrap();
        assert!(authorization.contains("q-ak=ak"));
        assert!(authorization.contains("q-header-list=host;x-cos-security-token"));
    }
}
