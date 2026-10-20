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

#[cfg(any(feature = "async", feature = "blocking"))]
impl TlsRootStore {
    pub(crate) const fn into_reqx(self) -> reqx::advanced::TlsRootStore {
        match self {
            Self::BackendDefault => reqx::advanced::TlsRootStore::BackendDefault,
            Self::WebPki => reqx::advanced::TlsRootStore::WebPki,
            Self::System => reqx::advanced::TlsRootStore::System,
        }
    }
}
