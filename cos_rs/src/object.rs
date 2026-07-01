//! Object APIs.
//!
//! Implemented methods cover object get/put/delete/head/existence, local file
//! upload/download helpers, CORS preflight OPTIONS, copy, multi-delete,
//! multipart upload lifecycle, object URL construction, and presigned URL
//! construction.

use crate::client::{Client, Endpoint, empty_put_post_body, status_is_not_found};
use crate::encoding::{RequestOptions, encode_key, insert_header, query_from_serialize};
use crate::error::{Error, Result};
use crate::response::Response;
use bytes::Bytes;
use reqwest::Method;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

#[derive(Clone)]
/// Object API entry point.
pub struct ObjectService {
    client: Client,
}

impl ObjectService {
    pub(crate) fn new(client: Client) -> Self {
        Self { client }
    }

    /// Get an object and return a buffered response body.
    pub async fn get(&self, key: &str, options: Option<ObjectGetOptions>) -> Result<Response> {
        let request_options = object_get_options(options.as_ref())?;
        self.client
            .send(
                Endpoint::Bucket,
                Method::GET,
                &object_path(key),
                request_options,
                None,
                None,
            )
            .await
    }

    /// Download an object directly to a local file path.
    pub async fn get_to_file(
        &self,
        key: &str,
        path: impl AsRef<std::path::Path>,
        options: Option<ObjectGetOptions>,
    ) -> Result<Response> {
        let response = self.get(key, options).await?;
        tokio::fs::write(path, response.bytes()).await?;
        Ok(response)
    }

    /// Upload an object from bytes.
    pub async fn put(
        &self,
        key: &str,
        body: impl Into<Bytes>,
        options: Option<ObjectPutOptions>,
    ) -> Result<Response> {
        let request_options = object_put_options(options.as_ref())?;
        self.client
            .send(
                Endpoint::Bucket,
                Method::PUT,
                &object_path(key),
                request_options,
                Some(body.into()),
                None,
            )
            .await
    }

