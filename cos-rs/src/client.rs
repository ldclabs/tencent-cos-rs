//! Client construction, endpoint configuration, retry settings, and the shared
//! HTTP request pipeline used by every service module.

use crate::USER_AGENT;
use crate::auth::{Credential, DynCredentialProvider, StaticCredentialProvider, maybe_authorize};
use crate::encoding::{RequestOptions, append_query, insert_header};
use crate::error::{Error, ErrorResponse, Result, VectorErrorResponse};
use crate::response::Response;
use base64::Engine;
use bytes::Bytes;
use md5::{Digest, Md5};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT as UA};
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

const DEFAULT_SERVICE_BASE_URL: &str = "http://service.cos.myqcloud.com";
const CONTENT_TYPE_XML: &str = "application/xml";
const CONTENT_TYPE_JSON: &str = "application/json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Logical COS endpoint family used when choosing a base URL.
pub enum Endpoint {
    Bucket,
    Service,
    Batch,
    Ci,
    Fetch,
    MetaInsight,
    Vector,
}

impl Endpoint {
    fn name(self) -> &'static str {
        match self {
            Endpoint::Bucket => "bucket",
            Endpoint::Service => "service",
            Endpoint::Batch => "batch",
            Endpoint::Ci => "ci",
            Endpoint::Fetch => "fetch",
            Endpoint::MetaInsight => "metainsight",
            Endpoint::Vector => "vector",
        }
    }
}

#[derive(Debug, Clone, Default)]
/// Base URLs for each COS service family.
///
/// `BaseUrl::new()` initializes the default Service endpoint. Bucket, Batch,
/// CI, MetaInsight, and Vector URLs are optional because most applications only
/// use a subset of COS features.
pub struct BaseUrl {
    pub bucket: Option<Url>,
    pub service: Option<Url>,
    pub batch: Option<Url>,
    pub ci: Option<Url>,
    pub fetch: Option<Url>,
    pub meta_insight: Option<Url>,
    pub vector: Option<Url>,
}

impl BaseUrl {
    /// Create an empty endpoint set with the default Service URL populated.
    pub fn new() -> Self {
        Self {
            service: Some(Url::parse(DEFAULT_SERVICE_BASE_URL).expect("valid service URL")),
            ..Self::default()
        }
    }

    /// Build a standard bucket endpoint URL.
    ///
    /// `bucket_name` must be in COS `{name}-{appid}` format.
    pub fn bucket_url(bucket_name: &str, region: &str, secure: bool) -> Result<Url> {
        if bucket_name.is_empty() || !bucket_name.contains('-') {
            return Err(Error::InvalidInput(format!(
                "bucketName[{bucket_name}] is invalid"
            )));
        }
        if region.is_empty()
            || !region
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(Error::InvalidInput(format!("region[{region}] is invalid")));
        }
        let scheme = if secure { "https" } else { "http" };
        Ok(Url::parse(&format!(
            "{scheme}://{bucket_name}.cos.{region}.myqcloud.com"
        ))?)
    }

    /// Build a public Vector endpoint URL for a region.
    pub fn vector_url(region: &str, secure: bool) -> Result<Url> {
        if region.is_empty() {
            return Err(Error::InvalidInput("region is required".to_owned()));
        }
        let scheme = if secure { "https" } else { "http" };
        Ok(Url::parse(&format!(
            "{scheme}://vectors.{region}.coslake.com"
        ))?)
    }

    /// Build an internal-network Vector endpoint URL for a region.
    pub fn vector_internal_url(region: &str, secure: bool) -> Result<Url> {
        if region.is_empty() {
            return Err(Error::InvalidInput("region is required".to_owned()));
        }
        let scheme = if secure { "https" } else { "http" };
        Ok(Url::parse(&format!(
            "{scheme}://vectors.{region}.internal.tencentcos.com"
        ))?)
    }

    /// Normalize a custom Vector endpoint, adding `https://` when omitted.
    pub fn vector_endpoint_url(endpoint: &str) -> Result<Url> {
        if endpoint.is_empty() {
            return Err(Error::InvalidInput("endpoint is required".to_owned()));
        }
        let endpoint = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            endpoint.to_owned()
        } else {
            format!("https://{endpoint}")
        };
        Ok(Url::parse(&endpoint)?)
    }

    fn endpoint(&self, endpoint: Endpoint) -> Result<Url> {
        let url = match endpoint {
            Endpoint::Bucket => self.bucket.clone(),
            Endpoint::Service => self.service.clone(),
            Endpoint::Batch => self.batch.clone(),
            Endpoint::Ci => self.ci.clone(),
            Endpoint::Fetch => self.fetch.clone(),
            Endpoint::MetaInsight => self.meta_insight.clone(),
            Endpoint::Vector => self.vector.clone(),
        };
        url.ok_or_else(|| Error::MissingBaseUrl(endpoint.name()))
    }
}

