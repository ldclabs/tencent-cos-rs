//! Tencent Cloud COS adapter for [`object_store`].
//!
//! This crate wraps [`cos_rs`] and exposes COS buckets through the
//! [`object_store::ObjectStore`] trait.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::{Debug, Formatter};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use cos_rs::{
    BaseUrl, BucketGetOptions, Client, CompletePart, Credential, CredentialProvider,
    EnvCredentialProvider, ObjectCopyOptions, ObjectGetOptions, ObjectHeadOptions,
    ObjectPutOptions, PresignedUrlOptions, RequestOptions, StaticCredentialProvider, encode_key,
};
use futures_util::stream::{self, BoxStream};
use futures_util::{StreamExt, TryStreamExt};
use http::Method;
use object_store::list::{PaginatedListOptions, PaginatedListResult, PaginatedListStore};
use object_store::multipart::{MultipartStore, PartId};
use object_store::path::{DELIMITER, Path};
use object_store::signer::Signer;
use object_store::{
    Attribute, AttributeValue, Attributes, CopyMode, CopyOptions, Error, Extensions, GetOptions,
    GetResult, GetResultPayload, ListResult, MultipartId, MultipartUpload, ObjectMeta, ObjectStore,
    PutMode, PutMultipartOptions, PutOptions, PutPayload, PutResult, Result, TagSet, UploadPart,
};
use reqwest::StatusCode;
use reqwest::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_LANGUAGE, CONTENT_LENGTH,
    CONTENT_TYPE, ETAG, HeaderMap, HeaderName, HeaderValue, IF_MATCH, IF_MODIFIED_SINCE,
    IF_NONE_MATCH, IF_UNMODIFIED_SINCE, LAST_MODIFIED,
};
use url::Url;

const STORE: &str = "TencentCOS";
const VERSION_HEADER: &str = "x-cos-version-id";
const STORAGE_CLASS_HEADER: &str = "x-cos-storage-class";
const TAGGING_HEADER: &str = "x-cos-tagging";
const FORBID_OVERWRITE_HEADER: &str = "x-cos-forbid-overwrite";
const USER_METADATA_PREFIX: &str = "x-cos-meta-";

/// [`cos_rs::CredentialProvider`] used by [`TencentCos`].
pub type CosCredentialProvider = Arc<dyn CredentialProvider>;

/// Builder for [`TencentCos`].
#[derive(Default)]
pub struct TencentCosBuilder {
    bucket_name: Option<String>,
    region: Option<String>,
    secure: Option<bool>,
    bucket_url: Option<Url>,
    http_client: Option<reqwest::Client>,
    config: Option<cos_rs::Config>,
    user_agent: Option<String>,
    host: Option<String>,
    credential_provider: Option<CosCredentialProvider>,
}

impl Debug for TencentCosBuilder {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TencentCosBuilder")
            .field("bucket_name", &self.bucket_name)
            .field("region", &self.region)
            .field("secure", &self.secure.unwrap_or(true))
            .field("bucket_url", &self.bucket_url)
            .field("has_http_client", &self.http_client.is_some())
            .field("has_config", &self.config.is_some())
            .field("user_agent", &self.user_agent)
            .field("host", &self.host)
            .field(
                "has_credential_provider",
                &self.credential_provider.is_some(),
            )
            .finish()
    }
}

