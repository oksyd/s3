#[cfg(feature = "blocking")]
use std::env;

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

    let buckets = client.buckets().list().send()?;
    for bucket in buckets.buckets {
        println!("{}", bucket.name);
    }

    Ok(())
}

#[cfg(not(feature = "blocking"))]
fn main() {
    eprintln!(
        "This example requires the `blocking` feature. Try:\n  cargo run --example blocking_list_buckets --no-default-features --features blocking,rustls"
    );
}
