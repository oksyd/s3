#[cfg(feature = "_async")]
use std::env;

#[cfg(feature = "_async")]
use s3::{Auth, Client, TlsConfig, TlsRootStore};

#[cfg(feature = "_async")]
fn tls_root_store_from_env() -> TlsRootStore {
    match env::var("S3_TLS_ROOT_STORE")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "webpki" => TlsRootStore::WebPki,
        "system" => TlsRootStore::System,
        _ => TlsRootStore::BackendDefault,
    }
}

#[cfg(feature = "_async")]
#[tokio::main]
async fn main() -> Result<(), s3::Error> {
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

    let tls_root_store = tls_root_store_from_env();
    eprintln!("Using request TLS root store: {tls_root_store:?}");
    eprintln!("Set S3_TLS_ROOT_STORE=system|webpki|backend-default (default backend-default).");

    let client = Client::builder(&endpoint)?
        .region(region)
        .auth(auth)
        .tls(TlsConfig {
            root_store: tls_root_store,
            ..TlsConfig::default()
        })
        .build()?;

    let buckets = client.buckets().list().send().await?;
    for bucket in buckets.buckets {
        println!("{}", bucket.name);
    }

    Ok(())
}

#[cfg(not(feature = "_async"))]
fn main() {
    eprintln!("This example requires an async client/TLS feature (enabled by default).");
}
