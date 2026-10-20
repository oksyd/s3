#[cfg(all(feature = "async", feature = "providers"))]
use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(all(feature = "async", feature = "providers"))]
use s3::Auth;

#[cfg(all(feature = "async", feature = "providers"))]
#[tokio::main]
async fn main() -> Result<(), s3::Error> {
    let bucket = match env::var("S3_TEST_BUCKET") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Set S3_TEST_BUCKET to a bucket you can write to.");
            return Ok(());
        }
    };

    let auth = match Auth::from_env() {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Set AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY: {err}");
            return Ok(());
        }
    };

    let preset = s3::providers::minio_local();
    let client = preset.async_client_builder()?.auth(auth).build()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let key = format!("examples/minio-local-{now}.txt");

    client
        .objects()
        .put(&bucket, &key)
        .content_type("text/plain")?
        .body_bytes(b"hello minio\n".to_vec())
        .send()
        .await?;

    let obj = client.objects().get(&bucket, &key).send().await?;
    let bytes = obj.bytes().await?;
    println!("downloaded {} bytes", bytes.len());

    client.objects().delete(&bucket, &key).send().await?;
    Ok(())
}

#[cfg(not(all(feature = "async", feature = "providers")))]
fn main() {
    eprintln!("This example requires `async` + `providers`.");
    eprintln!("Try: cargo run --example minio_local_put_get_delete --features providers");
}
