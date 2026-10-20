use std::{
    env,
    net::IpAddr,
    net::ToSocketAddrs as _,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use s3::{AddressingStyle, Auth, Error, Region};

static BUCKET_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct TestConfig {
    pub(crate) endpoint: String,
    pub(crate) region: Region,
    pub(crate) auth: Auth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TestBackend {
    Minio,
    Rustfs,
    Other,
}

pub(crate) fn load_config() -> Result<Option<TestConfig>, Error> {
    let Ok(endpoint) = env::var("S3_TEST_ENDPOINT") else {
        return Ok(None);
    };

    let region = env::var("S3_TEST_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let region = Region::new(region)?;

    let Ok(auth) = Auth::from_env() else {
        return Ok(None);
    };

    Ok(Some(TestConfig {
        endpoint,
        region,
        auth,
    }))
}

pub(crate) fn backend() -> TestBackend {
    match env::var("S3_TEST_BACKEND")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "minio" => TestBackend::Minio,
        "rustfs" => TestBackend::Rustfs,
        _ => TestBackend::Other,
    }
}

pub(crate) fn strict() -> bool {
    if let Ok(value) = env::var("S3_TEST_STRICT") {
        return matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }

    backend() != TestBackend::Rustfs
}

pub(crate) fn unique_bucket(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let n = BUCKET_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{now}-{n}")
}

pub(crate) fn virtual_hosted_endpoint(original: &str) -> Result<Option<String>, Error> {
    let mut url = url::Url::parse(original)
        .map_err(|_| Error::invalid_config("S3_TEST_ENDPOINT must be a valid URL"))?;

    let Some(host) = url.host_str() else {
        return Err(Error::invalid_config("S3_TEST_ENDPOINT must include host"));
    };

    let is_loopback =
        host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if !is_loopback {
        return Ok(None);
    }

    let port = url.port_or_known_default().unwrap_or(80);
    if format!("test.localhost:{port}").to_socket_addrs().is_err() {
        return Ok(None);
    }

    url.set_host(Some("localhost"))
        .map_err(|_| Error::invalid_config("failed to build virtual-hosted endpoint"))?;

    Ok(Some(url.to_string()))
}

#[cfg(feature = "async")]
#[allow(dead_code)]
pub(crate) fn build_async_client(
    cfg: &TestConfig,
    style: AddressingStyle,
) -> Result<s3::Client, Error> {
    s3::Client::builder(&cfg.endpoint)?
        .region(cfg.region.as_str())
        .auth(cfg.auth.clone())
        .addressing_style(style)
        .build()
}

#[cfg(feature = "blocking")]
#[allow(dead_code)]
pub(crate) fn build_blocking_client(
    cfg: &TestConfig,
    style: AddressingStyle,
) -> Result<s3::BlockingClient, Error> {
    s3::BlockingClient::builder(&cfg.endpoint)?
        .region(cfg.region.as_str())
        .auth(cfg.auth.clone())
        .addressing_style(style)
        .build()
}

#[cfg(feature = "async")]
#[allow(dead_code)]
pub(crate) async fn with_bucket_async<F, Fut>(
    client: &s3::Client,
    prefix: &str,
    f: F,
) -> Result<(), Error>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), Error>>,
{
    use futures_util::FutureExt as _;

    let bucket = unique_bucket(prefix);
    client.buckets().create(&bucket).send().await?;

    let result = std::panic::AssertUnwindSafe(f(bucket.clone()))
        .catch_unwind()
        .await;

    let cleanup = cleanup_bucket_async(client, &bucket).await;

    match result {
        Ok(inner) => match inner {
            Ok(()) => cleanup,
            Err(err) => {
                let _ = cleanup;
                Err(err)
            }
        },
        Err(panic) => {
            let _ = cleanup;
            std::panic::resume_unwind(panic);
        }
    }
}

#[cfg(feature = "async")]
#[allow(dead_code)]
async fn cleanup_bucket_async(client: &s3::Client, bucket: &str) -> Result<(), Error> {
    use http::StatusCode;

    match client.buckets().delete(bucket).send().await {
        Ok(_) => Ok(()),
        Err(err) if err.status() == Some(StatusCode::NOT_FOUND) => Ok(()),
        Err(err) if is_bucket_not_empty(&err) => {
            delete_all_objects_async(client, bucket).await?;
            client.buckets().delete(bucket).send().await?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

#[cfg(feature = "async")]
#[allow(dead_code)]
async fn delete_all_objects_async(client: &s3::Client, bucket: &str) -> Result<(), Error> {
    use http::StatusCode;

    let mut pager = client.objects().list_v2(bucket).max_keys(1000)?.pager();
    while let Some(page) = pager.next_page().await? {
        if page.contents.is_empty() {
            break;
        }

        let keys: Vec<String> = page.contents.into_iter().map(|o| o.key).collect();
        if let Err(err) = client
            .objects()
            .delete_objects(bucket)
            .objects(keys)?
            .send()
            .await
        {
            if err.status() == Some(StatusCode::NOT_FOUND) {
                break;
            }
            return Err(err);
        }
    }
    Ok(())
}

#[cfg(feature = "blocking")]
#[allow(dead_code)]
pub(crate) fn with_bucket_blocking<F>(
    client: &s3::BlockingClient,
    prefix: &str,
    f: F,
) -> Result<(), Error>
where
    F: FnOnce(String) -> Result<(), Error>,
{
    let bucket = unique_bucket(prefix);
    client.buckets().create(&bucket).send()?;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(bucket.clone())));
    let cleanup = cleanup_bucket_blocking(client, &bucket);

    match result {
        Ok(inner) => match inner {
            Ok(()) => cleanup,
            Err(err) => {
                let _ = cleanup;
                Err(err)
            }
        },
        Err(panic) => {
            let _ = cleanup;
            std::panic::resume_unwind(panic);
        }
    }
}

#[cfg(feature = "blocking")]
#[allow(dead_code)]
fn cleanup_bucket_blocking(client: &s3::BlockingClient, bucket: &str) -> Result<(), Error> {
    use http::StatusCode;

    match client.buckets().delete(bucket).send() {
        Ok(_) => Ok(()),
        Err(err) if err.status() == Some(StatusCode::NOT_FOUND) => Ok(()),
        Err(err) if is_bucket_not_empty(&err) => {
            delete_all_objects_blocking(client, bucket)?;
            client.buckets().delete(bucket).send()?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

#[cfg(feature = "blocking")]
#[allow(dead_code)]
fn delete_all_objects_blocking(client: &s3::BlockingClient, bucket: &str) -> Result<(), Error> {
    use http::StatusCode;

    let pager = client.objects().list_v2(bucket).max_keys(1000)?.pager();
    for page in pager {
        let page = page?;
        if page.contents.is_empty() {
            break;
        }

        let keys: Vec<String> = page.contents.into_iter().map(|o| o.key).collect();
        match client
            .objects()
            .delete_objects(bucket)
            .objects(keys)?
            .send()
        {
            Ok(_) => {}
            Err(err) if err.status() == Some(StatusCode::NOT_FOUND) => break,
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn is_bucket_not_empty(err: &Error) -> bool {
    err.status() == Some(http::StatusCode::CONFLICT)
        && matches!(
            err.code(),
            None | Some("BucketNotEmpty") | Some("BucketNotEmptyError")
        )
}

pub(crate) fn is_unsupported(err: &Error) -> bool {
    let Some(status) = err.status() else {
        return false;
    };

    // S3-compatible servers may return 501 for APIs they don't implement.
    status.as_u16() == 501 || matches!(err.code(), Some("NotImplemented" | "UnsupportedOperation"))
}
