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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagvalue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "max-keys")]
    pub max_keys: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "create-time")]
    pub create_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// COS owner identity.
#[serde(rename = "Owner", rename_all = "PascalCase")]
pub struct Owner {
    #[serde(default, rename = "ID")]
    pub id: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Bucket summary returned by `Get Service`.
#[serde(rename = "Bucket", rename_all = "PascalCase")]
pub struct Bucket {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "Location")]
    pub region: String,
    #[serde(default)]
    pub creation_date: String,
    #[serde(default, rename = "Type")]
    pub bucket_type_marker: String,
    #[serde(default)]
    pub bucket_type: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Bucket list wrapper returned by COS XML.
#[serde(rename_all = "PascalCase")]
pub struct Buckets {
    #[serde(default, rename = "Bucket")]
    pub buckets: Vec<Bucket>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
/// Parsed `Get Service` result.
#[serde(rename = "ListAllMyBucketsResult", rename_all = "PascalCase")]
pub struct ServiceGetResult {
    #[serde(default)]
    pub owner: Option<Owner>,
    #[serde(default)]
    pub buckets: Buckets,
    #[serde(default)]
    pub marker: String,
    #[serde(default)]
    pub next_marker: String,
    #[serde(default)]
    pub is_truncated: bool,
}
