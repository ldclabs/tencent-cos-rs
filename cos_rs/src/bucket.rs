//! Bucket APIs and common bucket subresource helpers.
//!
//! Implemented direct methods cover bucket get/put/delete/head/existence,
//! object versions, multipart-upload listing, ACL, versioning, policy,
//! location, and common XML subresources such as CORS, lifecycle, tagging,
//! encryption, website, logging, and accelerate.

use crate::client::{Client, Endpoint, empty_put_post_body, status_is_not_found};
use crate::encoding::{RequestOptions, insert_header, query_from_serialize};
use crate::error::Result;
use crate::response::Response;
use crate::service::Owner;
use bytes::Bytes;
use reqwest::Method;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
/// Bucket API entry point.
pub struct BucketService {
    client: Client,
}

impl BucketService {
    pub(crate) fn new(client: Client) -> Self {
        Self { client }
    }

    /// List objects in the bucket.
    pub async fn get(
        &self,
        options: Option<BucketGetOptions>,
    ) -> Result<(BucketGetResult, Response)> {
        let options = request_options(options.as_ref(), None::<&BucketPutOptions>)?;
        self.client
            .get_xml(Endpoint::Bucket, Method::GET, "/", options)
            .await
    }

    /// Create the bucket, optionally with a create-bucket configuration body.
    pub async fn put(&self, options: Option<BucketPutOptions>) -> Result<Response> {
        let body = options
            .as_ref()
            .and_then(|o| o.create_bucket_configuration.as_ref());
        let request_options = request_options(None::<&BucketGetOptions>, options.as_ref())?;
        if let Some(body) = body {
            self.client
                .send_xml_body(Endpoint::Bucket, Method::PUT, "/", request_options, body)
                .await
        } else {
            self.client
                .send(
                    Endpoint::Bucket,
                    Method::PUT,
                    "/",
                    request_options,
                    empty_put_post_body(&Method::PUT),
                    None,
                )
                .await
        }
    }

    /// Delete the bucket.
    pub async fn delete(&self) -> Result<Response> {
        self.client
            .send(
                Endpoint::Bucket,
                Method::DELETE,
                "/",
                RequestOptions::new(),
                None,
                None,
            )
            .await
    }

    /// Check bucket metadata and permissions with `HEAD`.
    pub async fn head(&self) -> Result<Response> {
        self.client
            .send(
                Endpoint::Bucket,
                Method::HEAD,
                "/",
                RequestOptions::new(),
                None,
                None,
            )
            .await
    }

