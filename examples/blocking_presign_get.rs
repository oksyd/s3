#[cfg(feature = "blocking")]
use std::time::Duration;

#[cfg(feature = "blocking")]
use s3::{Auth, BlockingClient, Credentials};

#[cfg(feature = "blocking")]
fn main() -> Result<(), s3::Error> {
    let creds = Credentials::new("TESTACCESSKEY", "TESTSECRETKEY")?;

    let client = BlockingClient::builder("https://s3.example.com")?
        .region("us-east-1")
        .auth(Auth::Static(creds))
        .build()?;

    let presigned = client
        .objects()
        .presign_get("my-bucket", "path/to/object.txt")
        .expires_in(Duration::from_secs(300))?
        .build()?;

    println!("{}", presigned.url);
    Ok(())
}

#[cfg(not(feature = "blocking"))]
fn main() {
    eprintln!("This example requires the `blocking` feature.");
    eprintln!(
        "Try: cargo run --example blocking_presign_get --no-default-features --features blocking,rustls"
    );
}