    /// Upload an object from a local file path.
    pub async fn put_from_file(
        &self,
        key: &str,
        path: impl AsRef<std::path::Path>,
        options: Option<ObjectPutOptions>,
    ) -> Result<Response> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut body = Vec::new();
        file.read_to_end(&mut body).await?;
        self.put(key, body, options).await
    }

    /// Delete an object, optionally with version query parameters.
    pub async fn delete(
        &self,
        key: &str,
        options: Option<ObjectDeleteOptions>,
    ) -> Result<Response> {
        let mut request_options = RequestOptions::new();
        if let Some(options) = options {
            request_options.query = query_from_serialize(&options)?;
        }
        self.client
            .send(
                Endpoint::Bucket,
                Method::DELETE,
                &object_path(key),
                request_options,
                None,
                None,
            )
            .await
    }

    /// Fetch object metadata with `HEAD`.
    pub async fn head(&self, key: &str, options: Option<ObjectHeadOptions>) -> Result<Response> {
        let request_options = object_head_options(options.as_ref())?;
        self.client
            .send(
                Endpoint::Bucket,
                Method::HEAD,
                &object_path(key),
                request_options,
                None,
                None,
            )
            .await
    }

    /// Return `false` only for a COS 404; other errors are returned.
    pub async fn is_exist(&self, key: &str) -> Result<bool> {
        match self.head(key, None).await {
            Ok(_) => Ok(true),
            Err(err) if status_is_not_found(&err) => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Send an object CORS preflight request.
    pub async fn options(&self, key: &str, options: ObjectOptionsOptions) -> Result<Response> {
        let mut request_options = RequestOptions::new();
        insert_header(
            &mut request_options.headers,
            "Origin",
            &options.origin.unwrap_or_default(),
        )?;
        insert_header(
            &mut request_options.headers,
            "Access-Control-Request-Method",
            &options.access_control_request_method.unwrap_or_default(),
        )?;
        if let Some(headers) = options.access_control_request_headers {
            insert_header(
                &mut request_options.headers,
                "Access-Control-Request-Headers",
                &headers,
            )?;
        }
        self.client
            .send(
                Endpoint::Bucket,
                Method::OPTIONS,
                &object_path(key),
                request_options,
                None,
                None,
            )
            .await
    }

    /// Copy an object using `x-cos-copy-source`.
    pub async fn copy(
        &self,
        key: &str,
        source: &str,
        options: Option<ObjectCopyOptions>,
    ) -> Result<Response> {
        let mut request_options = object_put_options(options.as_ref())?;
        insert_header(&mut request_options.headers, "x-cos-copy-source", source)?;
        self.client
            .send(
                Endpoint::Bucket,
                Method::PUT,
                &object_path(key),
                request_options,
                empty_put_post_body(&Method::PUT),
                None,
            )
            .await
    }

    /// Delete multiple objects in one request.
    pub async fn delete_multi(
        &self,
        objects: Vec<DeleteObject>,
        quiet: bool,
    ) -> Result<(DeleteMultiResult, Response)> {
        let body = DeleteMultiOptions { quiet, objects };
        let resp = self
            .client
            .send_xml_body(
                Endpoint::Bucket,
                Method::POST,
                "/",
                RequestOptions::new().raw_query("delete"),
                &body,
            )
            .await?;
        self.client.parse_xml(resp).await
    }

    /// Start a multipart upload.
    pub async fn initiate_multipart_upload(
        &self,
        key: &str,
        options: Option<ObjectPutOptions>,
    ) -> Result<(InitiateMultipartUploadResult, Response)> {
        let options = object_put_options(options.as_ref())?.raw_query("uploads");
        let resp = self
            .client
            .send(
                Endpoint::Bucket,
                Method::POST,
                &object_path(key),
                options,
                empty_put_post_body(&Method::POST),
                None,
            )
            .await?;
        self.client.parse_xml(resp).await
    }

    /// Upload one multipart part.
    pub async fn upload_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        body: impl Into<Bytes>,
    ) -> Result<Response> {
        let options = RequestOptions::new()
            .query("partNumber", part_number)
            .query("uploadId", upload_id);
        self.client
            .send(
                Endpoint::Bucket,
                Method::PUT,
                &object_path(key),
                options,
                Some(body.into()),
                None,
            )
            .await
    }

    /// List uploaded parts for a multipart upload.
    pub async fn list_parts(
        &self,
        key: &str,
        upload_id: &str,
        options: Option<ListPartsOptions>,
    ) -> Result<(ListPartsResult, Response)> {
        let mut request_options = RequestOptions::new().query("uploadId", upload_id);
        if let Some(options) = options {
            request_options
                .query
                .extend(query_from_serialize(&options)?);
        }
        self.client
            .get_xml(
                Endpoint::Bucket,
                Method::GET,
                &object_path(key),
                request_options,
            )
            .await
    }

    /// Complete a multipart upload.
    pub async fn complete_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
        parts: Vec<CompletePart>,
    ) -> Result<(CompleteMultipartUploadResult, Response)> {
        let body = CompleteMultipartUploadOptions { parts };
        let options = RequestOptions::new().query("uploadId", upload_id);
        let resp = self
            .client
            .send_xml_body(
                Endpoint::Bucket,
                Method::POST,
                &object_path(key),
                options,
                &body,
            )
            .await?;
        self.client.parse_xml(resp).await
    }

    /// Abort a multipart upload.
    pub async fn abort_multipart_upload(&self, key: &str, upload_id: &str) -> Result<Response> {
        self.client
            .send(
                Endpoint::Bucket,
                Method::DELETE,
                &object_path(key),
                RequestOptions::new().query("uploadId", upload_id),
                None,
                None,
            )
            .await
    }

    /// Build the canonical object URL from the configured bucket endpoint.
    pub fn get_object_url(&self, key: &str) -> Result<url::Url> {
        let base = self
            .client
            .base_url()
            .bucket
            .clone()
            .ok_or(Error::MissingBaseUrl("bucket"))?;
        Ok(base.join(&object_path(key))?)
    }

    /// Build a presigned object URL using explicit credentials.
    pub fn get_presigned_url(
        &self,
        method: Method,
        key: &str,
        credential: &crate::Credential,
        expires: std::time::Duration,
        options: Option<PresignedUrlOptions>,
    ) -> Result<url::Url> {
        use crate::auth::{AuthTime, authorization};
        let base = self
            .client
            .base_url()
            .bucket
            .clone()
            .ok_or(Error::MissingBaseUrl("bucket"))?;
        let mut url = base.join(&object_path(key))?;
        let request_options = options
            .as_ref()
            .map(|o| o.request_options.clone())
            .unwrap_or_default();
        crate::encoding::append_query(&mut url, &request_options);
        let mut headers = request_options.headers.clone();
        let auth_time = options
            .as_ref()
            .and_then(|o| o.auth_time.clone())
            .unwrap_or_else(|| AuthTime::new(expires));
        let auth = authorization(
            &credential.secret_id,
            &credential.secret_key,
            &method,
            &url,
            &mut headers,
            &auth_time,
            options.as_ref().map(|o| o.sign_host).unwrap_or(true),
        )?;
        let sign = if options.as_ref().map(|o| o.sign_merged).unwrap_or(false) {
            crate::encoding::encode_component(&auth)
        } else {
            auth.split('&')
                .map(|part| {
                    let mut kv = part.splitn(2, '=');
                    let k = kv.next().unwrap_or_default();
                    let v = kv.next().unwrap_or_default();
                    format!("{k}={}", crate::encoding::encode_component(v))
                })
                .collect::<Vec<_>>()
                .join("&")
        };
        let existing = url.query().map(str::to_owned);
        url.set_query(Some(
            match existing {
                Some(existing) if !existing.is_empty() => format!("{existing}&{sign}"),
                _ => sign,
            }
            .as_str(),
        ));
        Ok(url)
    }
}

