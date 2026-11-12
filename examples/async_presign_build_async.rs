#[cfg(feature = "_async")]
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

#[cfg(feature = "_async")]
use s3::{Auth, Client, Credentials, CredentialsProvider, CredentialsSnapshot};

#[cfg(feature = "_async")]
type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = s3::Result<CredentialsSnapshot>> + Send + 'a>>;

#[cfg(feature = "_async")]
#[derive(Clone, Debug)]
struct StaticProvider {
    credentials: Credentials,
}

#[cfg(feature = "_async")]
impl CredentialsProvider for StaticProvider {
    fn credentials_async(&self) -> ProviderFuture<'_> {
        let snapshot = CredentialsSnapshot::new(self.credentials.clone());
        Box::pin(async move { Ok(snapshot) })
    }

    #[cfg(feature = "_blocking")]
    fn credentials_blocking(&self) -> s3::Result<CredentialsSnapshot> {
        Ok(CredentialsSnapshot::new(self.credentials.clone()))
    }
}

#[cfg(feature = "_async")]
#[tokio::main]
async fn main() -> Result<(), s3::Error> {
    let provider = StaticProvider {
        credentials: Credentials::new("TESTACCESSKEY", "TESTSECRETKEY")?,
    };

    let client = Client::builder("https://s3.example.com")?
        .region("us-east-1")
        .auth(Auth::provider(Arc::new(provider)))
        .build()?;

    let presigned = client
        .objects()
        .presign_get("my-bucket", "path/to/object.txt")
        .expires_in(Duration::from_secs(300))?
        .build_async()
        .await?;

    println!("{}", presigned.url);
    Ok(())
}

#[cfg(not(feature = "_async"))]
fn main() {
    eprintln!("This example requires an async client/TLS feature (enabled by default).");
}