    /// Return `false` only for a COS 404; other errors are returned.
    pub async fn is_exist(&self) -> Result<bool> {
        match self.head().await {
            Ok(_) => Ok(true),
            Err(err) if status_is_not_found(&err) => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// List object versions with the `versions` subresource.
    pub async fn get_object_versions(
        &self,
        options: Option<BucketGetObjectVersionsOptions>,
    ) -> Result<(BucketGetObjectVersionsResult, Response)> {
        let mut request_options = RequestOptions::new().raw_query("versions");
        if let Some(options) = options {
            request_options.query = query_from_serialize(&options)?;
        }
        self.client
            .get_xml(Endpoint::Bucket, Method::GET, "/", request_options)
            .await
    }

    /// List in-progress multipart uploads with the `uploads` subresource.
    pub async fn list_multipart_uploads(
        &self,
        options: Option<ListMultipartUploadsOptions>,
    ) -> Result<(ListMultipartUploadsResult, Response)> {
        let mut request_options = RequestOptions::new().raw_query("uploads");
        if let Some(options) = options {
            request_options.query = query_from_serialize(&options)?;
        }
        self.client
            .get_xml(Endpoint::Bucket, Method::GET, "/", request_options)
            .await
    }

    /// Generic XML getter for bucket subresources not modeled as concrete methods.
    pub async fn get_subresource<T: DeserializeOwned>(
        &self,
        subresource: &str,
    ) -> Result<(T, Response)> {
        self.client
            .get_xml(
                Endpoint::Bucket,
                Method::GET,
                "/",
                RequestOptions::new().raw_query(subresource),
            )
            .await
    }

    /// Generic XML setter for bucket subresources not modeled as concrete methods.
    pub async fn put_subresource<T: Serialize>(
        &self,
        subresource: &str,
        body: &T,
        headers: Option<HeaderMap>,
    ) -> Result<Response> {
        let mut options = RequestOptions::new().raw_query(subresource);
        if let Some(headers) = headers {
            options.headers = headers;
        }
        self.client
            .send_xml_body(Endpoint::Bucket, Method::PUT, "/", options, body)
            .await
    }

    /// Generic deleter for bucket subresources.
    pub async fn delete_subresource(&self, subresource: &str) -> Result<Response> {
        self.client
            .send(
                Endpoint::Bucket,
                Method::DELETE,
                "/",
                RequestOptions::new().raw_query(subresource),
                None,
                None,
            )
            .await
    }

    /// Get bucket ACL.
    pub async fn get_acl(&self) -> Result<(AclXml, Response)> {
        self.get_subresource("acl").await
    }

    /// Put bucket ACL using ACL headers.
    pub async fn put_acl(&self, options: BucketPutAclOptions) -> Result<Response> {
        let mut request_options = RequestOptions::new().raw_query("acl");
        options.apply(&mut request_options)?;
        self.client
            .send(
                Endpoint::Bucket,
                Method::PUT,
                "/",
                request_options,
                empty_put_post_body(&Method::PUT),
                None,
            )
            .await
    }

    /// Get bucket CORS configuration into a caller-provided XML type.
    pub async fn get_cors<T: DeserializeOwned>(&self) -> Result<(T, Response)> {
        self.get_subresource("cors").await
    }

    /// Put bucket CORS configuration from a caller-provided XML type.
    pub async fn put_cors<T: Serialize>(&self, body: &T) -> Result<Response> {
        self.put_subresource("cors", body, None).await
    }

    /// Delete bucket CORS configuration.
    pub async fn delete_cors(&self) -> Result<Response> {
        self.delete_subresource("cors").await
    }

    /// Get bucket lifecycle configuration into a caller-provided XML type.
    pub async fn get_lifecycle<T: DeserializeOwned>(&self) -> Result<(T, Response)> {
        self.get_subresource("lifecycle").await
    }

    /// Put bucket lifecycle configuration from a caller-provided XML type.
    pub async fn put_lifecycle<T: Serialize>(&self, body: &T) -> Result<Response> {
        self.put_subresource("lifecycle", body, None).await
    }

    /// Delete bucket lifecycle configuration.
    pub async fn delete_lifecycle(&self) -> Result<Response> {
        self.delete_subresource("lifecycle").await
    }

    /// Get bucket tagging configuration into a caller-provided XML type.
    pub async fn get_tagging<T: DeserializeOwned>(&self) -> Result<(T, Response)> {
        self.get_subresource("tagging").await
    }

    /// Put bucket tagging configuration from a caller-provided XML type.
    pub async fn put_tagging<T: Serialize>(&self, body: &T) -> Result<Response> {
        self.put_subresource("tagging", body, None).await
    }

    /// Delete bucket tagging configuration.
    pub async fn delete_tagging(&self) -> Result<Response> {
        self.delete_subresource("tagging").await
    }

    /// Set bucket versioning status, commonly `Enabled` or `Suspended`.
    pub async fn put_versioning(&self, status: impl AsRef<str>) -> Result<Response> {
        let body = BucketVersioningConfiguration {
            status: status.as_ref().to_owned(),
        };
        self.put_subresource("versioning", &body, None).await
    }

    /// Get bucket versioning status.
    pub async fn get_versioning(&self) -> Result<(BucketVersioningConfiguration, Response)> {
        self.get_subresource("versioning").await
    }

    /// Put a raw JSON bucket policy document.
    pub async fn put_policy(&self, policy_json: impl Into<Bytes>) -> Result<Response> {
        self.client
            .send(
                Endpoint::Bucket,
                Method::PUT,
                "/",
                RequestOptions::new().raw_query("policy"),
                Some(policy_json.into()),
                Some("application/json"),
            )
            .await
    }

    /// Get the raw JSON bucket policy response.
    pub async fn get_policy(&self) -> Result<Response> {
        self.client
            .send(
                Endpoint::Bucket,
                Method::GET,
                "/",
                RequestOptions::new().raw_query("policy"),
                None,
                None,
            )
            .await
    }

    /// Delete bucket policy.
    pub async fn delete_policy(&self) -> Result<Response> {
        self.delete_subresource("policy").await
    }

    /// Get bucket encryption configuration into a caller-provided XML type.
    pub async fn get_encryption<T: DeserializeOwned>(&self) -> Result<(T, Response)> {
        self.get_subresource("encryption").await
    }

    /// Put bucket encryption configuration from a caller-provided XML type.
    pub async fn put_encryption<T: Serialize>(&self, body: &T) -> Result<Response> {
        self.put_subresource("encryption", body, None).await
    }

    /// Delete bucket encryption configuration.
    pub async fn delete_encryption(&self) -> Result<Response> {
        self.delete_subresource("encryption").await
    }

    /// Get bucket website configuration into a caller-provided XML type.
    pub async fn get_website<T: DeserializeOwned>(&self) -> Result<(T, Response)> {
        self.get_subresource("website").await
    }

    /// Put bucket website configuration from a caller-provided XML type.
    pub async fn put_website<T: Serialize>(&self, body: &T) -> Result<Response> {
        self.put_subresource("website", body, None).await
    }

    /// Delete bucket website configuration.
    pub async fn delete_website(&self) -> Result<Response> {
        self.delete_subresource("website").await
    }

    /// Get bucket logging configuration into a caller-provided XML type.
    pub async fn get_logging<T: DeserializeOwned>(&self) -> Result<(T, Response)> {
        self.get_subresource("logging").await
    }

    /// Put bucket logging configuration from a caller-provided XML type.
    pub async fn put_logging<T: Serialize>(&self, body: &T) -> Result<Response> {
        self.put_subresource("logging", body, None).await
    }

    /// Get bucket location.
    pub async fn get_location(&self) -> Result<(BucketLocationResult, Response)> {
        self.get_subresource("location").await
    }

    /// Get bucket accelerate configuration into a caller-provided XML type.
    pub async fn get_accelerate<T: DeserializeOwned>(&self) -> Result<(T, Response)> {
        self.get_subresource("accelerate").await
    }

    /// Put bucket accelerate configuration from a caller-provided XML type.
    pub async fn put_accelerate<T: Serialize>(&self, body: &T) -> Result<Response> {
        self.put_subresource("accelerate", body, None).await
    }
}

fn request_options<Q, H>(query: Option<Q>, headers: Option<H>) -> Result<RequestOptions>
where
    Q: Serialize,
    H: ApplyHeaders,
{
    let mut options = RequestOptions::new();
    if let Some(query) = query {
        options.query = query_from_serialize(&query)?;
    }
    if let Some(headers) = headers {
        headers.apply(&mut options)?;
    }
    Ok(options)
}

trait ApplyHeaders {
    fn apply(&self, options: &mut RequestOptions) -> Result<()>;
}

impl<T: ApplyHeaders + ?Sized> ApplyHeaders for &T {
    fn apply(&self, options: &mut RequestOptions) -> Result<()> {
        (*self).apply(options)
    }
}

impl ApplyHeaders for BucketPutOptions {
    fn apply(&self, options: &mut RequestOptions) -> Result<()> {
        if let Some(value) = &self.x_cos_acl {
            insert_header(&mut options.headers, "x-cos-acl", value)?;
        }
        if let Some(value) = &self.x_cos_grant_read {
            insert_header(&mut options.headers, "x-cos-grant-read", value)?;
        }
        if let Some(value) = &self.x_cos_grant_write {
            insert_header(&mut options.headers, "x-cos-grant-write", value)?;
        }
        if let Some(value) = &self.x_cos_grant_full_control {
            insert_header(&mut options.headers, "x-cos-grant-full-control", value)?;
        }
        if let Some(value) = &self.x_cos_tagging {
            insert_header(&mut options.headers, "x-cos-tagging", value)?;
        }
        options.headers.extend(self.extra_headers.clone());
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
/// Query options for listing objects in a bucket.
#[serde(rename_all = "kebab-case")]
pub struct BucketGetOptions {
    /// Return only keys with this prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Group keys by this delimiter, commonly `/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    /// Optional response encoding, commonly `url`.
    #[serde(skip_serializing_if = "Option::is_none", rename = "encoding-type")]
    pub encoding_type: Option<String>,
    /// Pagination marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// Maximum number of keys to return.
    #[serde(skip_serializing_if = "Option::is_none", rename = "max-keys")]
    pub max_keys: Option<i32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// Parsed `Get Bucket` result.
#[serde(rename = "ListBucketResult", rename_all = "PascalCase")]
pub struct BucketGetResult {
    /// Bucket name.
    #[serde(default)]
    pub name: String,
    /// Prefix echoed by COS.
    #[serde(default)]
    pub prefix: String,
    /// Current page marker.
    #[serde(default)]
    pub marker: String,
    /// Marker for the next page.
    #[serde(default)]
    pub next_marker: String,
    /// Delimiter echoed by COS.
    #[serde(default)]
    pub delimiter: String,
    /// Maximum keys echoed by COS.
    #[serde(default)]
    pub max_keys: i32,
    /// Whether more keys are available.
    #[serde(default)]
    pub is_truncated: bool,
    /// Object entries in this page.
    #[serde(default, rename = "Contents")]
    pub contents: Vec<Object>,
    /// Grouped prefixes when a delimiter is used.
    #[serde(default, rename = "CommonPrefixes")]
    pub common_prefixes: Vec<CommonPrefix>,
    /// Response encoding type.
    #[serde(default)]
    pub encoding_type: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// Common prefix group returned by `Get Bucket` when a delimiter is used.
#[serde(rename_all = "PascalCase")]
pub struct CommonPrefix {
    /// Prefix value.
    #[serde(default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// Object summary returned by bucket listing APIs.
#[serde(rename_all = "PascalCase")]
pub struct Object {
    /// Object key.
    #[serde(default)]
    pub key: String,
    /// Entity tag returned by COS.
    #[serde(default, rename = "ETag")]
    pub etag: String,
    /// Object size in bytes.
    #[serde(default)]
    pub size: i64,
    /// Last modification timestamp as returned by COS.
    #[serde(default)]
    pub last_modified: String,
    /// COS storage class.
    #[serde(default)]
    pub storage_class: String,
    /// Object owner, when returned by COS.
    #[serde(default)]
    pub owner: Option<Owner>,
}

#[derive(Debug, Clone, Default)]
/// Headers and optional body for `Put Bucket`.
pub struct BucketPutOptions {
    /// Canned ACL header (`x-cos-acl`).
    pub x_cos_acl: Option<String>,
    /// Read grant header.
    pub x_cos_grant_read: Option<String>,
    /// Write grant header.
    pub x_cos_grant_write: Option<String>,
    /// Full-control grant header.
    pub x_cos_grant_full_control: Option<String>,
    /// URL-encoded tagging header.
    pub x_cos_tagging: Option<String>,
    /// Additional headers not modeled by this struct.
    pub extra_headers: HeaderMap,
    /// Optional XML create-bucket body.
    pub create_bucket_configuration: Option<CreateBucketConfiguration>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Optional body used when creating a bucket.
#[serde(rename = "CreateBucketConfiguration", rename_all = "PascalCase")]
pub struct CreateBucketConfiguration {
    /// Availability-zone placement configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_az_config: Option<String>,
    /// Bucket archive architecture configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_arch_config: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
/// Query options for bucket object versions.
#[serde(rename_all = "kebab-case")]
pub struct BucketGetObjectVersionsOptions {
    /// Return only versioned keys with this prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Group keys by this delimiter, commonly `/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    /// Optional response encoding, commonly `url`.
    #[serde(skip_serializing_if = "Option::is_none", rename = "encoding-type")]
    pub encoding_type: Option<String>,
    /// Pagination key marker.
    #[serde(skip_serializing_if = "Option::is_none", rename = "key-marker")]
    pub key_marker: Option<String>,
    /// Pagination version marker.
    #[serde(skip_serializing_if = "Option::is_none", rename = "version-id-marker")]
    pub version_id_marker: Option<String>,
    /// Maximum number of entries to return.
    #[serde(skip_serializing_if = "Option::is_none", rename = "max-keys")]
    pub max_keys: Option<i32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// Parsed `Get Object Versions` result.
#[serde(rename = "ListVersionsResult", rename_all = "PascalCase")]
pub struct BucketGetObjectVersionsResult {
    /// Bucket name.
    #[serde(default)]
    pub name: String,
    /// Prefix echoed by COS.
    #[serde(default)]
    pub prefix: String,
    /// Current key marker.
    #[serde(default)]
    pub key_marker: String,
    /// Current version id marker.
    #[serde(default)]
    pub version_id_marker: String,
    /// Maximum keys echoed by COS.
    #[serde(default)]
    pub max_keys: i32,
    /// Whether more versions are available.
    #[serde(default)]
    pub is_truncated: bool,
    /// Key marker for the next page.
    #[serde(default)]
    pub next_key_marker: String,
    /// Version marker for the next page.
    #[serde(default)]
    pub next_version_id_marker: String,
    /// Object versions in this page.
    #[serde(default, rename = "Version")]
    pub versions: Vec<ListVersionsResultVersion>,
    /// Delete markers in this page.
    #[serde(default, rename = "DeleteMarker")]
    pub delete_markers: Vec<ListVersionsResultDeleteMarker>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// Object version entry returned by `Get Object Versions`.
#[serde(rename_all = "PascalCase")]
pub struct ListVersionsResultVersion {
    /// Object key.
    #[serde(default)]
    pub key: String,
    /// Version id.
    #[serde(default)]
    pub version_id: String,
    /// Whether this is the latest version.
    #[serde(default)]
    pub is_latest: bool,
    /// Last modification timestamp as returned by COS.
    #[serde(default)]
    pub last_modified: String,
    /// Entity tag.
    #[serde(default, rename = "ETag")]
    pub etag: String,
    /// Object size in bytes.
    #[serde(default)]
    pub size: i64,
    /// COS storage class.
    #[serde(default)]
    pub storage_class: String,
    /// Object owner, when returned by COS.
    #[serde(default)]
    pub owner: Option<Owner>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// Delete-marker entry returned by `Get Object Versions`.
#[serde(rename_all = "PascalCase")]
pub struct ListVersionsResultDeleteMarker {
    /// Object key.
    #[serde(default)]
    pub key: String,
    /// Version id of the delete marker.
    #[serde(default)]
    pub version_id: String,
    /// Whether this is the latest version marker.
    #[serde(default)]
    pub is_latest: bool,
    /// Last modification timestamp as returned by COS.
    #[serde(default)]
    pub last_modified: String,
    /// Marker owner, when returned by COS.
    #[serde(default)]
    pub owner: Option<Owner>,
}

#[derive(Debug, Clone, Default, Serialize)]
/// Query options for listing multipart uploads.
#[serde(rename_all = "kebab-case")]
pub struct ListMultipartUploadsOptions {
    /// Optional response encoding, commonly `url`.
    #[serde(skip_serializing_if = "Option::is_none", rename = "encoding-type")]
    pub encoding_type: Option<String>,
    /// Return only multipart uploads whose keys have this prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Group uploads by delimiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    /// Pagination key marker.
    #[serde(skip_serializing_if = "Option::is_none", rename = "key-marker")]
    pub key_marker: Option<String>,
    /// Pagination upload id marker.
    #[serde(skip_serializing_if = "Option::is_none", rename = "upload-id-marker")]
    pub upload_id_marker: Option<String>,
    /// Maximum uploads to return.
    #[serde(skip_serializing_if = "Option::is_none", rename = "max-uploads")]
    pub max_uploads: Option<i32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// Parsed `List Multipart Uploads` result.
#[serde(rename = "ListMultipartUploadsResult", rename_all = "PascalCase")]
pub struct ListMultipartUploadsResult {
    /// Bucket name.
    #[serde(default)]
    pub bucket: String,
    /// Response encoding type.
    #[serde(default)]
    pub encoding_type: String,
    /// Current key marker.
    #[serde(default)]
    pub key_marker: String,
    /// Current upload id marker.
    #[serde(default)]
    pub upload_id_marker: String,
    /// Key marker for the next page.
    #[serde(default)]
    pub next_key_marker: String,
    /// Upload id marker for the next page.
    #[serde(default)]
    pub next_upload_id_marker: String,
    /// Maximum uploads echoed by COS.
    #[serde(default)]
    pub max_uploads: i32,
    /// Whether more uploads are available.
    #[serde(default)]
    pub is_truncated: bool,
    /// Multipart uploads in this page.
    #[serde(default, rename = "Upload")]
    pub uploads: Vec<MultipartUpload>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// In-progress multipart upload summary.
#[serde(rename_all = "PascalCase")]
pub struct MultipartUpload {
    /// Object key.
    #[serde(default)]
    pub key: String,
    /// Upload id.
    #[serde(default)]
    pub upload_id: String,
    /// COS storage class.
    #[serde(default)]
    pub storage_class: String,
    /// Upload initiation timestamp as returned by COS.
    #[serde(default)]
    pub initiated: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// COS ACL XML document.
#[serde(rename = "AccessControlPolicy", rename_all = "PascalCase")]
pub struct AclXml {
    /// ACL owner.
    #[serde(default)]
    pub owner: Option<Owner>,
    /// Access grants.
    #[serde(default)]
    pub access_control_list: AccessControlList,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// COS ACL grant list.
#[serde(rename_all = "PascalCase")]
pub struct AccessControlList {
    /// Individual grants.
    #[serde(default, rename = "Grant")]
    pub grants: Vec<Grant>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// COS ACL grant.
#[serde(rename_all = "PascalCase")]
pub struct Grant {
    /// Grant recipient.
    #[serde(default)]
    pub grantee: Option<Grantee>,
    /// Permission string, for example `READ` or `FULL_CONTROL`.
    #[serde(default)]
    pub permission: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
/// COS ACL grantee.
#[serde(rename_all = "PascalCase")]
pub struct Grantee {
    /// Grantee id.
    #[serde(default)]
    pub id: String,
    /// Display name, when returned by COS.
    #[serde(default)]
    pub display_name: String,
    /// Predefined group URI, when the grantee is a group.
    #[serde(default, rename = "URI")]
    pub uri: String,
}

#[derive(Debug, Clone, Default)]
/// Header options for `Put Bucket ACL`.
pub struct BucketPutAclOptions {
    /// Canned ACL header (`x-cos-acl`).
    pub x_cos_acl: Option<String>,
    /// Read grant header.
    pub x_cos_grant_read: Option<String>,
    /// Write grant header.
    pub x_cos_grant_write: Option<String>,
    /// Full-control grant header.
    pub x_cos_grant_full_control: Option<String>,
    /// Additional ACL headers not modeled by this struct.
    pub extra_headers: HeaderMap,
}

impl BucketPutAclOptions {
    fn apply(&self, options: &mut RequestOptions) -> Result<()> {
        if let Some(value) = &self.x_cos_acl {
            insert_header(&mut options.headers, "x-cos-acl", value)?;
        }
        if let Some(value) = &self.x_cos_grant_read {
            insert_header(&mut options.headers, "x-cos-grant-read", value)?;
        }
        if let Some(value) = &self.x_cos_grant_write {
            insert_header(&mut options.headers, "x-cos-grant-write", value)?;
        }
        if let Some(value) = &self.x_cos_grant_full_control {
            insert_header(&mut options.headers, "x-cos-grant-full-control", value)?;
        }
        options.headers.extend(self.extra_headers.clone());
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Bucket versioning state.
#[serde(rename = "VersioningConfiguration", rename_all = "PascalCase")]
pub struct BucketVersioningConfiguration {
    /// Versioning status returned by COS, commonly `Enabled` or `Suspended`.
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Bucket location response.
#[serde(rename = "LocationConstraint")]
pub struct BucketLocationResult {
    /// COS region string.
    #[serde(rename = "$text", default)]
    pub location: String,
}
