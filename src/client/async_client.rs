use std::{borrow::Cow, sync::Arc, time::Duration};

use crate::TlsRootStore;
use http::{HeaderMap, Method};
use time::OffsetDateTime;
use url::Url;

use super::common::{parse_endpoint, sign_with_snapshot, validate_presign_credentials_lifetime};

use crate::{
    api,
    auth::{AddressingStyle, Auth, Region},
    error::{Error, Result},
    transport::async_transport::{AsyncBody, AsyncTransport},
    util,
};

/// Async S3 client.
///
/// This is the main async entry point for the crate. Build it with [`Client::builder`], then use
/// [`Client::objects`] or [`Client::buckets`] to create typed request builders from [`crate::api`].
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

/// Builder for [`Client`].
///
/// `region(...)` is required before calling [`ClientBuilder::build`]. Authentication defaults to
/// [`Auth::Anonymous`] until you set [`ClientBuilder::auth`].
pub struct ClientBuilder {
    endpoint: Url,
    region: Option<String>,
    auth: Auth,
    addressing: AddressingStyle,
    retry: crate::transport::RetryConfig,
    timeout: Option<Duration>,
    user_agent: Option<String>,
    tls_root_store: TlsRootStore,
}

struct Inner {
    endpoint: Url,
    region: Region,
    auth: Auth,
    addressing: AddressingStyle,
    transport: AsyncTransport,
}

impl Client {
    /// Creates a client builder from an endpoint URL.
    pub fn builder(endpoint: impl AsRef<str>) -> Result<ClientBuilder> {
        ClientBuilder::new(endpoint.as_ref())
    }

    /// Returns the objects service.
    ///
    /// Use this to create request builders such as [`crate::api::GetObjectRequest`] and
    /// [`crate::api::PutObjectRequest`].
    pub fn objects(&self) -> api::ObjectsService {
        api::ObjectsService::new(self.clone())
    }

    /// Returns the buckets service.
    ///
    /// Use this to create request builders such as [`crate::api::ListBucketsRequest`] and
    /// [`crate::api::PutBucketLifecycleRequest`].
    pub fn buckets(&self) -> api::BucketsService {
        api::BucketsService::new(self.clone())
    }

    pub(crate) fn region(&self) -> &str {
        self.inner.region.as_str()
    }

    pub(crate) async fn execute(
        &self,
        method: Method,
        bucket: Option<&str>,
        key: Option<&str>,
        query_params: Vec<(String, String)>,
        mut headers: HeaderMap,
        body: AsyncBody,
    ) -> Result<reqx::Response> {
        #[cfg(feature = "tracing")]
        let span = tracing::info_span!(
            "s3.request",
            method = %method,
            bucket_present = bucket.is_some(),
            key_present = key.is_some(),
            host = crate::transport::redacted_host_for_trace(&self.inner.endpoint),
        );

        let request = async {
            let (url, body) = self
                .prepare_request(&method, bucket, key, &query_params, &mut headers, body)
                .await?;
            self.inner.transport.send(method, url, headers, body).await
        };
        #[cfg(feature = "tracing")]
        let request = tracing::Instrument::instrument(request, span);
        request.await
    }

    pub(crate) async fn execute_stream(
        &self,
        method: Method,
        bucket: Option<&str>,
        key: Option<&str>,
        query_params: Vec<(String, String)>,
        mut headers: HeaderMap,
        body: AsyncBody,
    ) -> Result<reqx::ResponseStream> {
        #[cfg(feature = "tracing")]
        let span = tracing::info_span!(
            "s3.request",
            method = %method,
            bucket_present = bucket.is_some(),
            key_present = key.is_some(),
            host = crate::transport::redacted_host_for_trace(&self.inner.endpoint),
        );

        let request = async {
            let (url, body) = self
                .prepare_request(&method, bucket, key, &query_params, &mut headers, body)
                .await?;
            self.inner
                .transport
                .send_stream(method, url, headers, body)
                .await
        };
        #[cfg(feature = "tracing")]
        let request = tracing::Instrument::instrument(request, span);
        request.await
    }

