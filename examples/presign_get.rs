#[cfg(feature = "async")]
use std::time::Duration;

#[cfg(feature = "async")]
use s3::{Auth, Client, Credentials};

#[cfg(feature = "async")]
fn main() -> Result<(), s3::Error> {
    let creds = Credentials::new("TESTACCESSKEY", "TESTSECRETKEY")?;

    let client = Client::builder("https://s3.amazonaws.com")?
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

#[cfg(not(feature = "async"))]
fn main() {
    eprintln!("This example requires the `async` feature (enabled by default).");
}