#[derive(Debug, Clone)]
/// Retry behavior shared by COS XML and Vector JSON requests.
pub struct RetryOptions {
    pub count: usize,
    pub interval: Duration,
    pub auto_switch_host: bool,
}

impl Default for RetryOptions {
    fn default() -> Self {
        Self {
            count: 3,
            interval: Duration::ZERO,
            auto_switch_host: false,
        }
    }
}

#[derive(Debug, Clone)]
/// Client-wide behavior flags.
pub struct Config {
    pub enable_crc: bool,
    pub request_body_close: bool,
    pub retry: RetryOptions,
    pub object_key_simplify_check: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enable_crc: true,
            request_body_close: false,
            retry: RetryOptions::default(),
            object_key_simplify_check: true,
        }
    }
}

#[derive(Clone)]
/// Async COS client.
///
/// The client owns an underlying `reqwest::Client`, endpoint configuration,
/// retry settings, and optional credentials. Use service accessors such as
/// [`Client::bucket`], [`Client::object`], and [`Client::vector`] to call APIs.
pub struct Client {
    pub(crate) inner: Arc<ClientInner>,
}

pub(crate) struct ClientInner {
    pub http: reqwest::Client,
    pub base_url: BaseUrl,
    pub config: Config,
    pub user_agent: String,
    pub host: Option<String>,
    pub credential_provider: Option<DynCredentialProvider>,
}

/// Builder for [`Client`].
pub struct ClientBuilder {
    http: Option<reqwest::Client>,
    base_url: BaseUrl,
    config: Config,
    user_agent: String,
    host: Option<String>,
    credential_provider: Option<DynCredentialProvider>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            http: None,
            base_url: BaseUrl::new(),
            config: Config::default(),
            user_agent: USER_AGENT.to_owned(),
            host: None,
            credential_provider: None,
        }
    }
}

impl ClientBuilder {
    /// Create a builder with default config and no bucket endpoint.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a caller-provided `reqwest::Client`.
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    /// Replace all configured base URLs.
    pub fn base_url(mut self, base_url: BaseUrl) -> Self {
        self.base_url = base_url;
        self
    }

    /// Set the bucket endpoint.
    pub fn bucket_url(mut self, url: Url) -> Self {
        self.base_url.bucket = Some(url);
        self
    }

    /// Set the Service endpoint.
    pub fn service_url(mut self, url: Url) -> Self {
        self.base_url.service = Some(url);
        self
    }

    /// Set the Batch endpoint.
    pub fn batch_url(mut self, url: Url) -> Self {
        self.base_url.batch = Some(url);
        self
    }

    /// Set the CI endpoint.
    pub fn ci_url(mut self, url: Url) -> Self {
        self.base_url.ci = Some(url);
        self
    }

    /// Set the Fetch Task endpoint.
    pub fn fetch_url(mut self, url: Url) -> Self {
        self.base_url.fetch = Some(url);
        self
    }

    /// Set the MetaInsight endpoint.
    pub fn meta_insight_url(mut self, url: Url) -> Self {
        self.base_url.meta_insight = Some(url);
        self
    }

    /// Set the Vector endpoint.
    pub fn vector_url(mut self, url: Url) -> Self {
        self.base_url.vector = Some(url);
        self
    }

    /// Replace client configuration.
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Override the default SDK user agent.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Override the HTTP Host header.
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Use static COS credentials.
    pub fn credential(mut self, credential: Credential) -> Self {
        self.credential_provider = Some(Arc::new(StaticCredentialProvider::new(credential)));
        self
    }

    /// Use a custom async credential provider.
    pub fn credential_provider(mut self, provider: DynCredentialProvider) -> Self {
        self.credential_provider = Some(provider);
        self
    }

    /// Build the async client.
    pub fn build(self) -> Result<Client> {
        let http = match self.http {
            Some(http) => http,
            None => reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        };
        Ok(Client {
            inner: Arc::new(ClientInner {
                http,
                base_url: self.base_url,
                config: self.config,
                user_agent: self.user_agent,
                host: self.host,
                credential_provider: self.credential_provider,
            }),
        })
    }
}

impl Client {
    /// Create a [`ClientBuilder`].
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Build a client from base URLs with default configuration.
    pub fn new(base_url: BaseUrl) -> Result<Self> {
        Self::builder().base_url(base_url).build()
    }