impl TencentCosBuilder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder using common environment variables.
    ///
    /// Recognized variables:
    /// - `COS_BUCKET` or `TENCENT_COS_BUCKET`
    /// - `COS_REGION` or `TENCENT_COS_REGION`
    /// - `COS_BUCKET_URL` or `TENCENT_COS_BUCKET_URL`
    /// - `COS_SECRETID`, `COS_SECRETKEY`, and optional `COS_SESSION_TOKEN`
    pub fn from_env() -> Self {
        let mut builder = Self::new();
        builder.bucket_name = env_first(["COS_BUCKET", "TENCENT_COS_BUCKET"]);
        builder.region = env_first(["COS_REGION", "TENCENT_COS_REGION"]);
        builder.bucket_url = env_first(["COS_BUCKET_URL", "TENCENT_COS_BUCKET_URL"])
            .and_then(|v| Url::parse(&v).ok());
        builder.credential_provider = Some(Arc::new(EnvCredentialProvider::default()));
        builder
    }

    /// Set the COS bucket name in `{name}-{appid}` format.
    pub fn with_bucket_name(mut self, bucket_name: impl Into<String>) -> Self {
        self.bucket_name = Some(bucket_name.into());
        self
    }

    /// Set the COS region, for example `ap-guangzhou`.
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Use HTTPS when constructing the standard COS bucket endpoint.
    ///
    /// Defaults to `true`.
    pub fn with_secure(mut self, secure: bool) -> Self {
        self.secure = Some(secure);
        self
    }

    /// Set a complete bucket endpoint URL.
    pub fn with_bucket_url(mut self, bucket_url: Url) -> Self {
        self.bucket_url = Some(bucket_url);
        self
    }

    /// Use a caller-provided HTTP client.
    pub fn with_http_client(mut self, http_client: reqwest::Client) -> Self {
        self.http_client = Some(http_client);
        self
    }

    /// Replace the underlying COS SDK config.
    pub fn with_config(mut self, config: cos_rs::Config) -> Self {
        self.config = Some(config);
        self
    }

    /// Override the SDK user agent.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Override the HTTP Host header.
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Use static COS credentials.
    pub fn with_credential(mut self, credential: Credential) -> Self {
        self.credential_provider = Some(Arc::new(StaticCredentialProvider::new(credential)));
        self
    }

    /// Use static long-term COS credentials.
    pub fn with_secret_id_and_key(
        self,
        secret_id: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Self {
        self.with_credential(Credential::new(secret_id, secret_key))
    }

    /// Use static temporary COS credentials.
    pub fn with_session_credential(
        self,
        secret_id: impl Into<String>,
        secret_key: impl Into<String>,
        session_token: impl Into<String>,
    ) -> Self {
        self.with_credential(Credential::with_token(secret_id, secret_key, session_token))
    }

    /// Use a custom credential provider.
    pub fn with_credential_provider(mut self, provider: CosCredentialProvider) -> Self {
        self.credential_provider = Some(provider);
        self
    }

    /// Build the [`TencentCos`] store.
    pub fn build(self) -> Result<TencentCos> {
        let bucket_name = self.bucket_name.ok_or_else(|| Error::Generic {
            store: STORE,
            source: "bucket name is required".into(),
        })?;

        let bucket_url = match self.bucket_url {
            Some(url) => url,
            None => {
                let region = self.region.ok_or_else(|| Error::Generic {
                    store: STORE,
                    source: "region is required when bucket_url is not provided".into(),
                })?;
                BaseUrl::bucket_url(&bucket_name, &region, self.secure.unwrap_or(true))
                    .map_err(map_builder_error)?
            }
        };

        let credentials = self.credential_provider;
        let mut builder = Client::builder().bucket_url(bucket_url);
        if let Some(http_client) = self.http_client {
            builder = builder.http_client(http_client);
        }
        if let Some(config) = self.config {
            builder = builder.config(config);
        }
        if let Some(user_agent) = self.user_agent {
            builder = builder.user_agent(user_agent);
        }
        if let Some(host) = self.host {
            builder = builder.host(host);
        }
        if let Some(provider) = credentials.clone() {
            builder = builder.credential_provider(provider);
        }

        let client = builder.build().map_err(map_builder_error)?;
        Ok(TencentCos {
            client,
            bucket_name,
            credentials,
        })
    }
}

/// Tencent Cloud COS object store.
#[derive(Clone)]
pub struct TencentCos {
    client: Client,
    bucket_name: String,
    credentials: Option<CosCredentialProvider>,
}

impl TencentCos {
    /// Create a builder.
    pub fn builder() -> TencentCosBuilder {
        TencentCosBuilder::new()
    }

    /// Wrap an existing COS SDK client.
    pub fn new(client: Client, bucket_name: impl Into<String>) -> Self {
        Self {
            client,
            bucket_name: bucket_name.into(),
            credentials: None,
        }
    }

    /// Wrap an existing COS SDK client and credential provider.
    pub fn new_with_credentials(
        client: Client,
        bucket_name: impl Into<String>,
        credentials: CosCredentialProvider,
    ) -> Self {
        Self {
            client,
            bucket_name: bucket_name.into(),
            credentials: Some(credentials),
        }
    }

    /// Return the underlying COS SDK client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Return the configured bucket name.
    pub fn bucket_name(&self) -> &str {
        &self.bucket_name
    }

    /// Return the credential provider, if this store was built with one.
    pub fn credentials(&self) -> Option<&CosCredentialProvider> {
        self.credentials.as_ref()
    }

