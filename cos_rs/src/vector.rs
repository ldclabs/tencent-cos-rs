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
    /// Server-side encryption type requested for the vector bucket.
    pub sse_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for creating a vector bucket.
#[serde(rename_all = "camelCase")]
pub struct CreateVectorBucketOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Optional server-side encryption configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_configuration: Option<VectorEncryptionConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Create vector bucket response.
#[serde(rename_all = "camelCase")]
pub struct CreateVectorBucketResult {
    /// QCS resource identifier of the created vector bucket.
    #[serde(default)]
    pub vector_bucket_qcs: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for getting vector bucket metadata.
#[serde(rename_all = "camelCase")]
pub struct GetVectorBucketOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Vector bucket metadata.
#[serde(rename_all = "camelCase")]
pub struct VectorBucketInfo {
    /// Creation timestamp returned by Vector.
    #[serde(default)]
    pub creation_time: i64,
    /// Server-side encryption configuration, when present.
    #[serde(default)]
    pub encryption_configuration: Option<VectorEncryptionConfig>,
    /// QCS resource identifier.
    #[serde(default)]
    pub vector_bucket_qcs: String,
    /// Vector bucket name.
    #[serde(default)]
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Response body for `GetVectorBucket`.
#[serde(rename_all = "camelCase")]
pub struct GetVectorBucketResult {
    /// Bucket metadata, when the bucket exists and is visible.
    #[serde(default)]
    pub vector_bucket: Option<VectorBucketInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for deleting a vector bucket.
#[serde(rename_all = "camelCase")]
pub struct DeleteVectorBucketOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for listing vector buckets.
#[serde(rename_all = "camelCase")]
pub struct ListVectorBucketsOptions {
    /// Maximum number of buckets to return. `0` omits the field.
    #[serde(skip_serializing_if = "is_zero")]
    pub max_results: i32,
    /// Pagination token from a previous response.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub next_token: String,
    /// Name prefix filter.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Vector bucket summary.
#[serde(rename_all = "camelCase")]
pub struct VectorBucketBrief {
    /// Creation timestamp returned by Vector.
    #[serde(default)]
    pub creation_time: i64,
    /// QCS resource identifier.
    #[serde(default)]
    pub vector_bucket_qcs: String,
    /// Vector bucket name.
    #[serde(default)]
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Response body for `ListVectorBuckets`.
#[serde(rename_all = "camelCase")]
pub struct ListVectorBucketsResult {
    /// Pagination token for the next page.
    #[serde(default)]
    pub next_token: String,
    /// Bucket summaries.
    #[serde(default)]
    pub vector_buckets: Vec<VectorBucketBrief>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for setting a vector bucket policy.
#[serde(rename_all = "camelCase")]
pub struct PutVectorBucketPolicyOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Policy JSON string.
    pub policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for getting a vector bucket policy.
#[serde(rename_all = "camelCase")]
pub struct GetVectorBucketPolicyOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Response body for `GetVectorBucketPolicy`.
#[serde(rename_all = "camelCase")]
pub struct GetVectorBucketPolicyResult {
    /// Policy JSON string.
    #[serde(default)]
    pub policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for deleting a vector bucket policy.
#[serde(rename_all = "camelCase")]
pub struct DeleteVectorBucketPolicyOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Metadata indexing configuration for a vector index.
#[serde(rename_all = "camelCase")]
pub struct MetadataConfiguration {
    /// Metadata keys that should not be filterable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_filterable_metadata_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for creating a vector index.
#[serde(rename_all = "camelCase")]
pub struct CreateIndexOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Index name.
    pub index_name: String,
    /// Vector data type, for example `float32`.
    pub data_type: String,
    /// Vector dimension.
    pub dimension: i32,
    /// Distance metric name understood by Vector.
    pub distance_metric: String,
    /// Optional metadata indexing configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_configuration: Option<MetadataConfiguration>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Response body for `CreateIndex`.
#[serde(rename_all = "camelCase")]
pub struct CreateIndexResult {
    /// QCS resource identifier of the created index.
    #[serde(default)]
    pub index_qcs: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Vector index metadata.
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    /// QCS resource identifier.
    #[serde(default)]
    pub index_qcs: String,
    /// Index name.
    #[serde(default)]
    pub index_name: String,
    /// Vector bucket name.
    #[serde(default)]
    pub vector_bucket_name: String,
    /// Creation timestamp returned by Vector.
    #[serde(default)]
    pub creation_time: i64,
    /// Vector data type.
    #[serde(default)]
    pub data_type: String,
    /// Vector dimension.
    #[serde(default)]
    pub dimension: i32,
    /// Distance metric.
    #[serde(default)]
    pub distance_metric: String,
    /// Metadata indexing configuration.
    #[serde(default)]
    pub metadata_configuration: Option<MetadataConfiguration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for getting vector index metadata.
#[serde(rename_all = "camelCase")]
pub struct GetIndexOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Index name.
    pub index_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Response body for `GetIndex`.
#[serde(rename_all = "camelCase")]
pub struct GetIndexResult {
    /// Index metadata, when present.
    #[serde(default)]
    pub index: Option<IndexInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for listing indexes.
#[serde(rename_all = "camelCase")]
pub struct ListIndexesOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Maximum number of indexes to return. `0` omits the field.
    #[serde(skip_serializing_if = "is_zero")]
    pub max_results: i32,
    /// Pagination token from a previous response.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub next_token: String,
    /// Index name prefix filter.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Vector index summary.
#[serde(rename_all = "camelCase")]
pub struct IndexBrief {
    /// Creation timestamp returned by Vector.
    #[serde(default)]
    pub creation_time: i64,
    /// QCS resource identifier.
    #[serde(default)]
    pub index_qcs: String,
    /// Index name.
    #[serde(default)]
    pub index_name: String,
    /// Vector bucket name.
    #[serde(default)]
    pub vector_bucket_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
/// Response body for `ListIndexes`.
#[serde(rename_all = "camelCase")]
pub struct ListIndexesResult {
    /// Index summaries.
    #[serde(default)]
    pub indexes: Vec<IndexBrief>,
    /// Pagination token for the next page.
    #[serde(default)]
    pub next_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Request body for deleting a vector index.
#[serde(rename_all = "camelCase")]
pub struct DeleteIndexOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Index name.
    pub index_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
/// Vector payload.
#[serde(rename_all = "camelCase")]
pub struct VectorData {
    /// Float32 vector values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub float32: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Vector written to an index.
#[serde(rename_all = "camelCase")]
pub struct InputVector {
    /// Caller-defined vector key.
    pub key: String,
    /// Vector payload.
    pub data: VectorData,
    /// Optional metadata document.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
/// Vector read from an index.
#[serde(rename_all = "camelCase")]
pub struct OutputVector {
    /// Caller-defined vector key.
    #[serde(default)]
    pub key: String,
    /// Vector payload, present when requested.
    #[serde(default)]
    pub data: Option<VectorData>,
    /// Metadata document, present when requested and available.
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Target index for writing vectors.
#[serde(rename_all = "camelCase")]
pub struct PutVectorsOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Index name.
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
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Index name.
    pub index_name: String,
    /// Whether to return vector payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_data: Option<bool>,
    /// Whether to return metadata documents.
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
/// Response body for `GetVectors`.
#[serde(rename_all = "camelCase")]
pub struct GetVectorsResult {
    /// Vectors returned by key.
    #[serde(default)]
    pub vectors: Vec<OutputVector>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Target index and paging options for listing vectors.
#[serde(rename_all = "camelCase")]
pub struct ListVectorsOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Index name.
    pub index_name: String,
    /// Maximum vectors to return. `0` omits the field.
    #[serde(skip_serializing_if = "is_zero")]
    pub max_results: i32,
    /// Pagination token from a previous response.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub next_token: String,
    /// Whether to return vector payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_data: Option<bool>,
    /// Whether to return metadata documents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_metadata: Option<bool>,
    /// Optional segmented-list count. Valid range is `1..=16` when non-zero.
    #[serde(skip_serializing_if = "is_zero")]
    pub segment_count: i32,
    /// Segment index in `[0, segment_count)` when segmented listing is used.
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
/// Response body for `ListVectors`.
#[serde(rename_all = "camelCase")]
pub struct ListVectorsResult {
    /// Vectors in this page.
    #[serde(default)]
    pub vectors: Vec<OutputVector>,
    /// Pagination token for the next page.
    #[serde(default)]
    pub next_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Target index for deleting vectors.
#[serde(rename_all = "camelCase")]
pub struct DeleteVectorsOptions {
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Index name.
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
    /// Vector bucket name.
    pub vector_bucket_name: String,
    /// Index name.
    pub index_name: String,
    /// Optional metadata filter expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    /// Whether to return vector payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_data: Option<bool>,
    /// Whether to return metadata documents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_metadata: Option<bool>,
    /// Whether to return distance values.
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
/// Vector returned by a similarity query.
#[serde(rename_all = "camelCase")]
pub struct QueryOutputVector {
    /// Caller-defined vector key.
    #[serde(default)]
    pub key: String,
    /// Vector payload, present when requested.
    #[serde(default)]
    pub data: Option<VectorData>,
    /// Metadata document, present when requested and available.
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
    /// Distance from the query vector.
    #[serde(default)]
    pub distance: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
/// Response body for `QueryVectors`.
#[serde(rename_all = "camelCase")]
pub struct QueryVectorsResult {
    /// Nearest vectors.
    #[serde(default)]
    pub vectors: Vec<QueryOutputVector>,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}
