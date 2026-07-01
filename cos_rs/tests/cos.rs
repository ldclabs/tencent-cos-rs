use cos_rs::{BaseUrl, BucketGetOptions, Client, Credential, ObjectPutOptions};
use std::env;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn client_from_env() -> TestResult<Client> {
    dotenv::dotenv().ok();

    let bucket = required_env("COS_BUCKET")?;
    let region = required_env("COS_REGION")?;
    let credential = credential_from_env()?;

    Ok(Client::builder()
        .bucket_url(BaseUrl::bucket_url(&bucket, &region, true)?)
        .credential(credential)
        .build()?)
}

fn credential_from_env() -> TestResult<Credential> {
    let secret_id = required_env("COS_SECRETID")?;
    let secret_key = required_env("COS_SECRETKEY")?;
    Ok(
        match env::var("COS_SESSION_TOKEN").ok().filter(|v| !v.is_empty()) {
            Some(token) => Credential::with_token(secret_id, secret_key, token),
            None => Credential::new(secret_id, secret_key),
        },
    )
}

fn required_env(name: &str) -> TestResult<String> {
    env::var(name).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{name} must be set to run COS integration tests"),
        )
        .into()
    })
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn unique_key(name: &str) -> String {
    format!("cos-rs-integration/{}/{}", unique_suffix(), name)
}

fn unique_prefix(name: &str) -> String {
    format!("cos-rs-integration/{}/{name}/", unique_suffix())
}

fn temp_file(name: &str) -> PathBuf {
    env::temp_dir().join(format!("cos-rs-integration-{}-{name}", unique_suffix()))
}

#[tokio::test]
#[ignore]
async fn object_put_get_head_delete_round_trip() -> TestResult {
    let client = client_from_env()?;
    let key = unique_key("round-trip.txt");
    let body = b"hello from cos_rs integration test".to_vec();

    let observed = async {
        let put = client
            .object()
            .put(
                &key,
                body.clone(),
                Some(ObjectPutOptions {
                    content_type: Some("text/plain".to_owned()),
                    ..Default::default()
                }),
            )
            .await?;
        let head = client.object().head(&key, None).await?;
        let got = client.object().get(&key, None).await?;
        Ok::<_, cos_rs::Error>((put.status_code(), head.status_code(), got.into_bytes()))
    }
    .await;

    let delete = client.object().delete(&key, None).await;
    let exists = client.object().is_exist(&key).await.unwrap_or(false);

    let (put_status, head_status, got_body) = observed?;
    delete?;

    assert!((200..300).contains(&put_status));
    assert!((200..300).contains(&head_status));
    assert_eq!(got_body.as_ref(), body.as_slice());
    assert!(!exists);

    Ok(())
}

#[tokio::test]
#[ignore]
async fn bucket_list_with_prefix_finds_uploaded_objects() -> TestResult {
    let client = client_from_env()?;
    let prefix = unique_prefix("list");
    let keys = vec![format!("{prefix}a.txt"), format!("{prefix}nested/b.txt")];

    let observed = async {
        for (index, key) in keys.iter().enumerate() {
            client
                .object()
                .put(key, format!("body-{index}"), None)
                .await?;
        }

        let mut listed = Vec::new();
        for _ in 0..5 {
            let (result, _) = client
                .bucket()
                .get(Some(BucketGetOptions {
                    prefix: Some(prefix.clone()),
                    max_keys: Some(100),
                    ..Default::default()
                }))
                .await?;
            listed = result
                .contents
                .into_iter()
                .map(|object| object.key)
                .collect::<Vec<_>>();
            if keys.iter().all(|key| listed.contains(key)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }

        Ok::<_, cos_rs::Error>(listed)
    }
    .await;

    for key in &keys {
        let _ = client.object().delete(key, None).await;
    }

    let listed = observed?;
    for key in &keys {
        assert!(listed.contains(key), "missing {key} in {listed:?}");
    }

    Ok(())
}

#[tokio::test]
#[ignore]
async fn object_file_helpers_upload_and_download() -> TestResult {
    let client = client_from_env()?;
    let key = unique_key("file-helper.txt");
    let source = temp_file("source.txt");
    let target = temp_file("target.txt");
    let body = b"file helper integration body";

    tokio::fs::write(&source, body).await?;

    let observed = async {
        client
            .object()
            .put_from_file(
                &key,
                &source,
                Some(ObjectPutOptions {
                    content_type: Some("text/plain".to_owned()),
                    ..Default::default()
                }),
            )
            .await?;
        client.object().get_to_file(&key, &target, None).await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(tokio::fs::read(&target).await?)
    }
    .await;

    let _ = client.object().delete(&key, None).await;
    let _ = tokio::fs::remove_file(&source).await;
    let _ = tokio::fs::remove_file(&target).await;

    assert_eq!(observed?, body);

    Ok(())
}
