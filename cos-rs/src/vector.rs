//! Vector service JSON APIs.
//!
//! Implemented operations cover Vector endpoint URL helpers, vector bucket
//! management, bucket policy management, index management, vector put/get/list
//! /delete, and vector similarity search.

use crate::client::{BaseUrl, Client, Endpoint};
use crate::error::{Error, Result};
use crate::response::Response;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use url::Url;

#[derive(Clone)]
/// Vector API entry point.
pub struct VectorService {
    client: Client,
}

impl VectorService {
    pub(crate) fn new(client: Client) -> Self {
        Self { client }
    }

    /// Build the public Vector endpoint URL for a region.
    pub fn public_url(region: &str, secure: bool) -> Result<Url> {
        BaseUrl::vector_url(region, secure)
    }

    /// Build the internal Vector endpoint URL for a region.
    pub fn internal_url(region: &str, secure: bool) -> Result<Url> {
        BaseUrl::vector_internal_url(region, secure)
    }

    /// Normalize a custom Vector endpoint URL.
    pub fn endpoint_url(endpoint: &str) -> Result<Url> {
        BaseUrl::vector_endpoint_url(endpoint)
    }

    /// Create a vector bucket.
    pub async fn create_vector_bucket(
        &self,
        options: &CreateVectorBucketOptions,
    ) -> Result<(CreateVectorBucketResult, Response)> {
        self.json_post("/CreateVectorBucket", options).await
    }

    /// Get vector bucket metadata.
    pub async fn get_vector_bucket(
        &self,
        options: &GetVectorBucketOptions,
    ) -> Result<(GetVectorBucketResult, Response)> {
        self.json_post("/GetVectorBucket", options).await
    }

    /// Delete a vector bucket.
    pub async fn delete_vector_bucket(
        &self,
        options: &DeleteVectorBucketOptions,
    ) -> Result<Response> {
        self.json_post_no_body("/DeleteVectorBucket", options).await
    }

    /// List vector buckets.
    pub async fn list_vector_buckets(
        &self,
        options: Option<&ListVectorBucketsOptions>,
    ) -> Result<(ListVectorBucketsResult, Response)> {
        self.json_post("/ListVectorBuckets", &options.cloned().unwrap_or_default())
            .await
    }

    /// Set a vector bucket policy.
    pub async fn put_vector_bucket_policy(
        &self,
        options: &PutVectorBucketPolicyOptions,
    ) -> Result<Response> {
        self.json_post_no_body("/PutVectorBucketPolicy", options)
            .await
    }

    /// Get a vector bucket policy.
    pub async fn get_vector_bucket_policy(
        &self,
        options: &GetVectorBucketPolicyOptions,
    ) -> Result<(GetVectorBucketPolicyResult, Response)> {
        self.json_post("/GetVectorBucketPolicy", options).await
    }

    /// Delete a vector bucket policy.
    pub async fn delete_vector_bucket_policy(
        &self,
        options: &DeleteVectorBucketPolicyOptions,
    ) -> Result<Response> {
        self.json_post_no_body("/DeleteVectorBucketPolicy", options)
            .await
    }

    /// Create a vector index.
    pub async fn create_index(
        &self,
        options: &CreateIndexOptions,
    ) -> Result<(CreateIndexResult, Response)> {
        self.json_post("/CreateIndex", options).await
    }

    /// Get vector index metadata.
    pub async fn get_index(&self, options: &GetIndexOptions) -> Result<(GetIndexResult, Response)> {
        self.json_post("/GetIndex", options).await
    }

    /// List vector indexes in a bucket.
    pub async fn list_indexes(
        &self,
        options: &ListIndexesOptions,
    ) -> Result<(ListIndexesResult, Response)> {
        self.json_post("/ListIndexes", options).await
    }

    /// Delete a vector index.
    pub async fn delete_index(&self, options: &DeleteIndexOptions) -> Result<Response> {
        self.json_post_no_body("/DeleteIndex", options).await
    }

    /// Insert or update vectors.
    pub async fn put_vectors(
        &self,
        options: &PutVectorsOptions,
        vectors: Vec<InputVector>,
    ) -> Result<Response> {
        if vectors.is_empty() {
            return Err(Error::InvalidInput("vectors param is empty".to_owned()));
        }
        let body = PutVectorsRequest {
            vector_bucket_name: options.vector_bucket_name.clone(),
            index_name: options.index_name.clone(),
            vectors,
        };
        self.json_post_no_body("/PutVectors", &body).await
    }

    /// Get vectors by key.
    pub async fn get_vectors(
        &self,
        options: &GetVectorsOptions,
        keys: Vec<String>,
    ) -> Result<(GetVectorsResult, Response)> {
        if keys.is_empty() {
            return Err(Error::InvalidInput("GetVectors: keys is empty".to_owned()));
        }
        let body = GetVectorsRequest {
            vector_bucket_name: options.vector_bucket_name.clone(),
            index_name: options.index_name.clone(),
            keys,
            return_data: options.return_data,
            return_metadata: options.return_metadata,
        };
        self.json_post("/GetVectors", &body).await
    }

