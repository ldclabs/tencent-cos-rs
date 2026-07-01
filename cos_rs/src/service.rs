//! Service-level COS APIs.
//!
//! Currently implemented: `Get Service` for listing buckets owned by the
//! signing account.

use crate::client::{Client, Endpoint};
use crate::encoding::{RequestOptions, query_from_serialize};
use crate::error::Result;
use crate::response::Response;
use reqwest::Method;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
/// Service API entry point.
pub struct ServiceService {
    client: Client,
}

impl ServiceService {
    pub(crate) fn new(client: Client) -> Self {
        Self { client }
    }

    /// List buckets visible to the current credential.
    pub async fn get(
        &self,
        options: Option<ServiceGetOptions>,
    ) -> Result<(ServiceGetResult, Response)> {
        let mut request_options = RequestOptions::new();
        if let Some(options) = options {
            request_options.query = query_from_serialize(&options)?;
        }
        self.client
            .get_xml(Endpoint::Service, Method::GET, "/", request_options)
            .await
    }
}

#[derive(Debug, Clone, Default, Serialize)]
/// Query parameters for `Get Service`.
#[serde(rename_all = "kebab-case")]
pub struct ServiceGetOptions {
    /// Filter buckets by tag key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagkey: Option<String>,
    /// Filter buckets by tag value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagvalue: Option<String>,
    /// Maximum number of buckets to return.
    #[serde(skip_serializing_if = "Option::is_none", rename = "max-keys")]
    pub max_keys: Option<i64>,
    /// Pagination marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// Region range filter used by COS Service API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<String>,
    /// Creation-time filter understood by COS Service API.
    #[serde(skip_serializing_if = "Option::is_none", rename = "create-time")]
    pub create_time: Option<i64>,
    /// Region filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// COS owner identity.
#[serde(rename = "Owner", rename_all = "PascalCase")]
pub struct Owner {
    /// Owner UIN or canonical COS owner id.
    #[serde(default, rename = "ID")]
    pub id: String,
    /// Human-readable owner name, when returned by COS.
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Bucket summary returned by `Get Service`.
#[serde(rename = "Bucket", rename_all = "PascalCase")]
pub struct Bucket {
    /// Bucket name in `{name}-{appid}` format.
    #[serde(default)]
    pub name: String,
    /// Bucket region.
    #[serde(default, rename = "Location")]
    pub region: String,
    /// Bucket creation time as returned by COS.
    #[serde(default)]
    pub creation_date: String,
    /// Raw type marker returned by COS.
    #[serde(default, rename = "Type")]
    pub bucket_type_marker: String,
    /// Bucket type, when returned by COS.
    #[serde(default)]
    pub bucket_type: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Bucket list wrapper returned by COS XML.
#[serde(rename_all = "PascalCase")]
pub struct Buckets {
    /// Bucket entries.
    #[serde(default, rename = "Bucket")]
    pub buckets: Vec<Bucket>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Parsed `Get Service` result.
#[serde(rename = "ListAllMyBucketsResult", rename_all = "PascalCase")]
pub struct ServiceGetResult {
    /// Account owner.
    #[serde(default)]
    pub owner: Option<Owner>,
    /// Bucket list wrapper.
    #[serde(default)]
    pub buckets: Buckets,
    /// Current page marker.
    #[serde(default)]
    pub marker: String,
    /// Marker for the next page.
    #[serde(default)]
    pub next_marker: String,
    /// Whether more buckets are available.
    #[serde(default)]
    pub is_truncated: bool,
}
