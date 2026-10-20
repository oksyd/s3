#[cfg(feature = "blocking")]
use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(feature = "blocking")]
use s3::{Auth, BlockingClient};

#[cfg(feature = "blocking")]
fn main() -> Result<(), s3::Error> {
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

    let client = BlockingClient::builder(&endpoint)?
        .region(region)
        .auth(auth)
        .build()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let key = format!("examples/blocking-put-get-delete-{now}.txt");

    let body = b"hello from blocking s3-rs\n".to_vec();
    client
        .objects()
        .put(&bucket, &key)
        .content_type("text/plain")?
        .body_bytes(body)
        .send()?;

    let got = client.objects().get(&bucket, &key).send()?;
    let bytes = got.bytes()?;
    println!("downloaded {} bytes", bytes.len());

    client.objects().delete(&bucket, &key).send()?;
    Ok(())
}

#[cfg(not(feature = "blocking"))]
fn main() {
    eprintln!("This example requires the `blocking` feature.");
    eprintln!(
        "Try: cargo run --example blocking_put_get_delete --no-default-features --features blocking,rustls"
    );
}
