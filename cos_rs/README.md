# cos-rs

Tencent Cloud Object Storage (COS) XML API V5 Rust SDK.

`cos-rs` is an async-first SDK that follows the service grouping of
`cos-go-sdk-v5` while using Rust-native builders, `Result`, `serde`, and owned
response bodies.

## Packages

This workspace contains two crates:

- `cos_rs`: the async Tencent COS XML API V5 SDK.
- `cos_object_store`: an [`object_store`](https://docs.rs/object_store) adapter
  built on top of `cos_rs`.

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
- Rustdoc coverage for the public SDK surface.

Not provided:

- A blocking API. The public API is async-only.
- A fully generated typed wrapper for every CI, Batch, and MetaInsight request
  shape. Those families currently expose generic XML helpers so callers can use
  typed `serde` structs.

## Installation

For a workspace or Git dependency:

```toml
[dependencies]
cos_rs = { git = "https://github.com/ldclabs/tencent-cos-rs", package = "cos_rs" }

# Optional object_store integration:
cos_object_store = { git = "https://github.com/ldclabs/tencent-cos-rs", package = "cos_object_store" }
```

The SDK is async-only and expects a Tokio runtime for network and file helper
methods.

## Client Setup

Most bucket/object APIs require a bucket endpoint. COS bucket names use the
`{name}-{appid}` format:

```rust,no_run
use cos_rs::{BaseUrl, Client, Credential};

fn client() -> Result<Client, Box<dyn std::error::Error>> {
    let bucket_url = BaseUrl::bucket_url("example-1250000000", "ap-guangzhou", true)?;

    Ok(Client::builder()
        .bucket_url(bucket_url)
        .credential(Credential::new("secret-id", "secret-key"))
        .build()?)
}
```

To load credentials from the environment, use `EnvCredentialProvider`:

```rust,no_run
use cos_rs::{BaseUrl, Client, EnvCredentialProvider};
use std::sync::Arc;

fn client_from_env() -> Result<Client, Box<dyn std::error::Error>> {
    let bucket = std::env::var("COS_TEST_BUCKET")?;
    let region = std::env::var("COS_TEST_REGION")?;

    Ok(Client::builder()
        .bucket_url(BaseUrl::bucket_url(&bucket, &region, true)?)
        .credential_provider(Arc::new(EnvCredentialProvider::default()))
        .build()?)
}
```

Recognized credential variables:

- `COS_SECRETID`
- `COS_SECRETKEY`
- `COS_SESSION_TOKEN` for temporary STS credentials

`BaseUrl::new()` includes the default Service endpoint. Configure additional
families only when you use them, for example `BaseUrl::vector_url(...)` for
Vector or `ClientBuilder::ci_url(...)` for Cloud Infinite.

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
- `EnvCredentialProvider` using `COS_SECRETID`, `COS_SECRETKEY`, and optional
  `COS_SESSION_TOKEN`.
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

## Common Object Operations

`Response` contains the HTTP status, headers, final URL, and a fully buffered
body. The simple file helpers also buffer the whole object before writing or
uploading, so prefer lower-level streaming outside this crate for very large
objects.

```rust,no_run
use cos_rs::{BaseUrl, BucketGetOptions, Client, Credential, ObjectPutOptions};

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

    let (page, _) = client
        .bucket()
        .get(Some(BucketGetOptions {
            prefix: Some("docs/".to_owned()),
            max_keys: Some(100),
            ..Default::default()
        }))
        .await?;

    for object in page.contents {
        println!("{} {}", object.key, object.size);
    }

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

    let response = client.object().get("docs/readme.txt", None).await?;
    println!("{}", response.text()?);
    Ok(())
}
```

## Bucket Subresources

Common bucket subresources such as CORS, lifecycle, tagging, encryption,
website, logging, and accelerate are exposed as generic XML helpers. Define
serde structs matching the COS XML shape and pass them to the helper:

```rust,no_run
use cos_rs::{BaseUrl, Client, Credential};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "CORSConfiguration", rename_all = "PascalCase")]
struct CorsConfiguration {
    #[serde(rename = "CORSRule")]
    rules: Vec<CorsRule>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CorsRule {
    allowed_origin: String,
    allowed_method: String,
    allowed_header: String,
}

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

    let cors = CorsConfiguration {
        rules: vec![CorsRule {
            allowed_origin: "*".to_owned(),
            allowed_method: "GET".to_owned(),
            allowed_header: "*".to_owned(),
        }],
    };

    client.bucket().put_cors(&cors).await?;
    let (_cors, _response): (CorsConfiguration, _) = client.bucket().get_cors().await?;
    Ok(())
}
```

The lower-level `get_subresource`, `put_subresource`, and `delete_subresource`
methods are available for COS subresources not yet covered by convenience
methods.

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

Use `PresignedUrlOptions` when the URL needs extra signed query parameters or
headers. Temporary credentials should include the session token in the query
options when the URL will be used by a caller that cannot add headers.

## Error Handling

COS XML APIs return `Error::Api` on non-2xx responses. Vector JSON APIs return
`Error::Vector`. Both retain status, headers, request id, and raw body.

```rust,no_run
use cos_rs::{Error, Response};

async fn read_optional(
    client: &cos_rs::Client,
    key: &str,
) -> Result<Option<Response>, cos_rs::Error> {
    match client.object().get(key, None).await {
        Ok(response) => Ok(Some(response)),
        Err(Error::Api(response)) if response.is_not_found() => Ok(None),
        Err(err) => Err(err),
    }
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

## Client-Side Encryption

`Client::crypto(master)` wraps object `put`, `get`, and `delete` with
client-side envelope encryption metadata. `LocalMasterCipher` is useful for
tests or controlled local deployments; `KmsMasterCipher` uses the included
minimal TencentCloud KMS TC3 client.

```rust,no_run
use cos_rs::crypto::LocalMasterCipher;
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
        .credential(Credential::new("secret-id", "secret-key"))
        .build()?;

    let master = LocalMasterCipher::new([7; 32], r#"{"name":"local"}"#);
    let crypto = client.crypto(master);

    crypto.object().put("secret.txt", "plaintext", None).await?;
    let response = crypto.object().get("secret.txt", None).await?;
    println!("{}", response.text()?);
    Ok(())
}
```

## object_store Adapter

The `cos_object_store` crate exposes a COS bucket as an `object_store::ObjectStore`.
It supports object reads/writes, conditional create/update, copy, delete,
listing, paginated listing, multipart upload, and signed URLs.

```rust,no_run
use bytes::Bytes;
use cos_object_store::TencentCos;
use object_store::ObjectStoreExt;
use object_store::path::Path;

#[tokio::main]
async fn main() -> object_store::Result<()> {
    let store = TencentCos::builder()
        .with_bucket_name("example-1250000000")
        .with_region("ap-guangzhou")
        .with_secret_id_and_key("secret-id", "secret-key")
        .build()?;

    store
        .put(&Path::from("docs/readme.txt"), Bytes::from_static(b"hello").into())
        .await?;

    let meta = store.head(&Path::from("docs/readme.txt")).await?;
    println!("{} bytes", meta.size);
    Ok(())
}
```

## Tests

Run formatting and the offline SDK tests:

```bash
cargo fmt --check
cargo test -p cos_rs --test offline
cargo test -p cos_object_store
```

The Go SDK parity inventory test expects a sibling checkout at
`../cos-go-sdk-v5` and currently scans first-party Go SDK test names. When that
checkout exists, the full suite can be run with:

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

The `live-tests` feature is reserved for COS/KMS integration tests gated by
environment variables such as `COS_SECRETID`, `COS_SECRETKEY`, `KMSID`,
`COS_TEST_BUCKET`, and `COS_TEST_REGION`.
