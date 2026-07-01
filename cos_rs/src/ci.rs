//! CI (Cloud Infinite) XML helpers.
//!
//! Implemented helpers cover object CI GET/POST requests, CI job creation,
//! CI job description, and a generic `send_xml` escape hatch for CI XML
//! endpoints.

use crate::client::{Client, Endpoint};
use crate::encoding::{RequestOptions, encode_key, query_from_serialize};
use crate::error::Result;
use crate::response::Response;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
/// CI API entry point.
pub struct CiService {
    client: Client,
}

impl CiService {
    pub(crate) fn new(client: Client) -> Self {
        Self { client }
    }

    /// Send a CI GET request against a bucket object path.
    pub async fn get_object_ci<R: DeserializeOwned>(
        &self,
        key: &str,
        options: impl Serialize,
    ) -> Result<(R, Response)> {
        let mut request_options = RequestOptions::new();
        request_options.query = query_from_serialize(&options)?;
        self.client
            .get_xml(
                Endpoint::Bucket,
                Method::GET,
                &format!("/{}", encode_key(key, true)),
                request_options,
            )
            .await
    }

    /// Send a CI POST request against a bucket object path.
    pub async fn post_object_ci<T, R>(&self, key: &str, body: &T) -> Result<(R, Response)>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let response = self
            .client
            .send_xml_body(
                Endpoint::Bucket,
                Method::POST,
                &format!("/{}", encode_key(key, true)),
                RequestOptions::new(),
                body,
            )
            .await?;
        self.client.parse_xml(response).await
    }

    /// Create a CI job at a caller-supplied CI path.
    pub async fn create_job<T, R>(&self, path: &str, body: &T) -> Result<(R, Response)>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let response = self
            .client
            .send_xml_body(
                Endpoint::Ci,
                Method::POST,
                path,
                RequestOptions::new(),
                body,
            )
            .await?;
        self.client.parse_xml(response).await
    }

    /// Describe a CI resource at a caller-supplied CI path.
    pub async fn describe<R: DeserializeOwned>(&self, path: &str) -> Result<(R, Response)> {
        self.client
            .get_xml(Endpoint::Ci, Method::GET, path, RequestOptions::new())
            .await
    }

    /// Generic XML helper for CI-style endpoints.
    pub async fn send_xml<T, R>(
        &self,
        endpoint: Endpoint,
        method: Method,
        path: &str,
        query: Option<impl Serialize>,
        body: Option<&T>,
    ) -> Result<(R, Response)>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let mut options = RequestOptions::new();
        if let Some(query) = query {
            options.query = query_from_serialize(&query)?;
        }
        let response = if let Some(body) = body {
            self.client
                .send_xml_body(endpoint, method, path, options, body)
                .await?
        } else {
            self.client
                .send(endpoint, method, path, options, None, None)
                .await?
        };
        self.client.parse_xml(response).await
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Picture processing operation list.
#[serde(rename_all = "PascalCase")]
pub struct PicOperations {
    /// Picture processing rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "Rules")]
    pub rules: Vec<PicOperationsRule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Single picture processing rule.
#[serde(rename_all = "PascalCase")]
pub struct PicOperationsRule {
    /// Output object key for this processing rule.
    #[serde(default)]
    pub file_id: String,
    /// CI picture processing rule string.
    #[serde(default)]
    pub rule: String,
}
