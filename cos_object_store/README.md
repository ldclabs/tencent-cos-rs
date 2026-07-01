# cos_object_store

Tencent Cloud COS adapter for the Rust `object_store` trait.

This crate wraps `cos_rs` and maps common `object_store` operations to COS XML
APIs: object read/write/head/delete/copy, listing, paginated listing,
multipart upload, conditional writes, metadata attributes, tags, and signed
URLs.

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

    let result = store.get(&Path::from("docs/readme.txt")).await?;
    println!("{:?}", result.meta);
    Ok(())
}
```

`TencentCosBuilder::from_env()` recognizes `COS_BUCKET` or
`TENCENT_COS_BUCKET`, `COS_REGION` or `TENCENT_COS_REGION`, `COS_BUCKET_URL` or
`TENCENT_COS_BUCKET_URL`, plus `COS_SECRETID`, `COS_SECRETKEY`, and optional
`COS_SESSION_TOKEN`.

## Conditional Writes

`object_store::PutMode::Update` is not implemented for COS. Direct `PUT Object`
does not support destination-side `If-Match` / `If-None-Match`, and `PUT Object
- Copy` only applies `x-cos-copy-source-If-*` conditions to the source object.
That copy API can atomically copy an existing object or update metadata, but it
cannot replace the target object with a new `PutPayload`.

`PutMode::Create` and `CopyMode::Create` use `x-cos-forbid-overwrite: true`
where COS supports it. When bucket versioning is enabled, COS documents that
`x-cos-forbid-overwrite` is ineffective, so these modes cannot provide a
cross-client atomic create-if-absent guarantee on versioned buckets.

For buckets that enable versioning, configure lifecycle rules to periodically
clean old non-current versions; otherwise workloads can accumulate historical
versions over time.
