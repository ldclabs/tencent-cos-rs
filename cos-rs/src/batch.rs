//! Batch operation APIs.
//!
//! Implemented operations cover job creation, job description, job listing,
//! job priority updates, and job status updates. Request/response bodies are
//! generic `serde` XML structs so callers can model newer Batch shapes without
//! waiting for a crate release.

use crate::client::{Client, Endpoint};
use crate::encoding::{RequestOptions, query_from_serialize};
use crate::error::Result;
use crate::response::Response;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
/// Batch API entry point.
pub struct BatchService {
    client: Client,
}

impl BatchService {
    pub(crate) fn new(client: Client) -> Self {
        Self { client }
    }

    /// Create a Batch job using a caller-supplied XML request/response type.
    pub async fn create_job<T, R>(
        &self,
        body: &T,
        headers: Option<BatchRequestHeaders>,
    ) -> Result<(R, Response)>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let response = self
            .client
            .send_xml_body(
                Endpoint::Batch,
                Method::POST,
                "/jobs",
                headers.map(Into::into).unwrap_or_default(),
                body,
            )
            .await?;
        self.client.parse_xml(response).await
    }

    /// Describe a Batch job.
    pub async fn describe_job<R: DeserializeOwned>(
        &self,
        job_id: &str,
        headers: Option<BatchRequestHeaders>,
    ) -> Result<(R, Response)> {
        self.client
            .get_xml(
                Endpoint::Batch,
                Method::GET,
                &format!("/jobs/{job_id}"),
                headers.map(Into::into).unwrap_or_default(),
            )
            .await
    }

    /// List Batch jobs.
    pub async fn list_jobs<R: DeserializeOwned>(
        &self,
        options: Option<BatchListJobsOptions>,
        headers: Option<BatchRequestHeaders>,
    ) -> Result<(R, Response)> {
        let mut request_options: RequestOptions = headers.map(Into::into).unwrap_or_default();
        if let Some(options) = options {
            request_options.query = query_from_serialize(&options)?;
        }
        self.client
            .get_xml(Endpoint::Batch, Method::GET, "/jobs", request_options)
            .await
    }

    /// Update Batch job priority.
    pub async fn update_job_priority(
        &self,
        job_id: &str,
        priority: i32,
        headers: Option<BatchRequestHeaders>,
    ) -> Result<Response> {
        let body = BatchUpdatePriority { priority };
        self.client
            .send_xml_body(
                Endpoint::Batch,
                Method::POST,
                &format!("/jobs/{job_id}/priority"),
                headers.map(Into::into).unwrap_or_default(),
                &body,
            )
            .await
    }

    /// Update Batch job status.
    pub async fn update_job_status(
        &self,
        job_id: &str,
        requested_job_status: impl Into<String>,
        status_update_reason: impl Into<String>,
        headers: Option<BatchRequestHeaders>,
    ) -> Result<Response> {
        let body = BatchUpdateStatus {
            requested_job_status: requested_job_status.into(),
            status_update_reason: status_update_reason.into(),
        };
        self.client
            .send_xml_body(
                Endpoint::Batch,
                Method::POST,
                &format!("/jobs/{job_id}/status"),
                headers.map(Into::into).unwrap_or_default(),
                &body,
            )
            .await
    }
}

#[derive(Debug, Clone, Default)]
/// Headers commonly required by Batch APIs.
pub struct BatchRequestHeaders {
    pub x_cos_appid: Option<i64>,
    pub content_length: Option<String>,
    pub content_type: Option<String>,
    pub extra_headers: reqwest::header::HeaderMap,
}

impl From<BatchRequestHeaders> for RequestOptions {
    fn from(value: BatchRequestHeaders) -> Self {
        let mut options = RequestOptions::new();
        if let Some(appid) = value.x_cos_appid {
            let _ = crate::encoding::insert_header(
                &mut options.headers,
                "x-cos-appid",
                &appid.to_string(),
            );
        }
        if let Some(content_length) = value.content_length {
            let _ = crate::encoding::insert_header(
                &mut options.headers,
                "Content-Length",
                &content_length,
            );
        }
        if let Some(content_type) = value.content_type {
            let _ =
                crate::encoding::insert_header(&mut options.headers, "Content-Type", &content_type);
        }
        options.headers.extend(value.extra_headers);
        options
    }
}

#[derive(Debug, Clone, Default, Serialize)]
/// Query options for listing Batch jobs.
#[serde(rename_all = "camelCase")]
pub struct BatchListJobsOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_statuses: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "UpdateJobPriority", rename_all = "PascalCase")]
pub struct BatchUpdatePriority {
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "UpdateJobStatus", rename_all = "PascalCase")]
pub struct BatchUpdateStatus {
    pub requested_job_status: String,
    pub status_update_reason: String,
}