    async fn head_object(&self, location: &Path, options: GetOptions) -> Result<GetResult> {
        let mut head_options = ObjectHeadOptions {
            version_id: options.version.clone(),
            extra_headers: HeaderMap::new(),
        };
        apply_get_preconditions(&mut head_options.extra_headers, &options)?;

        let response = self
            .client
            .object()
            .head(path_to_key(location), Some(head_options))
            .await
            .map_err(|e| map_cos_error(e, location))?;

        let meta = meta_from_headers(location, &response.headers, None)?;
        options.check_preconditions(&meta)?;
        Ok(GetResult {
            payload: GetResultPayload::Stream(stream::empty().boxed()),
            range: 0..meta.size,
            meta,
            attributes: attributes_from_headers(&response.headers),
            extensions: Extensions::new(),
        })
    }

    async fn list_page(
        &self,
        prefix: Option<&str>,
        opts: PaginatedListOptions,
    ) -> Result<PaginatedListResult> {
        let marker = opts.page_token.or(opts.offset);
        let max_keys = match opts.max_keys {
            Some(v) => Some(i32::try_from(v).map_err(|e| Error::Generic {
                store: STORE,
                source: Box::new(e),
            })?),
            None => None,
        };

        let options = BucketGetOptions {
            prefix: prefix.map(ToOwned::to_owned),
            delimiter: opts.delimiter.map(Cow::into_owned),
            marker,
            max_keys,
            encoding_type: None,
        };
        let (result, _response) = self
            .client
            .bucket()
            .get(Some(options))
            .await
            .map_err(|e| map_cos_error_with_path(e, ""))?;
        let is_truncated = result.is_truncated;
        let next_marker = result.next_marker.clone();

        let mut objects = Vec::with_capacity(result.contents.len());
        for object in result.contents {
            objects.push(object_meta_from_list_object(object)?);
        }

        let mut common_prefixes = Vec::with_capacity(result.common_prefixes.len());
        for prefix in result.common_prefixes {
            common_prefixes.push(Path::parse(prefix.prefix).map_err(Error::from)?);
        }

        let page_token = if is_truncated {
            if !next_marker.is_empty() {
                Some(next_marker)
            } else {
                objects
                    .last()
                    .map(|o| o.location.to_string())
                    .or_else(|| common_prefixes.last().map(ToString::to_string))
            }
        } else {
            None
        };

        Ok(PaginatedListResult {
            result: ListResult {
                common_prefixes,
                objects,
                extensions: Extensions::new(),
            },
            page_token,
        })
    }

    fn list_pages(
        &self,
        prefix: Option<String>,
        delimiter: Option<Cow<'static, str>>,
        offset: Option<String>,
    ) -> BoxStream<'static, Result<ListResult>> {
        let store = self.clone();
        stream::try_unfold(
            ListState {
                page_token: None,
                offset,
                finished: false,
            },
            move |mut state| {
                let store = store.clone();
                let prefix = prefix.clone();
                let delimiter = delimiter.clone();
                async move {
                    if state.finished {
                        return Ok(None);
                    }

                    let result = store
                        .list_page(
                            prefix.as_deref(),
                            PaginatedListOptions {
                                offset: state.offset.take(),
                                delimiter,
                                page_token: state.page_token.take(),
                                ..Default::default()
                            },
                        )
                        .await?;

                    state.page_token = result.page_token;
                    state.finished = state.page_token.is_none();

                    Ok(Some((result.result, state)))
                }
            },
        )
        .boxed()
    }

    async fn upload_part_impl(
        &self,
        path: &Path,
        id: &MultipartId,
        part_idx: usize,
        payload: PutPayload,
    ) -> Result<PartId> {
        let part_number = part_number(part_idx)?;
        let response = self
            .client
            .object()
            .upload_part(path_to_key(path), id, part_number, Bytes::from(payload))
            .await
            .map_err(|e| map_cos_error(e, path))?;

        Ok(PartId {
            content_id: etag_from_headers(&response.headers),
        })
    }

    async fn complete_multipart_impl(
        &self,
        path: &Path,
        id: &MultipartId,
        parts: Vec<PartId>,
    ) -> Result<PutResult> {
        let parts = parts
            .into_iter()
            .enumerate()
            .map(|(idx, part)| {
                Ok(CompletePart {
                    part_number: part_number(idx)?,
                    etag: part.content_id,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let (result, response) = self
            .client
            .object()
            .complete_multipart_upload(path_to_key(path), id, parts)
            .await
            .map_err(|e| map_cos_error(e, path))?;

        Ok(PutResult {
            e_tag: non_empty(result.etag).or_else(|| header_str(&response.headers, ETAG)),
            version: header_str(&response.headers, VERSION_HEADER),
            extensions: Extensions::new(),
        })
    }
}

impl Debug for TencentCos {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TencentCos")
            .field("bucket_name", &self.bucket_name)
            .field("has_credentials", &self.credentials.is_some())
            .finish()
    }
}

impl std::fmt::Display for TencentCos {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "TencentCos({})", self.bucket_name)
    }
}

