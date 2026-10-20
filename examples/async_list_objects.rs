#[cfg(feature = "async")]
use std::env;

#[cfg(feature = "async")]
use s3::{AddressingStyle, Auth, Client};

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
            eprintln!("Set S3_TEST_BUCKET to the bucket you want to list.");
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
        .addressing_style(AddressingStyle::Auto)
        .build()?;

    let mut pager = client.objects().list_v2(&bucket).max_keys(1000)?.pager();

    let mut pages = 0u32;
    while let Some(page) = pager.next_page().await? {
        pages += 1;
        for obj in page.contents {
            println!("{} ({} bytes)", obj.key, obj.size);
        }
        for prefix in page.common_prefixes {
            println!("{prefix}/");
        }

        if pages >= 3 {
            break;
        }
    }

    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() {
    eprintln!("This example requires the `async` feature (enabled by default).");
}
