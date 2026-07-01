use cos_object_store::TencentCos;
use object_store::{integration::*, path::Path};
use std::env;

#[tokio::test]
#[ignore]
async fn test_tencent_cos() {
    dotenv::dotenv().ok();

    // let store = TencentCosBuilder::from_env().build().unwrap();
    let store = TencentCos::builder()
        .with_bucket_name(env::var("COS_BUCKET").unwrap())
        .with_region(env::var("COS_REGION").unwrap())
        .with_secret_id_and_key(
            env::var("COS_SECRETID").unwrap(),
            env::var("COS_SECRETKEY").unwrap(),
        )
        .build()
        .unwrap();
    print!("Tencent COS: \n{:?}", store);

    let location = Path::from("nonexistentname");
    let err = get_nonexistent_object(&store, Some(location))
        .await
        .unwrap_err();
    println!("\nError: {:?}", err);

    put_get_delete_list(&store).await;
    put_get_attributes(&store).await;
    get_opts(&store).await;
    put_opts(&store, false).await;

    list_uses_directories_correctly(&store).await;
    list_with_delimiter(&store).await;
    rename_and_copy(&store).await;
    copy_if_not_exists(&store).await;
    copy_rename_nonexistent_object(&store).await;
    multipart_race_condition(&store, true).await;
    multipart_out_of_order(&store).await;
}