#[async_trait]
impl ObjectStore for TencentCos {
    async fn put_opts(
        &self,
        location: &Path,
        payload: PutPayload,
        opts: PutOptions,
    ) -> Result<PutResult> {
        let PutOptions {
            mode,
            tags,
            attributes,
            extensions: _,
        } = opts;

        let mut put_options = object_put_options(&attributes, &tags)?;
        match mode {
            PutMode::Overwrite => {}
            PutMode::Create => {
                insert_header_value(
                    &mut put_options.extra_headers,
                    FORBID_OVERWRITE_HEADER,
                    "true",
                )?;
            }
            PutMode::Update(ref version) => {
                let etag = version.e_tag.clone().ok_or_else(|| Error::Generic {
                    store: STORE,
                    source: "ETag required for conditional put".into(),
                })?;
                insert_header_value(&mut put_options.extra_headers, IF_MATCH.as_str(), &etag)?;
            }
        }

        let response = self
            .client
            .object()
            .put(
                path_to_key(location),
                Bytes::from(payload),
                Some(put_options),
            )
            .await
            .map_err(|e| match &mode {
                PutMode::Create => map_already_exists(e, location),
                PutMode::Update(_) => map_precondition(e, location),
                PutMode::Overwrite => map_cos_error(e, location),
            })?;

        Ok(PutResult {
            e_tag: header_str(&response.headers, ETAG),
            version: header_str(&response.headers, VERSION_HEADER),
            extensions: Extensions::new(),
        })
    }

    async fn put_multipart_opts(
        &self,
        location: &Path,
        opts: PutMultipartOptions,
    ) -> Result<Box<dyn MultipartUpload>> {
        let upload_id = self.create_multipart_opts(location, opts).await?;
        Ok(Box::new(CosMultipartUpload {
            part_idx: 0,
            state: Arc::new(UploadState {
                store: self.clone(),
                location: location.clone(),
                upload_id,
                parts: UploadedParts::default(),
            }),
        }))
    }

    async fn get_opts(&self, location: &Path, options: GetOptions) -> Result<GetResult> {
        if options.head {
            return self.head_object(location, options).await;
        }

        if let Some(range) = &options.range {
            range.is_valid().map_err(|source| Error::Generic {
                store: STORE,
                source: Box::new(source),
            })?;
        }

        let mut get_options = ObjectGetOptions {
            version_id: options.version.clone(),
            ..Default::default()
        };
        if let Some(range) = &options.range {
            get_options.range = Some(range.to_string());
        }
        apply_get_preconditions(&mut get_options.extra_headers, &options)?;

        let response = self
            .client
            .object()
            .get(path_to_key(location), Some(get_options))
            .await
            .map_err(|e| map_cos_error(e, location))?;

        let content_range = content_range(&response.headers)?;
        let mut meta = meta_from_headers(location, &response.headers, content_range.as_ref())?;
        if meta.size == 0 && !response.body.is_empty() {
            meta.size = response.body.len() as u64;
        }
        options.check_preconditions(&meta)?;

        let attributes = attributes_from_headers(&response.headers);
        let range = content_range
            .map(|(range, _)| range)
            .unwrap_or_else(|| 0..response.body.len() as u64);
        let body = response.into_bytes();
        Ok(GetResult {
            payload: GetResultPayload::Stream(stream::once(async move { Ok(body) }).boxed()),
            meta,
            range,
            attributes,
            extensions: Extensions::new(),
        })
    }