    async fn prepare_request(
        &self,
        method: &Method,
        bucket: Option<&str>,
        key: Option<&str>,
        query_params: &[(String, String)],
        headers: &mut HeaderMap,
        body: AsyncBody,
    ) -> Result<(Url, AsyncBody)> {
        let resolved = util::url::resolve_url(
            &self.inner.endpoint,
            bucket,
            key,
            query_params,
            self.inner.addressing,
        )?;

        if let Some(snapshot) = self.inner.auth.credentials_snapshot_async().await? {
            let now = OffsetDateTime::now_utc();
            let payload_hash = match &body {
                AsyncBody::Empty => Cow::Borrowed(util::signing::EMPTY_PAYLOAD_HASH),
                AsyncBody::Bytes(b) => Cow::Owned(util::signing::sha256_hex(b)),
                AsyncBody::Stream { .. } => Cow::Borrowed(util::signing::UNSIGNED_PAYLOAD),
            };

            sign_with_snapshot(
                method,
                &resolved,
                headers,
                &payload_hash,
                &self.inner.region,
                &snapshot,
                now,
            )?;
        }

        Ok((resolved.url, body))
    }

    pub(crate) fn presign(
        &self,
        method: Method,
        bucket: &str,
        key: &str,
        expires_in: Duration,
        query_params: Vec<(String, String)>,
        headers: HeaderMap,
    ) -> Result<crate::types::PresignedRequest> {
        let creds = self.inner.auth.static_credentials().ok_or_else(|| {
            Error::invalid_config(
                "presign requires static credentials; use PresignObjectRequest::build_async for credential providers",
            )
        })?;

        let resolved = util::url::resolve_url(
            &self.inner.endpoint,
            Some(bucket),
            Some(key),
            &[],
            self.inner.addressing,
        )?;

        let now = OffsetDateTime::now_utc();
        util::signing::presign(
            method,
            resolved,
            util::signing::SigV4Params::for_s3(&self.inner.region, creds, now),
            expires_in,
            query_params,
            headers,
        )
    }

    pub(crate) async fn presign_async(
        &self,
        method: Method,
        bucket: &str,
        key: &str,
        expires_in: Duration,
        query_params: Vec<(String, String)>,
        headers: HeaderMap,
    ) -> Result<crate::types::PresignedRequest> {
        let snapshot = self
            .inner
            .auth
            .credentials_snapshot_async()
            .await?
            .ok_or_else(|| Error::invalid_config("presign requires credentials"))?;

        let resolved = util::url::resolve_url(
            &self.inner.endpoint,
            Some(bucket),
            Some(key),
            &[],
            self.inner.addressing,
        )?;

        let now = OffsetDateTime::now_utc();
        validate_presign_credentials_lifetime(&snapshot, expires_in, now)?;

        util::signing::presign(
            method,
            resolved,
            util::signing::SigV4Params::for_s3(&self.inner.region, snapshot.credentials(), now),
            expires_in,
            query_params,
            headers,
        )
    }
}

impl ClientBuilder {
    fn new(endpoint: &str) -> Result<Self> {
        let endpoint = parse_endpoint(endpoint)?;

        Ok(Self {
            endpoint,
            region: None,
            auth: Auth::Anonymous,
            addressing: AddressingStyle::Auto,
            retry: crate::transport::RetryConfig::default(),
            timeout: None,
            user_agent: None,
            tls_root_store: TlsRootStore::BackendDefault,
        })
    }

