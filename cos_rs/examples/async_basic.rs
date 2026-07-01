use cos_rs::{BaseUrl, Client, Credential};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bucket = std::env::var("COS_TEST_BUCKET").unwrap_or_else(|_| "example-1250000000".into());
    let region = std::env::var("COS_TEST_REGION").unwrap_or_else(|_| "ap-guangzhou".into());

    let mut base = BaseUrl::new();
    base.bucket = Some(BaseUrl::bucket_url(&bucket, &region, true)?);

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