    fn delete_stream(
        &self,
        locations: BoxStream<'static, Result<Path>>,
    ) -> BoxStream<'static, Result<Path>> {
        let store = self.clone();
        locations
            .map(move |location| {
                let store = store.clone();
                async move {
                    let location = location?;
                    store
                        .client
                        .object()
                        .delete(path_to_key(&location), None)
                        .await
                        .map_err(|e| map_cos_error(e, &location))?;
                    Ok(location)
                }
            })
            .buffered(10)
            .boxed()
    }

    fn list(&self, prefix: Option<&Path>) -> BoxStream<'static, Result<ObjectMeta>> {
        let prefix = list_prefix(prefix);
        self.list_pages(prefix, None, None)
            .map_ok(|r| stream::iter(r.objects.into_iter().map(Ok)))
            .try_flatten()
            .boxed()
    }

    fn list_with_offset(
        &self,
        prefix: Option<&Path>,
        offset: &Path,
    ) -> BoxStream<'static, Result<ObjectMeta>> {
        let prefix = list_prefix(prefix);
        self.list_pages(prefix, None, Some(offset.to_string()))
            .map_ok(|r| stream::iter(r.objects.into_iter().map(Ok)))
            .try_flatten()
            .boxed()
    }

    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> Result<ListResult> {
        let prefix = list_prefix(prefix);
        let mut stream = self.list_pages(prefix, Some(Cow::Borrowed(DELIMITER)), None);
        let mut common_prefixes = BTreeSet::new();
        let mut objects = Vec::new();
        let mut extensions = Extensions::new();

        while let Some(result) = stream.next().await {
            let result = result?;
            common_prefixes.extend(result.common_prefixes);
            objects.extend(result.objects);
            extensions.extend(result.extensions);
        }

        Ok(ListResult {
            common_prefixes: common_prefixes.into_iter().collect(),
            objects,
            extensions,
        })
    }

    async fn copy_opts(&self, from: &Path, to: &Path, options: CopyOptions) -> Result<()> {
        let CopyOptions {
            mode,
            extensions: _,
        } = options;

        let mut copy_options = ObjectCopyOptions::default();
        if mode == CopyMode::Create {
            insert_header_value(
                &mut copy_options.extra_headers,
                FORBID_OVERWRITE_HEADER,
                "true",
            )?;
        }

        self.client
            .object()
            .copy(path_to_key(to), &self.copy_source(from), Some(copy_options))
            .await
            .map_err(|e| match mode {
                CopyMode::Create => map_already_exists(e, to),
                CopyMode::Overwrite => map_cos_error(e, to),
            })?;

        Ok(())
    }
}

#[async_trait]
impl MultipartStore for TencentCos {
    async fn create_multipart(&self, path: &Path) -> Result<MultipartId> {
        self.create_multipart_opts(path, PutMultipartOptions::default())
            .await
    }

    async fn create_multipart_opts(
        &self,
        path: &Path,
        opts: PutMultipartOptions,
    ) -> Result<MultipartId> {
        let PutMultipartOptions {
            tags,
            attributes,
            extensions: _,
        } = opts;
        let put_options = object_put_options(&attributes, &tags)?;
        let (result, _response) = self
            .client
            .object()
            .initiate_multipart_upload(path_to_key(path), Some(put_options))
            .await
            .map_err(|e| map_cos_error(e, path))?;
        Ok(result.upload_id)
    }

    async fn put_part(
        &self,
        path: &Path,
        id: &MultipartId,
        part_idx: usize,
        data: PutPayload,
    ) -> Result<PartId> {
        self.upload_part_impl(path, id, part_idx, data).await
    }

    async fn complete_multipart(
        &self,
        path: &Path,
        id: &MultipartId,
        parts: Vec<PartId>,
    ) -> Result<PutResult> {
        self.complete_multipart_impl(path, id, parts).await
    }

    async fn abort_multipart(&self, path: &Path, id: &MultipartId) -> Result<()> {
        self.client
            .object()
            .abort_multipart_upload(path_to_key(path), id)
            .await
            .map_err(|e| map_cos_error(e, path))?;
        Ok(())
    }
}

#[async_trait]
impl PaginatedListStore for TencentCos {
    async fn list_paginated(
        &self,
        prefix: Option<&str>,
        opts: PaginatedListOptions,
    ) -> Result<PaginatedListResult> {
        self.list_page(prefix, opts).await
    }
}

#[async_trait]
impl Signer for TencentCos {
    async fn signed_url(&self, method: Method, path: &Path, expires_in: Duration) -> Result<Url> {
        let credentials = self.credentials.as_ref().ok_or_else(|| Error::Generic {
            store: STORE,
            source: "signed_url requires a credential provider".into(),
        })?;
        let credential = credentials.credential().await.map_err(map_builder_error)?;
        let mut request_options = RequestOptions::new();
        if let Some(token) = &credential.session_token {
            request_options = request_options.query("x-cos-security-token", token);
        }

        self.client
            .object()
            .get_presigned_url(
                method,
                path_to_key(path),
                &credential,
                expires_in,
                Some(PresignedUrlOptions {
                    request_options,
                    sign_host: true,
                    ..Default::default()
                }),
            )
            .map_err(map_builder_error)
    }
}

#[derive(Debug)]
struct CosMultipartUpload {
    part_idx: usize,
    state: Arc<UploadState>,
}

struct UploadState {
    store: TencentCos,
    location: Path,
    upload_id: MultipartId,
    parts: UploadedParts,
}