    /// Return configured base URLs.
    pub fn base_url(&self) -> &BaseUrl {
        &self.inner.base_url
    }

    /// Return client configuration.
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Service API entry point.
    pub fn service(&self) -> crate::service::ServiceService {
        crate::service::ServiceService::new(self.clone())
    }

    /// Bucket API entry point.
    pub fn bucket(&self) -> crate::bucket::BucketService {
        crate::bucket::BucketService::new(self.clone())
    }

    /// Object API entry point.
    pub fn object(&self) -> crate::object::ObjectService {
        crate::object::ObjectService::new(self.clone())
    }

    /// Batch API entry point.
    pub fn batch(&self) -> crate::batch::BatchService {
        crate::batch::BatchService::new(self.clone())
    }

    /// CI API entry point.
    pub fn ci(&self) -> crate::ci::CiService {
        crate::ci::CiService::new(self.clone())
    }

    /// MetaInsight API entry point.
    pub fn meta_insight(&self) -> crate::metainsight::MetaInsightService {
        crate::metainsight::MetaInsightService::new(self.clone())
    }

    /// Vector API entry point.
    pub fn vector(&self) -> crate::vector::VectorService {
        crate::vector::VectorService::new(self.clone())
    }

    /// Client-side crypto entry point.
    pub fn crypto<M>(&self, master: M) -> crate::crypto::CryptoClient<M>
    where
        M: crate::crypto::MasterCipher,
    {
        crate::crypto::CryptoClient::new(self.clone(), master)
    }

    pub(crate) async fn send(
        &self,
        endpoint: Endpoint,
        method: Method,
        path: &str,
        mut options: RequestOptions,
        body: Option<Bytes>,
        content_type: Option<&str>,
    ) -> Result<Response> {
        let base_url = self.inner.base_url.endpoint(endpoint)?;
        let mut url = join_url(&base_url, path)?;
        append_query(&mut url, &options);

        if !options.headers.contains_key(UA) && !self.inner.user_agent.is_empty() {
            options.headers.insert(
                UA,
                HeaderValue::from_str(&self.inner.user_agent)
                    .map_err(|e| Error::InvalidInput(format!("invalid user-agent header: {e}")))?,
            );
        }
        if let Some(host) = &self.inner.host {
            insert_header(&mut options.headers, "Host", host)?;
        }
        if let Some(content_type) = content_type {
            options.headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_str(content_type).map_err(|e| {
                    Error::InvalidInput(format!("invalid content type header: {e}"))
                })?,
            );
        }
        if let Some(body) = &body {
            options.headers.insert(
                CONTENT_LENGTH,
                HeaderValue::from_str(&body.len().to_string()).map_err(|e| {
                    Error::InvalidInput(format!("invalid content length header: {e}"))
                })?,
            );
        }