fn object_path(key: &str) -> String {
    format!("/{}", encode_key(key.trim_start_matches('/'), true))
}

fn object_get_options(options: Option<&ObjectGetOptions>) -> Result<RequestOptions> {
    let mut request_options = RequestOptions::new();
    if let Some(options) = options {
        request_options.query = query_from_serialize(options)?;
        if let Some(range) = &options.range {
            insert_header(&mut request_options.headers, "Range", range)?;
        }
        if let Some(value) = &options.if_modified_since {
            insert_header(&mut request_options.headers, "If-Modified-Since", value)?;
        }
        if let Some(value) = &options.traffic_limit {
            insert_header(
                &mut request_options.headers,
                "x-cos-traffic-limit",
                &value.to_string(),
            )?;
        }
        request_options
            .headers
            .extend(options.extra_headers.clone());
    }
    Ok(request_options)
}

fn object_head_options(options: Option<&ObjectHeadOptions>) -> Result<RequestOptions> {
    let mut request_options = RequestOptions::new();
    if let Some(options) = options {
        if let Some(version_id) = &options.version_id {
            request_options
                .query
                .push(("versionId".to_owned(), version_id.clone()));
        }
        request_options
            .headers
            .extend(options.extra_headers.clone());
    }
    Ok(request_options)
}

fn object_put_options(options: Option<&ObjectPutOptions>) -> Result<RequestOptions> {
    let mut request_options = RequestOptions::new();
    if let Some(options) = options {
        if let Some(value) = &options.cache_control {
            insert_header(&mut request_options.headers, "Cache-Control", value)?;
        }
        if let Some(value) = &options.content_disposition {
            insert_header(&mut request_options.headers, "Content-Disposition", value)?;
        }
        if let Some(value) = &options.content_encoding {
            insert_header(&mut request_options.headers, "Content-Encoding", value)?;
        }
        if let Some(value) = &options.content_type {
            insert_header(&mut request_options.headers, "Content-Type", value)?;
        }
        if let Some(value) = &options.content_md5 {
            insert_header(&mut request_options.headers, "Content-MD5", value)?;
        }
        if let Some(value) = options.content_length {
            insert_header(
                &mut request_options.headers,
                "Content-Length",
                &value.to_string(),
            )?;
        }
        if let Some(value) = &options.x_cos_acl {
            insert_header(&mut request_options.headers, "x-cos-acl", value)?;
        }
        if let Some(value) = &options.storage_class {
            insert_header(&mut request_options.headers, "x-cos-storage-class", value)?;
        }
        if let Some(value) = options.traffic_limit {
            insert_header(
                &mut request_options.headers,
                "x-cos-traffic-limit",
                &value.to_string(),
            )?;
        }
        request_options
            .headers
            .extend(options.extra_headers.clone());
    }
    Ok(request_options)
}

#[derive(Debug, Clone, Default, Serialize)]
/// Query and header options for object downloads.
#[serde(rename_all = "kebab-case")]
pub struct ObjectGetOptions {
    #[serde(skip_serializing_if = "Option::is_none", rename = "versionId")]
    pub version_id: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "response-content-type"
    )]
    pub response_content_type: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "response-content-language"
    )]
    pub response_content_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "response-expires")]
    pub response_expires: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "response-cache-control"
    )]
    pub response_cache_control: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "response-content-disposition"
    )]
    pub response_content_disposition: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "response-content-encoding"
    )]
    pub response_content_encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "ci-process")]
    pub ci_process: Option<String>,
    #[serde(skip)]
    pub range: Option<String>,
    #[serde(skip)]
    pub if_modified_since: Option<String>,
    #[serde(skip)]
    pub traffic_limit: Option<i32>,
    #[serde(skip)]
    pub extra_headers: HeaderMap,
}

