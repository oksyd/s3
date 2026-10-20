#[cfg(all(feature = "async", feature = "multipart"))]
use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(all(feature = "async", feature = "multipart"))]
use s3::{Auth, Client, Error};

#[cfg(all(feature = "async", feature = "multipart"))]
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
    let key = format!("examples/multipart-{now}.bin");

    let created = client
        .objects()
        .create_multipart_upload(&bucket, &key)
        .send()
        .await?;
    let upload_id = created.upload_id.clone();

    let upload = async {
        let part1 = vec![b'a'; 5 * 1024 * 1024];
        let out1 = client
            .objects()
            .upload_part(&bucket, &key, &upload_id, 1)
            .body_bytes(part1)
            .send()
            .await?;
        let etag1 = out1
            .etag
            .ok_or_else(|| Error::decode("missing etag for part 1", None))?;

        let out2 = client
            .objects()
            .upload_part(&bucket, &key, &upload_id, 2)
            .body_bytes(b"done\n".to_vec())
            .send()
            .await?;
        let etag2 = out2
            .etag
            .ok_or_else(|| Error::decode("missing etag for part 2", None))?;

        client
            .objects()
            .complete_multipart_upload(&bucket, &key, &upload_id)
            .part(1, etag1)?
            .part(2, etag2)?
            .send()
            .await?;
        Ok::<(), s3::Error>(())
    }
    .await;

    if let Err(err) = upload {
        let _ = client
            .objects()
            .abort_multipart_upload(&bucket, &key, &upload_id)
            .send()
            .await;
        return Err(err);
    }

    let got = client.objects().get(&bucket, &key).send().await?;
    let bytes = got.bytes().await?;
    println!("uploaded {} bytes via multipart upload", bytes.len());

    client.objects().delete(&bucket, &key).send().await?;
    Ok(())
}

#[cfg(not(all(feature = "async", feature = "multipart")))]
fn main() {
    eprintln!("This example requires `async` and `multipart`.");
    eprintln!("Try: cargo run --example async_multipart_upload --features multipart");
}
