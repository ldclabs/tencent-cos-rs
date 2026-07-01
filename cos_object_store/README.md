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
