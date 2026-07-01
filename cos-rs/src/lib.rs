//! Tencent Cloud COS XML API V5 Rust SDK.
//!
//! The crate exposes an async-first client. It intentionally keeps the Go SDK's
//! service grouping while using Rust builders, `Result`, `serde`, and owned
//! response bodies.
//!
//! # Implemented surface
//!
//! - Client construction, endpoint configuration, retry options, custom
//!   `reqwest::Client`, user agent, host override, static credentials, and
//!   credential providers.
//! - Tencent COS Authorization V5 signing, temporary-token headers, and object
//!   presigned URLs.
//! - Service API: list buckets.
//! - Bucket API: get, put, delete, head, existence check, object versions,
//!   multipart upload listing, ACL, versioning, location, policy, and common
//!   XML subresource helpers for CORS, lifecycle, tagging, encryption, website,
//!   logging, and accelerate.
//! - Object API: get, get to file, put, put from file, delete, head, existence
//!   check, OPTIONS, copy, multi-delete, multipart upload lifecycle, object URL,
//!   and presigned URL creation.
//! - Batch, CI, and MetaInsight generic XML request helpers for implemented
//!   endpoint families whose option/result shapes can be supplied with `serde`
//!   structs.
//! - Vector JSON API: vector bucket management, bucket policy management, index
//!   management, vector put/get/list/delete, and vector similarity query.
//! - Client-side crypto scaffolding: `MasterCipher`, local AES-CTR master
//!   cipher, Tencent KMS master cipher, crypto object put/get/delete, and the
//!   minimal TencentCloud TC3 KMS client used by the KMS master cipher.
//! - Response helpers plus COS XML and Vector JSON error types.
//!
//! # Example
//!
//! ```no_run
//! use cos_rs::{BaseUrl, Client, Credential};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let mut base = BaseUrl::new();
//! base.bucket = Some(BaseUrl::bucket_url("example-1250000000", "ap-guangzhou", true)?);
//!
//! let client = Client::builder()
//!     .base_url(base)
//!     .credential(Credential::new("secret-id", "secret-key"))
//!     .build()?;
//!
//! let response = client.object().get("test/hello.txt", None).await?;
//! println!("{}", response.text()?);
//! # Ok(())
//! # }
//! ```

mod auth;
mod batch;
mod bucket;
mod ci;
mod client;
pub mod crypto;
mod encoding;
mod error;
mod metainsight;
mod object;
mod response;
mod service;
mod vector;

pub use auth::{
    AuthTime, Credential, CredentialProvider, EnvCredentialProvider, StaticCredentialProvider,
    add_authorization_headers, authorization,
};
pub use batch::*;
pub use bucket::*;
pub use ci::*;
pub use client::{BaseUrl, Client, ClientBuilder, Config, Endpoint, RetryOptions};
pub use encoding::{
    Bool, HeaderOptions, QueryOptions, RequestOptions, encode_component, encode_key,
};
pub use error::{Error, ErrorResponse, Result, VectorErrorResponse, VectorValidateField};
pub use metainsight::*;
pub use object::*;
pub use response::Response;
pub use service::*;
pub use vector::*;

/// Current crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default `User-Agent` prefix sent by the SDK.
pub const USER_AGENT: &str = concat!("cos-rs/", env!("CARGO_PKG_VERSION"));