    /// Sets the region used for signing.
    pub fn region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Sets the authentication strategy.
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = auth;
        self
    }

    /// Sets the bucket addressing style.
    pub fn addressing_style(mut self, style: AddressingStyle) -> Self {
        self.addressing = style;
        self
    }

    /// Sets a per-request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Sets the maximum number of attempts, including the initial request.
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.retry.max_attempts = max_attempts;
        self
    }

    /// Sets the base delay for retries.
    pub fn base_retry_delay(mut self, delay: Duration) -> Self {
        self.retry.base_delay = delay;
        self
    }

    /// Sets the maximum delay for retries.
    pub fn max_retry_delay(mut self, delay: Duration) -> Self {
        self.retry.max_delay = delay;
        self
    }

    /// Sets the maximum delay honored from `Retry-After` hints.
    pub fn max_retry_after(mut self, delay: Duration) -> Self {
        self.retry.max_retry_after = delay;
        self
    }

    /// Overrides the default user agent.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Sets the trust root policy for async HTTPS requests.
    ///
    /// Builder default is `BackendDefault`.
    /// Use `System` for enterprise/private PKI environments that rely on OS trust stores.
    pub fn tls_root_store(mut self, tls_root_store: TlsRootStore) -> Self {
        self.tls_root_store = tls_root_store;
        self
    }

    /// Builds the client after validating configuration.
    pub fn build(self) -> Result<Client> {
        let region = self
            .region
            .ok_or_else(|| Error::invalid_config("region is required"))
            .and_then(Region::new)?;

        let transport = AsyncTransport::new(
            self.retry,
            self.user_agent,
            self.timeout,
            self.tls_root_store.into_reqx(),
        )?;

        let inner = Inner {
            endpoint: self.endpoint,
            region,
            auth: self.auth,
            addressing: self.addressing,
            transport,
        };

        Ok(Client {
            inner: Arc::new(inner),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FixedProvider;

    impl crate::auth::CredentialsProvider for FixedProvider {
        #[cfg(feature = "async")]
        fn credentials_async(&self) -> crate::auth::CredentialsFuture<'_> {
            Box::pin(async {
                let creds = crate::auth::Credentials::new("AKIA_TEST", "SECRET_TEST")?;
                Ok(crate::auth::CredentialsSnapshot::new(creds))
            })
        }

        #[cfg(feature = "blocking")]
        fn credentials_blocking(&self) -> crate::Result<crate::auth::CredentialsSnapshot> {
            let creds = crate::auth::Credentials::new("AKIA_TEST", "SECRET_TEST")?;
            Ok(crate::auth::CredentialsSnapshot::new(creds))
        }
    }

    #[test]
    fn builder_defaults_tls_root_store_to_backend_default() {
        let builder = ClientBuilder::new("https://s3.example.com").expect("builder should parse");
        assert_eq!(builder.tls_root_store, TlsRootStore::BackendDefault);
    }

    #[test]
    fn builder_tls_root_store_override_is_applied() {
        let builder = ClientBuilder::new("https://s3.example.com")
            .expect("builder should parse")
            .tls_root_store(TlsRootStore::System);
        assert_eq!(builder.tls_root_store, TlsRootStore::System);
    }

    #[test]
    fn builder_rejects_endpoint_with_user_info() {
        let err = match ClientBuilder::new("https://user:pass@s3.example.com") {
            Ok(_) => panic!("endpoint with user info must be rejected"),
            Err(err) => err,
        };
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("must not include user info"));
            }
            other => panic!("expected invalid config, got {other:?}"),
        }
    }

    #[test]
    fn build_rejects_zero_max_attempts() {
        let err = match Client::builder("https://s3.example.com")
            .expect("builder should parse")
            .region("us-east-1")
            .max_attempts(0)
            .build()
        {
            Ok(_) => panic!("zero retry attempts must be rejected"),
            Err(err) => err,
        };

        match err {
            Error::InvalidConfig { message } => assert!(message.contains("max_attempts")),
            other => panic!("expected invalid config, got {other:?}"),
        }
    }

    #[cfg(feature = "rustls")]
    #[test]
    fn build_accepts_webpki_on_rustls() {
        let client = Client::builder("https://s3.example.com")
            .expect("builder should parse")
            .region("us-east-1")
            .auth(Auth::Anonymous)
            .tls_root_store(TlsRootStore::WebPki)
            .build();
        assert!(client.is_ok(), "rustls should accept WebPki root store");
    }

    #[test]
    fn presign_with_provider_returns_actionable_error() {
        let client = Client::builder("https://s3.example.com")
            .expect("builder should parse")
            .region("us-east-1")
            .auth(Auth::provider(std::sync::Arc::new(FixedProvider)))
            .build()
            .expect("client should build");

        let err = client
            .presign(
                Method::GET,
                "bucket",
                "key",
                Duration::from_secs(60),
                Vec::new(),
                HeaderMap::new(),
            )
            .expect_err("presign should require static credentials");

        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("PresignObjectRequest::build_async"));
            }
            other => panic!("expected invalid config error, got {other:?}"),
        }
    }

    #[cfg(all(feature = "native-tls", not(feature = "rustls")))]
    #[test]
    fn build_rejects_webpki_on_native_tls() {
        let err = match Client::builder("https://s3.example.com")
            .expect("builder should parse")
            .region("us-east-1")
            .auth(Auth::Anonymous)
            .tls_root_store(TlsRootStore::WebPki)
            .build()
        {
            Ok(_) => panic!("native-tls should reject WebPki root store"),
            Err(err) => err,
        };

        match err {
            Error::Transport {
                source: Some(source),
                ..
            } => {
                assert!(
                    source.to_string().contains("TlsRootStore::WebPki"),
                    "unexpected source error: {source}"
                );
            }
            other => panic!("expected transport error, got {other:?}"),
        }
    }
}