#[derive(Debug, Clone, Default)]
/// Header options for object uploads and object copy.
pub struct ObjectPutOptions {
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub content_type: Option<String>,
    pub content_md5: Option<String>,
    pub content_length: Option<u64>,
    pub x_cos_acl: Option<String>,
    pub storage_class: Option<String>,
    pub traffic_limit: Option<i32>,
    pub extra_headers: HeaderMap,
}

pub type ObjectCopyOptions = ObjectPutOptions;

#[derive(Debug, Clone, Default, Serialize)]
/// Delete-object query options.
#[serde(rename_all = "camelCase")]
pub struct ObjectDeleteOptions {
    #[serde(skip_serializing_if = "Option::is_none", rename = "versionId")]
    pub version_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
/// Head-object query and header options.
pub struct ObjectHeadOptions {
    pub version_id: Option<String>,
    pub extra_headers: HeaderMap,
}

#[derive(Debug, Clone, Default)]
/// CORS preflight headers for object `OPTIONS`.
pub struct ObjectOptionsOptions {
    pub origin: Option<String>,
    pub access_control_request_method: Option<String>,
    pub access_control_request_headers: Option<String>,
}

#[derive(Debug, Clone, Default)]
/// Additional options for presigned object URLs.
pub struct PresignedUrlOptions {
    pub request_options: RequestOptions,
    pub auth_time: Option<crate::AuthTime>,
    pub sign_host: bool,
    pub sign_merged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Object identifier used in multi-delete XML.
#[serde(rename_all = "PascalCase")]
pub struct DeleteObject {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "VersionId")]
    pub version_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Multi-delete request body.
#[serde(rename = "Delete", rename_all = "PascalCase")]
pub struct DeleteMultiOptions {
    pub quiet: bool,
    #[serde(rename = "Object")]
    pub objects: Vec<DeleteObject>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Multi-delete response body.
#[serde(rename = "DeleteResult", rename_all = "PascalCase")]
pub struct DeleteMultiResult {
    #[serde(default, rename = "Deleted")]
    pub deleted: Vec<DeleteObject>,
    #[serde(default, rename = "Error")]
    pub errors: Vec<DeleteError>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct DeleteError {
    pub key: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Multipart upload initiation response.
#[serde(rename = "InitiateMultipartUploadResult", rename_all = "PascalCase")]
pub struct InitiateMultipartUploadResult {
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub upload_id: String,
}

#[derive(Debug, Clone, Default, Serialize)]
/// Query options for listing uploaded parts.
#[serde(rename_all = "camelCase")]
pub struct ListPartsOptions {
    #[serde(skip_serializing_if = "Option::is_none", rename = "part-number-marker")]
    pub part_number_marker: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "max-parts")]
    pub max_parts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "encoding-type")]
    pub encoding_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// Uploaded part list response.
#[serde(rename = "ListPartsResult", rename_all = "PascalCase")]
pub struct ListPartsResult {
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub upload_id: String,
    #[serde(default, rename = "Part")]
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Part {
    #[serde(default)]
    pub part_number: u32,
    #[serde(default)]
    pub last_modified: String,
    #[serde(default, rename = "ETag")]
    pub etag: String,
    #[serde(default)]
    pub size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Part descriptor used when completing a multipart upload.
#[serde(rename_all = "PascalCase")]
pub struct CompletePart {
    pub part_number: u32,
    #[serde(rename = "ETag")]
    pub etag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Complete multipart upload request body.
#[serde(rename = "CompleteMultipartUpload", rename_all = "PascalCase")]
pub struct CompleteMultipartUploadOptions {
    #[serde(rename = "Part")]
    pub parts: Vec<CompletePart>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Complete multipart upload response body.
#[serde(rename = "CompleteMultipartUploadResult", rename_all = "PascalCase")]
pub struct CompleteMultipartUploadResult {
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub key: String,
    #[serde(default, rename = "ETag")]
    pub etag: String,
}