impl Debug for UploadState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadState")
            .field("store", &self.store)
            .field("location", &self.location)
            .field("upload_id", &self.upload_id)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl MultipartUpload for CosMultipartUpload {
    fn put_part(&mut self, data: PutPayload) -> UploadPart {
        let idx = self.part_idx;
        self.part_idx += 1;
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let part = state
                .store
                .upload_part_impl(&state.location, &state.upload_id, idx, data)
                .await?;
            state.parts.put(idx, part);
            Ok(())
        })
    }

    async fn complete(&mut self) -> Result<PutResult> {
        let parts = self.state.parts.finish(self.part_idx)?;
        self.state
            .store
            .complete_multipart_impl(&self.state.location, &self.state.upload_id, parts)
            .await
    }

    async fn abort(&mut self) -> Result<()> {
        self.state
            .store
            .abort_multipart(&self.state.location, &self.state.upload_id)
            .await
    }
}

#[derive(Default)]
struct UploadedParts {
    parts: Mutex<Vec<Option<PartId>>>,
}

impl UploadedParts {
    fn put(&self, idx: usize, part: PartId) {
        let mut parts = self.parts.lock().expect("parts mutex poisoned");
        if parts.len() <= idx {
            parts.resize_with(idx + 1, || None);
        }
        parts[idx] = Some(part);
    }

    fn finish(&self, len: usize) -> Result<Vec<PartId>> {
        let parts = self.parts.lock().expect("parts mutex poisoned");
        if parts.len() < len {
            return Err(Error::Generic {
                store: STORE,
                source: format!("missing multipart part {}", parts.len()).into(),
            });
        }

        parts
            .iter()
            .take(len)
            .enumerate()
            .map(|(idx, part)| {
                part.clone().ok_or_else(|| Error::Generic {
                    store: STORE,
                    source: format!("missing multipart part {idx}").into(),
                })
            })
            .collect()
    }
}

impl Debug for UploadedParts {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let len = self.parts.lock().map(|p| p.len()).unwrap_or_default();
        f.debug_struct("UploadedParts").field("len", &len).finish()
    }
}

#[derive(Debug)]
struct ListState {
    page_token: Option<String>,
    offset: Option<String>,
    finished: bool,
}

impl TencentCos {
    fn copy_source(&self, path: &Path) -> String {
        let key = encode_key(path_to_key(path), true);
        format!("/{}/{}", self.bucket_name, key)
    }
}

fn env_first<const N: usize>(names: [&str; N]) -> Option<String> {
    names
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
}

fn path_to_key(path: &Path) -> &str {
    path.as_ref()
}

fn list_prefix(prefix: Option<&Path>) -> Option<String> {
    prefix
        .filter(|p| !p.as_ref().is_empty())
        .map(|p| format!("{}{DELIMITER}", p.as_ref()))
}

fn object_put_options(attributes: &Attributes, tags: &TagSet) -> Result<ObjectPutOptions> {
    let mut options = ObjectPutOptions::default();
    for (key, value) in attributes {
        let value = value.as_ref();
        match key {
            Attribute::CacheControl => options.cache_control = Some(value.to_owned()),
            Attribute::ContentDisposition => options.content_disposition = Some(value.to_owned()),
            Attribute::ContentEncoding => options.content_encoding = Some(value.to_owned()),
            Attribute::ContentLanguage => {
                insert_header_value(&mut options.extra_headers, CONTENT_LANGUAGE.as_str(), value)?;
            }
            Attribute::ContentType => options.content_type = Some(value.to_owned()),
            Attribute::StorageClass => options.storage_class = Some(value.to_owned()),
            Attribute::Metadata(name) => {
                insert_header_value(
                    &mut options.extra_headers,
                    &format!("{USER_METADATA_PREFIX}{name}"),
                    value,
                )?;
            }
            _ => {
                return Err(Error::NotSupported {
                    source: format!("unsupported COS object attribute: {key:?}").into(),
                });
            }
        }
    }

    if !tags.is_empty() {
        insert_header_value(&mut options.extra_headers, TAGGING_HEADER, tags.encoded())?;
    }

    Ok(options)
}

fn apply_get_preconditions(headers: &mut HeaderMap, options: &GetOptions) -> Result<()> {
    if let Some(value) = &options.if_match {
        insert_header_value(headers, IF_MATCH.as_str(), value)?;
    }
    if let Some(value) = &options.if_none_match {
        insert_header_value(headers, IF_NONE_MATCH.as_str(), value)?;
    }
    if let Some(value) = options.if_modified_since {
        insert_header_value(headers, IF_MODIFIED_SINCE.as_str(), &value.to_rfc2822())?;
    }
    if let Some(value) = options.if_unmodified_since {
        insert_header_value(headers, IF_UNMODIFIED_SINCE.as_str(), &value.to_rfc2822())?;
    }
    Ok(())
}