    /// List vectors, optionally using segmented listing.
    pub async fn list_vectors(
        &self,
        options: &ListVectorsOptions,
    ) -> Result<(ListVectorsResult, Response)> {
        if options.segment_count == 0 && options.segment_index != 0 {
            return Err(Error::InvalidInput(
                "ListVectors: segmentIndex requires segmentCount".to_owned(),
            ));
        }
        if options.segment_count != 0 {
            if !(1..=16).contains(&options.segment_count) {
                return Err(Error::InvalidInput(
                    "ListVectors: segmentCount must be in [1,16]".to_owned(),
                ));
            }
            if options.segment_index < 0 || options.segment_index >= options.segment_count {
                return Err(Error::InvalidInput(
                    "ListVectors: segmentIndex must be in [0,segmentCount)".to_owned(),
                ));
            }
        }
        let body = ListVectorsRequest {
            vector_bucket_name: options.vector_bucket_name.clone(),
            index_name: options.index_name.clone(),
            max_results: options.max_results,
            next_token: options.next_token.clone(),
            return_data: options.return_data,
            return_metadata: options.return_metadata,
            segment_count: (options.segment_count != 0).then_some(options.segment_count),
            segment_index: (options.segment_count != 0).then_some(options.segment_index),
        };
        self.json_post("/ListVectors", &body).await
    }

    /// Delete vectors by key.
    pub async fn delete_vectors(
        &self,
        options: &DeleteVectorsOptions,
        keys: Vec<String>,
    ) -> Result<Response> {
        if keys.is_empty() {
            return Err(Error::InvalidInput(
                "DeleteVectors: keys is empty".to_owned(),
            ));
        }
        let body = DeleteVectorsRequest {
            vector_bucket_name: options.vector_bucket_name.clone(),
            index_name: options.index_name.clone(),
            keys,
        };
        self.json_post_no_body("/DeleteVectors", &body).await
    }

    /// Query nearest vectors.
    pub async fn query_vectors(
        &self,
        options: &QueryVectorsOptions,
        query_vector: VectorData,
        top_k: i32,
    ) -> Result<(QueryVectorsResult, Response)> {
        if top_k <= 0 {
            return Err(Error::InvalidInput(
                "topK must be greater than 0".to_owned(),
            ));
        }
        let body = QueryVectorsRequest {
            vector_bucket_name: options.vector_bucket_name.clone(),
            index_name: options.index_name.clone(),
            query_vector,
            top_k,
            filter: options.filter.clone(),
            return_data: options.return_data,
            return_metadata: options.return_metadata,
            return_distance: options.return_distance,
        };
        self.json_post("/QueryVectors", &body).await
    }

