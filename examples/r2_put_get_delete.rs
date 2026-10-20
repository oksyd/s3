#[cfg(all(feature = "async", feature = "providers"))]
use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(all(feature = "async", feature = "providers"))]
use s3::{Auth, Credentials};

#[cfg(all(feature = "async", feature = "providers"))]
#[tokio::main]
async fn main() -> Result<(), s3::Error> {
    let account_id = match env::var("R2_ACCOUNT_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Set R2_ACCOUNT_ID to run this example.");
            return Ok(());
        }
    };

    let bucket = match env::var("R2_BUCKET") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Set R2_BUCKET to run this example.");
            return Ok(());
        }
    };

    let access_key_id = match env::var("R2_ACCESS_KEY_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Set R2_ACCESS_KEY_ID to run this example.");
            return Ok(());
        }
    };
    let secret_access_key = match env::var("R2_SECRET_ACCESS_KEY") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Set R2_SECRET_ACCESS_KEY to run this example.");
            return Ok(());
        }
    };

    let mut creds = Credentials::new(access_key_id, secret_access_key)?;
    if let Ok(token) = env::var("R2_SESSION_TOKEN") {
        creds = creds.with_session_token(token)?;
    }

    let jurisdiction = env::var("R2_JURISDICTION").ok();
    let endpoint = match jurisdiction
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(value) => match value.parse::<s3::providers::R2Endpoint>() {
            Ok(v) => v,
            Err(_) => {
                eprintln!("R2_JURISDICTION must be one of: global, eu, fedramp");
                return Ok(());
            }
        },
        None => s3::providers::R2Endpoint::Global,
    };
    let preset = s3::providers::cloudflare_r2(&account_id, endpoint)?;
    let client = preset
        .async_client_builder()?
        .auth(Auth::Static(creds))
        .build()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let key = format!("examples/r2-put-get-delete-{now}.txt");

    client
        .objects()
        .put(&bucket, &key)
        .content_type("text/plain")?
        .body_bytes(b"hello r2\n".to_vec())
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
    eprintln!("This example requires `async` and `providers`.");
    eprintln!("Try: cargo run --example r2_put_get_delete --features providers");
}