        let count = self.inner.config.retry.count.max(1);
        let mut last_err = None;
        let mut current_url = url;
        for attempt in 0..count {
            let mut attempt_options = options.clone();
            if attempt > 0 {
                attempt_options
                    .headers
                    .insert("x-cos-sdk-retry", HeaderValue::from_static("true"));
            }
            maybe_authorize(
                self.inner.credential_provider.clone(),
                &method,
                &current_url,
                &mut attempt_options,
            )
            .await?;

            match self
                .send_once(
                    endpoint,
                    method.clone(),
                    current_url.clone(),
                    attempt_options,
                    body.clone(),
                )
                .await
            {
                Ok(resp) => return Ok(resp),
                Err(err) if should_retry(&err) && attempt + 1 < count => {
                    last_err = Some(err);
                    if self.inner.config.retry.auto_switch_host && attempt + 2 >= count {
                        current_url = switch_host(&current_url);
                    }
                    if !self.inner.config.retry.interval.is_zero() {
                        tokio::time::sleep(self.inner.config.retry.interval).await;
                    }
                }
                Err(err) => return Err(err),
            }
        }
        Err(last_err.unwrap_or_else(|| Error::InvalidInput("retry failed".to_owned())))
    }

    async fn send_once(
        &self,
        endpoint: Endpoint,
        method: Method,
        url: Url,
        options: RequestOptions,
        body: Option<Bytes>,
    ) -> Result<Response> {
        let mut request = self
            .inner
            .http
            .request(method.clone(), url.clone())
            .headers(options.headers);
        if let Some(body) = body {
            request = request.body(body);
        }
        let resp = request.send().await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.bytes().await?;
        if !status.is_success() {
            if endpoint == Endpoint::Vector {
                return Err(Error::Vector(Box::new(VectorErrorResponse::from_response(
                    method, url, status, headers, body,
                ))));
            }
            return Err(Error::Api(Box::new(ErrorResponse::from_response(
                method, url, status, headers, body,
            ))));
        }
        Ok(Response {
            status,
            headers,
            body,
            url,
        })
    }

    pub(crate) async fn send_xml_body<T: Serialize>(
        &self,
        endpoint: Endpoint,
        method: Method,
        path: &str,
        mut options: RequestOptions,
        body: &T,
    ) -> Result<Response> {
        let xml = quick_xml::se::to_string(body).map_err(|e| Error::XmlSerialize(e.to_string()))?;
        let bytes = Bytes::from(xml);
        let mut digest = Md5::new();
        digest.update(&bytes);
        let md5 = base64::engine::general_purpose::STANDARD.encode(digest.finalize());
        options.headers.insert(
            HeaderName::from_static("content-md5"),
            HeaderValue::from_str(&md5)
                .map_err(|e| Error::InvalidInput(format!("invalid md5 header: {e}")))?,
        );
        self.send(
            endpoint,
            method,
            path,
            options,
            Some(bytes),
            Some(CONTENT_TYPE_XML),
        )
        .await
    }

    pub(crate) async fn send_json_body<T: Serialize>(
        &self,
        endpoint: Endpoint,
        method: Method,
        path: &str,
        options: RequestOptions,
        body: &T,
    ) -> Result<Response> {
        let bytes = Bytes::from(serde_json::to_vec(body)?);
        self.send(
            endpoint,
            method,
            path,
            options,
            Some(bytes),
            Some(CONTENT_TYPE_JSON),
        )
        .await
    }

    pub(crate) async fn get_xml<T: DeserializeOwned>(
        &self,
        endpoint: Endpoint,
        method: Method,
        path: &str,
        options: RequestOptions,
    ) -> Result<(T, Response)> {
        let resp = self
            .send(endpoint, method, path, options, None, None)
            .await?;
        let parsed = quick_xml::de::from_reader(resp.body.as_ref())?;
        Ok((parsed, resp))
    }

    pub(crate) async fn parse_xml<T: DeserializeOwned>(
        &self,
        resp: Response,
    ) -> Result<(T, Response)> {
        let parsed = quick_xml::de::from_reader(resp.body.as_ref())?;
        Ok((parsed, resp))
    }

    pub(crate) async fn parse_json<T: DeserializeOwned>(
        &self,
        resp: Response,
    ) -> Result<(T, Response)> {
        let parsed = serde_json::from_slice(&resp.body)?;
        Ok((parsed, resp))
    }
}

fn join_url(base: &Url, path: &str) -> Result<Url> {
    if path.is_empty() {
        return Ok(base.clone());
    }
    base.join(path).map_err(Error::from)
}

fn should_retry(err: &Error) -> bool {
    match err {
        Error::Api(resp) => resp.status.as_u16() >= 500,
        Error::Vector(resp) => resp.status.as_u16() >= 500,
        Error::Http(_) => true,
        _ => false,
    }
}

fn switch_host(url: &Url) -> Url {
    let Some(host) = url.host_str() else {
        return url.clone();
    };
    if host.ends_with(".myqcloud.com")
        && host.contains(".cos.")
        && !host.ends_with("accelerate.myqcloud.com")
    {
        let mut switched = url.clone();
        let new_host = host.trim_end_matches(".myqcloud.com").to_owned() + ".tencentcos.cn";
        let _ = switched.set_host(Some(&new_host));
        switched
    } else {
        url.clone()
    }
}

pub(crate) fn empty_put_post_body(method: &Method) -> Option<Bytes> {
    if *method == Method::PUT || *method == Method::POST {
        Some(Bytes::new())
    } else {
        None
    }
}

pub(crate) fn status_is_not_found(err: &Error) -> bool {
    matches!(err, Error::Api(resp) if resp.status == StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_bucket_url_matches_go_sdk() {
        let url = BaseUrl::bucket_url("bname-idx", "ap-guangzhou", false).unwrap();
        assert_eq!(
            url.as_str().trim_end_matches('/'),
            "http://bname-idx.cos.ap-guangzhou.myqcloud.com"
        );
        assert!(BaseUrl::bucket_url("", "ap-guangzhou", false).is_err());
        assert!(BaseUrl::bucket_url("bname-idx", "", false).is_err());
    }

    #[test]
    fn switch_host_matches_go_sdk() {
        let url = Url::parse("https://example-125000000.cos.ap-chengdu.myqcloud.com/123").unwrap();
        assert_eq!(
            switch_host(&url).as_str(),
            "https://example-125000000.cos.ap-chengdu.tencentcos.cn/123"
        );
    }
}