    async fn json_post<T, R>(&self, path: &str, body: &T) -> Result<(R, Response)>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let response = self
            .client
            .send_json_body(
                Endpoint::Vector,
                Method::POST,
                path,
                Default::default(),
                body,
            )
            .await?;
        self.client.parse_json(response).await
    }

    async fn json_post_no_body<T>(&self, path: &str, body: &T) -> Result<Response>
    where
        T: Serialize,
    {
        self.client
            .send_json_body(
                Endpoint::Vector,
                Method::POST,
                path,
                Default::default(),
                body,
            )
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Vector bucket encryption configuration.
#[serde(rename_all = "camelCase")]
pub struct VectorEncryptionConfig {
    pub sse_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for creating a vector bucket.
#[serde(rename_all = "camelCase")]
pub struct CreateVectorBucketOptions {
    pub vector_bucket_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_configuration: Option<VectorEncryptionConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Create vector bucket response.
#[serde(rename_all = "camelCase")]
pub struct CreateVectorBucketResult {
    #[serde(default)]
    pub vector_bucket_qcs: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for getting vector bucket metadata.
#[serde(rename_all = "camelCase")]
pub struct GetVectorBucketOptions {
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Vector bucket metadata.
#[serde(rename_all = "camelCase")]
pub struct VectorBucketInfo {
    #[serde(default)]
    pub creation_time: i64,
    #[serde(default)]
    pub encryption_configuration: Option<VectorEncryptionConfig>,
    #[serde(default)]
    pub vector_bucket_qcs: String,
    #[serde(default)]
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetVectorBucketResult {
    #[serde(default)]
    pub vector_bucket: Option<VectorBucketInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for deleting a vector bucket.
#[serde(rename_all = "camelCase")]
pub struct DeleteVectorBucketOptions {
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for listing vector buckets.
#[serde(rename_all = "camelCase")]
pub struct ListVectorBucketsOptions {
    #[serde(skip_serializing_if = "is_zero")]
    pub max_results: i32,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub next_token: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VectorBucketBrief {
    #[serde(default)]
    pub creation_time: i64,
    #[serde(default)]
    pub vector_bucket_qcs: String,
    #[serde(default)]
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListVectorBucketsResult {
    #[serde(default)]
    pub next_token: String,
    #[serde(default)]
    pub vector_buckets: Vec<VectorBucketBrief>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutVectorBucketPolicyOptions {
    pub vector_bucket_name: String,
    pub policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetVectorBucketPolicyOptions {
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetVectorBucketPolicyResult {
    #[serde(default)]
    pub policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteVectorBucketPolicyOptions {
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataConfiguration {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_filterable_metadata_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for creating a vector index.
#[serde(rename_all = "camelCase")]
pub struct CreateIndexOptions {
    pub vector_bucket_name: String,
    pub index_name: String,
    pub data_type: String,
    pub dimension: i32,
    pub distance_metric: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_configuration: Option<MetadataConfiguration>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateIndexResult {
    #[serde(default)]
    pub index_qcs: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    #[serde(default)]
    pub index_qcs: String,
    #[serde(default)]
    pub index_name: String,
    #[serde(default)]
    pub vector_bucket_name: String,
    #[serde(default)]
    pub creation_time: i64,
    #[serde(default)]
    pub data_type: String,
    #[serde(default)]
    pub dimension: i32,
    #[serde(default)]
    pub distance_metric: String,
    #[serde(default)]
    pub metadata_configuration: Option<MetadataConfiguration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetIndexOptions {
    pub vector_bucket_name: String,
    pub index_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetIndexResult {
    #[serde(default)]
    pub index: Option<IndexInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListIndexesOptions {
    pub vector_bucket_name: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub max_results: i32,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub next_token: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexBrief {
    #[serde(default)]
    pub creation_time: i64,
    #[serde(default)]
    pub index_qcs: String,
    #[serde(default)]
    pub index_name: String,
    #[serde(default)]
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListIndexesResult {
    #[serde(default)]
    pub indexes: Vec<IndexBrief>,
    #[serde(default)]
    pub next_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteIndexOptions {
    pub vector_bucket_name: String,
    pub index_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
/// Vector payload.
#[serde(rename_all = "camelCase")]
pub struct VectorData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub float32: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Vector written to an index.
#[serde(rename_all = "camelCase")]
pub struct InputVector {
    pub key: String,
    pub data: VectorData,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
/// Vector read from an index.
#[serde(rename_all = "camelCase")]
pub struct OutputVector {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub data: Option<VectorData>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Target index for writing vectors.
#[serde(rename_all = "camelCase")]
pub struct PutVectorsOptions {
    pub vector_bucket_name: String,
    pub index_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PutVectorsRequest {
    vector_bucket_name: String,
    index_name: String,
    vectors: Vec<InputVector>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Target index and return flags for reading vectors.
#[serde(rename_all = "camelCase")]
pub struct GetVectorsOptions {
    pub vector_bucket_name: String,
    pub index_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_data: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_metadata: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GetVectorsRequest {
    vector_bucket_name: String,
    index_name: String,
    keys: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_data: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_metadata: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GetVectorsResult {
    #[serde(default)]
    pub vectors: Vec<OutputVector>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Target index and paging options for listing vectors.
#[serde(rename_all = "camelCase")]
pub struct ListVectorsOptions {
    pub vector_bucket_name: String,
    pub index_name: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub max_results: i32,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub next_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_data: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_metadata: Option<bool>,
    #[serde(skip_serializing_if = "is_zero")]
    pub segment_count: i32,
    #[serde(skip_serializing_if = "is_zero")]
    pub segment_index: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListVectorsRequest {
    vector_bucket_name: String,
    index_name: String,
    #[serde(skip_serializing_if = "is_zero")]
    max_results: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    next_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_data: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    segment_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    segment_index: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListVectorsResult {
    #[serde(default)]
    pub vectors: Vec<OutputVector>,
    #[serde(default)]
    pub next_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteVectorsOptions {
    pub vector_bucket_name: String,
    pub index_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteVectorsRequest {
    vector_bucket_name: String,
    index_name: String,
    keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Vector query options.
#[serde(rename_all = "camelCase")]
pub struct QueryVectorsOptions {
    pub vector_bucket_name: String,
    pub index_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_data: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_distance: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryVectorsRequest {
    vector_bucket_name: String,
    index_name: String,
    query_vector: VectorData,
    top_k: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_data: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_distance: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryOutputVector {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub data: Option<VectorData>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
    #[serde(default)]
    pub distance: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryVectorsResult {
    #[serde(default)]
    pub vectors: Vec<QueryOutputVector>,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}
