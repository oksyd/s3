/// TLS backend to use for HTTPS requests.
///
/// The corresponding `async-tls-*` or `blocking-tls-*` feature must be enabled
/// for the client's execution mode. Selecting an unavailable backend returns
/// [`crate::Error::InvalidConfig`] when the client is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TlsBackend {
    /// rustls with the ring crypto provider.
    RustlsRing,
    /// rustls with the Graviola crypto provider, configured per client.
    RustlsGraviola,
    /// The platform-native TLS stack.
    NativeTls,
}

/// TLS settings shared by S3 clients and credential providers.
///
/// Client builders and credential-loading methods receive their own settings.
/// Pass the same value to both to share a policy. Credential providers retain
/// these settings when refreshing credentials.
///
/// ```no_run
/// # #[cfg(feature = "_async")]
/// # fn demo() -> Result<(), s3::Error> {
/// use s3::{Client, TlsBackend, TlsConfig, TlsRootStore};
///
/// // Enable `async-tls-rustls-graviola` for this selection.
/// let tls = TlsConfig {
///     backend: Some(TlsBackend::RustlsGraviola),
///     root_store: TlsRootStore::System,
/// };
/// let client = Client::builder("https://s3.example.com")?
///     .region("us-east-1")
///     .tls(tls)
///     .build()?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlsConfig {
    /// Explicit backend, or `None` to select from this crate's enabled features.
    ///
    /// The default priority for each execution mode is ring, native TLS, then
    /// Graviola. An explicit choice always takes precedence over this default.
    pub backend: Option<TlsBackend>,
    /// Certificate trust roots. Defaults to [`TlsRootStore::BackendDefault`].
    pub root_store: TlsRootStore,
}

#[cfg(any(feature = "_async", feature = "_blocking"))]
impl TlsConfig {
    #[cfg(feature = "_async")]
    pub(crate) fn async_backend(self) -> crate::Result<reqx::advanced::TlsBackend> {
        let backend = self
            .backend
            .unwrap_or(if cfg!(feature = "async-tls-rustls-ring") {
                TlsBackend::RustlsRing
            } else if cfg!(feature = "async-tls-native") {
                TlsBackend::NativeTls
            } else {
                TlsBackend::RustlsGraviola
            });
        let (feature, enabled) = match backend {
            TlsBackend::RustlsRing => (
                "async-tls-rustls-ring",
                cfg!(feature = "async-tls-rustls-ring"),
            ),
            TlsBackend::RustlsGraviola => (
                "async-tls-rustls-graviola",
                cfg!(feature = "async-tls-rustls-graviola"),
            ),
            TlsBackend::NativeTls => ("async-tls-native", cfg!(feature = "async-tls-native")),
        };
        if !enabled {
            return Err(crate::Error::invalid_config(format!(
                "TLS backend requires s3 feature `{feature}`"
            )));
        }
        Ok(backend.into_reqx())
    }

    #[cfg(feature = "_blocking")]
    pub(crate) fn blocking_backend(self) -> crate::Result<reqx::advanced::TlsBackend> {
        let backend = self
            .backend
            .unwrap_or(if cfg!(feature = "blocking-tls-rustls-ring") {
                TlsBackend::RustlsRing
            } else if cfg!(feature = "blocking-tls-native") {
                TlsBackend::NativeTls
            } else {
                TlsBackend::RustlsGraviola
            });
        let (feature, enabled) = match backend {
            TlsBackend::RustlsRing => (
                "blocking-tls-rustls-ring",
                cfg!(feature = "blocking-tls-rustls-ring"),
            ),
            TlsBackend::RustlsGraviola => (
                "blocking-tls-rustls-graviola",
                cfg!(feature = "blocking-tls-rustls-graviola"),
            ),
            TlsBackend::NativeTls => ("blocking-tls-native", cfg!(feature = "blocking-tls-native")),
        };
        if !enabled {
            return Err(crate::Error::invalid_config(format!(
                "TLS backend requires s3 feature `{feature}`"
            )));
        }
        Ok(backend.into_reqx())
    }
}

#[cfg(any(feature = "_async", feature = "_blocking"))]
impl TlsBackend {
    const fn into_reqx(self) -> reqx::advanced::TlsBackend {
        match self {
            Self::RustlsRing => reqx::advanced::TlsBackend::RustlsRing,
            Self::RustlsGraviola => reqx::advanced::TlsBackend::RustlsGraviola,
            Self::NativeTls => reqx::advanced::TlsBackend::NativeTls,
        }
    }
}

/// Trust root policy for HTTPS requests, including credential providers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TlsRootStore {
    /// Use the TLS backend's default roots.
    ///
    /// `rustls` uses WebPKI roots; `native-tls` follows its backend defaults.
    #[default]
    BackendDefault,
    /// Force WebPKI roots.
    WebPki,
    /// Use platform/system trust verification.
    System,
}

#[cfg(any(feature = "_async", feature = "_blocking"))]
impl TlsRootStore {
    pub(crate) const fn into_reqx(self) -> reqx::advanced::TlsRootStore {
        match self {
            Self::BackendDefault => reqx::advanced::TlsRootStore::BackendDefault,
            Self::WebPki => reqx::advanced::TlsRootStore::WebPki,
            Self::System => reqx::advanced::TlsRootStore::System,
        }
    }
}
