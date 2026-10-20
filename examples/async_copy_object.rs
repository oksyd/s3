#[cfg(feature = "async")]
use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(feature = "async")]
use s3::{Auth, Client};

#[cfg(feature = "async")]
#[tokio::main]
async fn main() -> Result<(), s3::Error> {
    let endpoint = match env::var("S3_TEST_ENDPOINT") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Set S3_TEST_ENDPOINT to run this example (e.g. http://127.0.0.1:9000).");
            return Ok(());
        }
    };

    let bucket = match env::var("S3_TEST_BUCKET") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Set S3_TEST_BUCKET to a bucket you can write to.");
            return Ok(());
        }
    };

    let region = env::var("S3_TEST_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let auth = match Auth::from_env() {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Set AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY: {err}");
            return Ok(());
        }
    };

    let client = Client::builder(&endpoint)?
        .region(region)
        .auth(auth)
        .build()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let src = format!("examples/copy-src-{now}.txt");
    let dst = format!("examples/copy-dst-{now}.txt");

    client
        .objects()
        .put(&bucket, &src)
        .content_type("text/plain")?
        .body_bytes(b"copy me\n".to_vec())
        .send()
        .await?;

    client
        .objects()
        .copy(&bucket, &src, &bucket, &dst)
        .replace_metadata()
        .metadata("copied", "true")?
        .send()
        .await?;

    let obj = client.objects().get(&bucket, &dst).send().await?;
    let bytes = obj.bytes().await?;
    println!("copied {} bytes", bytes.len());

    client.objects().delete(&bucket, &src).send().await?;
    client.objects().delete(&bucket, &dst).send().await?;
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() {
    eprintln!("This example requires the `async` feature (enabled by default).");
}
