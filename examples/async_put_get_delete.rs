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
    let key = format!("examples/put-get-delete-{now}.txt");

    let body = b"hello from s3-rs\n".to_vec();
    client
        .objects()
        .put(&bucket, &key)
        .content_type("text/plain")?
        .body_bytes(body.clone())
        .send()
        .await?;

    let head = client.objects().head(&bucket, &key).send().await?;
    println!(
        "uploaded key={key} etag={:?} content_length={:?}",
        head.etag, head.content_length
    );

    let got = client.objects().get(&bucket, &key).send().await?;
    let bytes = got.bytes().await?;
    println!("downloaded {} bytes", bytes.len());

    client.objects().delete(&bucket, &key).send().await?;
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() {
    eprintln!("This example requires the `async` feature (enabled by default).");
}