fn insert_header_value(headers: &mut HeaderMap, name: &str, value: &str) -> Result<()> {
    let name = HeaderName::from_bytes(name.as_bytes()).map_err(|e| Error::Generic {
        store: STORE,
        source: Box::new(e),
    })?;
    let value = HeaderValue::from_str(value).map_err(|e| Error::Generic {
        store: STORE,
        source: Box::new(e),
    })?;
    headers.insert(name, value);
    Ok(())
}

fn object_meta_from_list_object(object: cos_rs::Object) -> Result<ObjectMeta> {
    let size = u64::try_from(object.size).map_err(|e| Error::Generic {
        store: STORE,
        source: Box::new(e),
    })?;
    Ok(ObjectMeta {
        location: Path::parse(object.key).map_err(Error::from)?,
        last_modified: parse_cos_datetime(&object.last_modified)?,
        size,
        e_tag: non_empty(object.etag),
        version: None,
    })
}

fn meta_from_headers(
    location: &Path,
    headers: &HeaderMap,
    content_range: Option<&(Range<u64>, u64)>,
) -> Result<ObjectMeta> {
    let size = match content_range {
        Some((_, total)) => *total,
        None => header_str(headers, CONTENT_LENGTH)
            .and_then(|v| v.parse().ok())
            .unwrap_or_default(),
    };
    Ok(ObjectMeta {
        location: location.clone(),
        last_modified: header_str(headers, LAST_MODIFIED)
            .map(|v| parse_cos_datetime(&v))
            .transpose()?
            .unwrap_or_else(unix_epoch),
        size,
        e_tag: header_str(headers, ETAG),
        version: header_str(headers, VERSION_HEADER),
    })
}

fn attributes_from_headers(headers: &HeaderMap) -> Attributes {
    let mut attributes = Attributes::new();
    insert_attribute_header(
        headers,
        &mut attributes,
        CACHE_CONTROL,
        Attribute::CacheControl,
    );
    insert_attribute_header(
        headers,
        &mut attributes,
        CONTENT_DISPOSITION,
        Attribute::ContentDisposition,
    );
    insert_attribute_header(
        headers,
        &mut attributes,
        CONTENT_ENCODING,
        Attribute::ContentEncoding,
    );
    insert_attribute_header(
        headers,
        &mut attributes,
        CONTENT_LANGUAGE,
        Attribute::ContentLanguage,
    );
    insert_attribute_header(
        headers,
        &mut attributes,
        CONTENT_TYPE,
        Attribute::ContentType,
    );
    if let Some(value) = header_str(headers, STORAGE_CLASS_HEADER) {
        attributes.insert(Attribute::StorageClass, AttributeValue::from(value));
    }

    for (name, value) in headers {
        let name = name.as_str();
        if let Some(suffix) = name.strip_prefix(USER_METADATA_PREFIX)
            && let Ok(value) = value.to_str()
        {
            attributes.insert(
                Attribute::Metadata(Cow::Owned(suffix.to_owned())),
                AttributeValue::from(value.to_owned()),
            );
        }
    }

    attributes
}

fn insert_attribute_header(
    headers: &HeaderMap,
    attributes: &mut Attributes,
    header: HeaderName,
    attribute: Attribute,
) {
    if let Some(value) = header_str(headers, header) {
        attributes.insert(attribute, AttributeValue::from(value));
    }
}

fn content_range(headers: &HeaderMap) -> Result<Option<(Range<u64>, u64)>> {
    let Some(value) = header_str(headers, "content-range") else {
        return Ok(None);
    };
    let Some(value) = value.strip_prefix("bytes ") else {
        return Err(Error::Generic {
            store: STORE,
            source: format!("invalid Content-Range header: {value}").into(),
        });
    };
    let Some((range, total)) = value.split_once('/') else {
        return Err(Error::Generic {
            store: STORE,
            source: format!("invalid Content-Range header: {value}").into(),
        });
    };
    let Some((start, end)) = range.split_once('-') else {
        return Err(Error::Generic {
            store: STORE,
            source: format!("invalid Content-Range header: {value}").into(),
        });
    };
    let start = start.parse::<u64>().map_err(|e| Error::Generic {
        store: STORE,
        source: Box::new(e),
    })?;
    let end = end.parse::<u64>().map_err(|e| Error::Generic {
        store: STORE,
        source: Box::new(e),
    })?;
    let total = total.parse::<u64>().map_err(|e| Error::Generic {
        store: STORE,
        source: Box::new(e),
    })?;
    Ok(Some((start..end + 1, total)))
}

