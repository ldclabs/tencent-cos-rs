//! MetaInsight XML helpers.
//!
//! Implemented operations are generic `post`, `get`, and `delete` helpers for
//! MetaInsight paths. Callers supply typed `serde` request/response structs.

use crate::client::{Client, Endpoint};
use crate::encoding::RequestOptions;
use crate::error::Result;
use crate::response::Response;
use reqwest::Method;
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Clone)]
/// MetaInsight API entry point.
pub struct MetaInsightService {
    client: Client,
}

impl MetaInsightService {
    pub(crate) fn new(client: Client) -> Self {
        Self { client }
    }

    /// Send a MetaInsight XML POST request.
    pub async fn post<T, R>(&self, path: &str, body: &T) -> Result<(R, Response)>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let response = self
            .client
            .send_xml_body(
                Endpoint::MetaInsight,
                Method::POST,
                path,
                RequestOptions::new(),
                body,
            )
            .await?;
        self.client.parse_xml(response).await
    }

    /// Send a MetaInsight XML GET request.
    pub async fn get<R: DeserializeOwned>(&self, path: &str) -> Result<(R, Response)> {
        self.client
            .get_xml(
                Endpoint::MetaInsight,
                Method::GET,
                path,
                RequestOptions::new(),
            )
            .await
    }

    /// Send a MetaInsight DELETE request.
    pub async fn delete(&self, path: &str) -> Result<Response> {
        self.client
            .send(
                Endpoint::MetaInsight,
                Method::DELETE,
                path,
                RequestOptions::new(),
                None,
                None,
            )
            .await
    }
}
