# cos-rs

Tencent Cloud COS XML API V5 Rust SDK.

`cos-rs` is an async-first SDK that follows the service grouping of
`cos-go-sdk-v5` while using Rust-native builders, `Result`, `serde`, and owned
response bodies.

## Current Status

Implemented and tested in the current crate:

- Async COS client based on `reqwest`.
- Endpoint configuration for Service, Bucket, Batch, CI, Fetch, MetaInsight,
  and Vector.
- Static credentials, environment credentials, custom async credential
  providers, and temporary session token signing.
- COS Authorization V5 signing and object presigned URL generation.
- Retry configuration and COS domain switch support for retryable failures.
- COS XML error parsing and Vector JSON error parsing.
- Owned response wrapper with status/header/body helpers.
- Offline test coverage for core request construction and XML parsing.
- Go SDK first-party test parity inventory, excluding `vendor/**` and
  `example/**`.

Not provided:

- A blocking API. The public API is async-only.
- A fully generated typed wrapper for every CI, Batch, and MetaInsight request
  shape. Those families currently expose generic XML helpers so callers can use
  typed `serde` structs.

## Implemented Feature List

### Core Client

- `Client`, `ClientBuilder`, `BaseUrl`, `Config`, `RetryOptions`.
- Custom `reqwest::Client`.
- Custom user agent and Host override.
- `client.service()`, `client.bucket()`, `client.object()`, `client.batch()`,
  `client.ci()`, `client.meta_insight()`, `client.vector()`, and
  `client.crypto(...)`.

### Authentication

- `Credential::new(secret_id, secret_key)`.
- `Credential::with_token(secret_id, secret_key, token)`.
- `StaticCredentialProvider`.
- `EnvCredentialProvider` using:
  - `COS_SECRETID`
  - `COS_SECRETKEY`
  - `COS_SESSION_TOKEN` (optional)
- `AuthTime` for deterministic signing windows.
- `authorization(...)` and `add_authorization_headers(...)`.

### Service APIs

- `Get Service`: list buckets for the signing account.

### Bucket APIs

- `Get Bucket`: list objects.
- `Put Bucket`.
- `Delete Bucket`.
- `Head Bucket`.
- Bucket existence check.
- `Get Object Versions`.
- `List Multipart Uploads`.
- `Get/Put Bucket ACL` using ACL headers.
- `Get/Put Bucket Versioning`.
- `Get Bucket Location`.
- `Get/Put/Delete Bucket Policy`.
- Generic subresource helpers:
  - `get_subresource`
  - `put_subresource`
  - `delete_subresource`
- Convenience methods built on the generic helpers:
  - CORS get/put/delete
  - Lifecycle get/put/delete
  - Tagging get/put/delete
  - Encryption get/put/delete
  - Website get/put/delete
  - Logging get/put
  - Accelerate get/put

### Object APIs

- `Get Object`.
- `Get Object To File`.
- `Put Object`.
- `Put Object From File`.
- `Delete Object`.
- `Head Object`.
- Object existence check.
- `Options Object`.
- `Put Object Copy`.
- `Delete Multiple Objects`.
- Multipart upload lifecycle:
  - Initiate multipart upload
  - Upload part
  - List parts
  - Complete multipart upload
  - Abort multipart upload
- Object URL construction.
- Object presigned URL construction.

### Batch APIs

Generic XML helpers for:

- Create job.
- Describe job.
- List jobs.
- Update job priority.
- Update job status.

### CI APIs

Generic XML helpers for:

- Object CI GET requests.
- Object CI POST requests.
- CI job creation.
- CI resource/job description.
- Generic `send_xml` for CI-style endpoints.

Included CI data helpers:

- `PicOperations`.
- `PicOperationsRule`.

### MetaInsight APIs

Generic XML helpers for:

- POST.
- GET.
- DELETE.

### Vector APIs

- Public, internal, and custom Vector endpoint URL helpers.
- Vector bucket management:
  - Create vector bucket
  - Get vector bucket
  - Delete vector bucket
  - List vector buckets
- Vector bucket policy management:
  - Put policy
  - Get policy
  - Delete policy
- Vector index management:
  - Create index
  - Get index
  - List indexes
  - Delete index
- Vector data operations:
  - Put vectors
  - Get vectors
  - List vectors
  - Delete vectors
  - Query vectors
- Validation for empty vector/key lists and segmented list bounds.

### Client-Side Crypto

- `MasterCipher` trait.
- `LocalMasterCipher` for local/test key wrapping.
- `KmsClient`, a minimal TencentCloud KMS TC3 client.
- `KmsMasterCipher`.
- `CryptoClient`.
- Encrypted object put/get/delete.
- COS client-side encryption metadata headers.

## Async Usage

```rust,no_run
use cos_rs::{BaseUrl, Client, Credential};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut base = BaseUrl::new();
    base.bucket = Some(BaseUrl::bucket_url(
        "example-1250000000",
        "ap-guangzhou",
        true,
    )?);

    let client = Client::builder()
        .base_url(base)
        .credential(Credential::new(
            std::env::var("COS_SECRETID")?,
            std::env::var("COS_SECRETKEY")?,
        ))
        .build()?;

    let response = client.object().get("test/hello.txt", None).await?;
    println!("{}", response.text()?);
    Ok(())
}
```

## Upload Example

```rust,no_run
use cos_rs::{BaseUrl, Client, Credential, ObjectPutOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut base = BaseUrl::new();
    base.bucket = Some(BaseUrl::bucket_url(
        "example-1250000000",
        "ap-guangzhou",
        true,
    )?);

    let client = Client::builder()
        .base_url(base)
        .credential(Credential::new("secret-id", "secret-key"))
        .build()?;

    client
        .object()
        .put(
            "docs/readme.txt",
            "hello cos",
            Some(ObjectPutOptions {
                content_type: Some("text/plain".to_owned()),
                ..Default::default()
            }),
        )
        .await?;

    Ok(())
}
```

## Presigned URL Example

```rust,no_run
use cos_rs::{BaseUrl, Client, Credential};
use reqwest::Method;
use std::time::Duration;

fn build_url() -> Result<url::Url, Box<dyn std::error::Error>> {
    let mut base = BaseUrl::new();
    base.bucket = Some(BaseUrl::bucket_url(
        "example-1250000000",
        "ap-guangzhou",
        true,
    )?);

    let client = Client::new(base)?;
    let credential = Credential::new("secret-id", "secret-key");

    Ok(client.object().get_presigned_url(
        Method::GET,
        "test/hello.txt",
        &credential,
        Duration::from_secs(3600),
        None,
    )?)
}
```

## Vector Example

```rust,no_run
use cos_rs::{BaseUrl, Client, Credential, ListVectorBucketsOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut base = BaseUrl::new();
    base.vector = Some(BaseUrl::vector_url("ap-guangzhou", true)?);

    let client = Client::builder()
        .base_url(base)
        .credential(Credential::new("secret-id", "secret-key"))
        .build()?;

    let (result, _) = client
        .vector()
        .list_vector_buckets(Some(&ListVectorBucketsOptions::default()))
        .await?;

    println!("{} buckets", result.vector_buckets.len());
    Ok(())
}
```

## Tests

Default tests are offline:

```bash
cargo test
```

Recommended validation before sending changes:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

The `live-tests` feature is reserved for COS/KMS integration tests gated by
environment variables such as `COS_SECRETID`, `COS_SECRETKEY`, `KMSID`,
`COS_TEST_BUCKET`, and `COS_TEST_REGION`.
