# tencent-cos-rs

Rust SDK workspace for Tencent Cloud Object Storage (COS).

## Crates

- [`cos_rs`](cos_rs/README.md): async COS XML API V5 client with bucket,
  object, service, Batch, CI, MetaInsight, Vector, signing, and client-side
  encryption helpers.
- [`cos_object_store`](cos_object_store/README.md): adapter that exposes a COS
  bucket through the `object_store::ObjectStore` trait.

## Quick Start

```rust,no_run
use cos_rs::{BaseUrl, Client, Credential};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .bucket_url(BaseUrl::bucket_url(
            "example-1250000000",
            "ap-guangzhou",
            true,
        )?)
        .credential(Credential::new("secret-id", "secret-key"))
        .build()?;

    let response = client.object().get("docs/readme.txt", None).await?;
    println!("{}", response.text()?);
    Ok(())
}
```

See [`cos_rs/README.md`](cos_rs/README.md) for setup details, supported API
families, error handling, Vector, client-side encryption, and `object_store`
usage.