fn parse_cos_datetime(value: &str) -> Result<DateTime<Utc>> {
    if value.is_empty() {
        return Ok(unix_epoch());
    }

    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_rfc2822(value))
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|source| Error::Generic {
            store: STORE,
            source: Box::new(source),
        })
}

fn unix_epoch() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(0, 0).expect("unix epoch is valid")
}

fn part_number(part_idx: usize) -> Result<u32> {
    let number = part_idx.checked_add(1).ok_or_else(|| Error::Generic {
        store: STORE,
        source: "multipart part index overflow".into(),
    })?;
    u32::try_from(number).map_err(|e| Error::Generic {
        store: STORE,
        source: Box::new(e),
    })
}

fn header_str<K>(headers: &HeaderMap, name: K) -> Option<String>
where
    K: reqwest::header::AsHeaderName,
{
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
}

fn etag_from_headers(headers: &HeaderMap) -> String {
    header_str(headers, ETAG).unwrap_or_default()
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn map_builder_error(source: cos_rs::Error) -> Error {
    Error::Generic {
        store: STORE,
        source: Box::new(source),
    }
}

fn map_cos_error(source: cos_rs::Error, path: &Path) -> Error {
    map_cos_error_with_path(source, path.to_string())
}

fn map_cos_error_with_path(source: cos_rs::Error, path: impl Into<String>) -> Error {
    let path = path.into();
    match api_status(&source) {
        Some(StatusCode::NOT_FOUND) => Error::NotFound {
            path,
            source: Box::new(source),
        },
        Some(StatusCode::FORBIDDEN) => Error::PermissionDenied {
            path,
            source: Box::new(source),
        },
        Some(StatusCode::UNAUTHORIZED) => Error::Unauthenticated {
            path,
            source: Box::new(source),
        },
        Some(StatusCode::NOT_MODIFIED) => Error::NotModified {
            path,
            source: Box::new(source),
        },
        Some(StatusCode::PRECONDITION_FAILED) => Error::Precondition {
            path,
            source: Box::new(source),
        },
        _ => Error::Generic {
            store: STORE,
            source: Box::new(source),
        },
    }
}

fn map_already_exists(source: cos_rs::Error, path: &Path) -> Error {
    match api_status(&source) {
        Some(StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED) => Error::AlreadyExists {
            path: path.to_string(),
            source: Box::new(source),
        },
        _ => map_cos_error(source, path),
    }
}

fn map_precondition(source: cos_rs::Error, path: &Path) -> Error {
    match api_status(&source) {
        Some(StatusCode::NOT_FOUND | StatusCode::PRECONDITION_FAILED) => Error::Precondition {
            path: path.to_string(),
            source: Box::new(source),
        },
        _ => map_cos_error(source, path),
    }
}

fn api_status(error: &cos_rs::Error) -> Option<StatusCode> {
    match error {
        cos_rs::Error::Api(response) => Some(response.status),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_prefix_matches_object_store_segment_semantics() {
        assert_eq!(list_prefix(None), None);
        assert_eq!(list_prefix(Some(&Path::from(""))), None);
        assert_eq!(list_prefix(Some(&Path::from("foo"))), Some("foo/".into()));
    }

    #[test]
    fn parses_content_range() {
        let mut headers = HeaderMap::new();
        headers.insert("content-range", HeaderValue::from_static("bytes 2-5/10"));

        assert_eq!(content_range(&headers).unwrap(), Some((2..6, 10)));
    }

    #[test]
    fn maps_attributes_and_tags_to_cos_put_options() {
        let mut attributes = Attributes::new();
        attributes.insert(Attribute::ContentType, AttributeValue::from("text/plain"));
        attributes.insert(
            Attribute::Metadata(Cow::Borrowed("trace")),
            AttributeValue::from("abc"),
        );
        let mut tags = TagSet::default();
        tags.push("env", "test");

        let options = object_put_options(&attributes, &tags).unwrap();

        assert_eq!(options.content_type.as_deref(), Some("text/plain"));
        assert_eq!(
            options.extra_headers.get("x-cos-meta-trace").unwrap(),
            "abc"
        );
        assert_eq!(
            options.extra_headers.get(TAGGING_HEADER).unwrap(),
            "env=test"
        );
    }
}
