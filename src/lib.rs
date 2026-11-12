//! An unofficial S3-compatible client for Rust, with async and blocking APIs,
//! presigned URLs, and multipart uploads.
//!
//! ## Start here
//!
//! If you are new to the crate, read these items in this order:
//!
//! - [`Client`] for the async entry point
//! - [`BlockingClient`] for the blocking entry point
//! - [`Auth`] for signing strategy and credential loading
//! - [`api`] for request builders exposed from `objects()` and `buckets()`
//! - [`types`] for public request and response models
//! - [`providers`] for endpoint presets such as AWS, MinIO, and Cloudflare R2
//!
//! ## Mental model
//!
//! Most applications follow the same flow:
//!
//! - [`Auth`] chooses how requests are signed.
//! - `Client::builder(...)` and `BlockingClient::builder(...)` capture endpoint, region, retry,
//!   TLS, and addressing policy.
//! - `client.objects()` and `client.buckets()` produce typed request builders for object and
//!   bucket operations.
//! - [`types`] contains the public request and response models you work with. Protocol XML mapping
//!   stays internal.
//! - [`providers`] presets bootstrap common S3-compatible endpoints.
//!
//! ## Quick start (async)
//!
//! ```no_run
//! # #[cfg(feature = "_async")]
//! # async fn demo() -> Result<(), s3::Error> {
//! use s3::{Auth, Client};
//!
//! let client = Client::builder("https://s3.example.com")?
//!     .region("us-east-1")
//!     .auth(Auth::from_env()?)
//!     .build()?;
//!
//! let obj = client
//!     .objects()
//!     .get("my-bucket", "path/to/object.txt")
//!     .send()
//!     .await?;
//! let bytes = obj.bytes().await?;
//! println!("{} bytes", bytes.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Quick start (blocking)
//!
//! ```no_run
//! # #[cfg(feature = "_blocking")]
//! # fn demo() -> Result<(), s3::Error> {
//! use s3::{Auth, BlockingClient};
//!
//! let client = BlockingClient::builder("https://s3.example.com")?
//!     .region("us-east-1")
//!     .auth(Auth::from_env()?)
//!     .build()?;
//!
//! let obj = client.objects().get("my-bucket", "path/to/object.txt").send()?;
//! let bytes = obj.bytes()?;
//! println!("{} bytes", bytes.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Common tasks
//!
//! - Download an object: [`api::GetObjectRequest`]
//! - Upload an object: [`api::PutObjectRequest`]
//! - List objects: [`api::ListObjectsV2Request`]
//! - Presign a request: [`api::PresignObjectRequest`] or [`types::PresignedRequest`]
//! - List buckets: [`api::ListBucketsRequest`]
//! - Manage bucket settings: [`api::PutBucketLifecycleRequest`], [`api::PutBucketCorsRequest`],
//!   [`api::PutBucketTaggingRequest`], [`api::PutBucketEncryptionRequest`]
//!
//! ## Features
//!
//! Select a client mode and TLS backend:
//!
//! | Feature | Client | TLS backend |
//! | --- | --- | --- |
//! | `async-tls-rustls-ring` (default) | Async | rustls with ring |
//! | `async-tls-rustls-graviola` | Async | rustls with Graviola |
//! | `async-tls-native` | Async | native-tls |
//! | `blocking-tls-rustls-ring` | Blocking | rustls with ring |
//! | `blocking-tls-rustls-graviola` | Blocking | rustls with Graviola |
//! | `blocking-tls-native` | Blocking | native-tls |
//!
//! Use `default-features = false` for blocking-only builds or to replace the default
//! TLS backend. For example:
//!
//! ```toml
//! [dependencies]
//! s3 = { version = "0.1", default-features = false, features = ["blocking-tls-rustls-ring"] }
//! ```
//!
//! Each feature enables only its corresponding HTTP client. Combine features to use
//! both modes, including different TLS backends per mode. [`TlsConfig`] controls
//! backend selection and trust roots. Set [`TlsConfig::backend`] explicitly when
//! Cargo unifies multiple backend features and a particular backend is required.
//! Graviola is configured per client; no process-wide crypto provider is needed.
//!
//! Additional capabilities are opt-in:
//!
//! | Feature | Capability |
//! | --- | --- |
//! | `multipart` | Multipart upload operations |
//! | `checksums` | Upload checksum calculation and headers |
//! | `credentials-profile` | Shared configuration and credentials file loading |
//! | `credentials-imds` | Container metadata and IMDSv2 credentials |
//! | `credentials-sts` | STS assume-role and web identity credentials |
//! | `tracing` | Request tracing |
//! | `metrics` | Request metrics |
//!
//! `credentials-imds` and `credentials-sts` require a client/TLS feature.
//! Endpoint presets in [`providers`] are always available.
//!
//! Docs.rs builds with all features enabled. Feature-gated items are labeled on
//! their documentation pages. `_async` and `_blocking` are internal mode switches
//! enabled by the client/TLS features; do not select them directly.
//!
//! ## Examples
//!
//! See the [runnable examples](https://github.com/oksyd/s3/tree/main/examples)
//! for object operations, credential providers, TLS configuration, and blocking clients.

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(all(
    feature = "_async",
    not(any(
        feature = "async-tls-rustls-ring",
        feature = "async-tls-rustls-graviola",
        feature = "async-tls-native"
    ))
))]
compile_error!(
    "Enable `async-tls-rustls-ring`, `async-tls-rustls-graviola`, or `async-tls-native` to use the async client."
);

#[cfg(all(
    feature = "_blocking",
    not(any(
        feature = "blocking-tls-rustls-ring",
        feature = "blocking-tls-rustls-graviola",
        feature = "blocking-tls-native"
    ))
))]
compile_error!(
    "Enable `blocking-tls-rustls-ring`, `blocking-tls-rustls-graviola`, or `blocking-tls-native` to use the blocking client."
);

#[cfg(all(
    any(feature = "credentials-imds", feature = "credentials-sts"),
    not(any(feature = "_async", feature = "_blocking"))
))]
compile_error!("Enable a client/TLS feature when using `credentials-imds` or `credentials-sts`.");

#[cfg(any(feature = "_async", feature = "_blocking"))]
/// Service entry points and request builders.
pub mod api;
/// Endpoint presets for common S3-compatible services.
pub mod providers;
/// Shared request/response types.
pub mod types;

mod auth;
#[cfg(any(test, feature = "_async", feature = "_blocking"))]
mod client;
mod credentials;
mod error;
mod tls;
#[cfg(any(test, feature = "_async", feature = "_blocking"))]
mod transport;
mod util;

#[cfg(any(feature = "_async", feature = "_blocking"))]
pub use auth::CachedProvider;
#[cfg(feature = "_async")]
pub use auth::CredentialsFuture;
pub use auth::{
    AddressingStyle, Auth, Credentials, CredentialsProvider, CredentialsSnapshot,
    DynCredentialsProvider, Region,
};
#[cfg(feature = "_blocking")]
pub use client::{BlockingClient, BlockingClientBuilder};
#[cfg(feature = "_async")]
pub use client::{Client, ClientBuilder};
pub use error::{ApiError, Error, Result};
pub use tls::{TlsBackend, TlsConfig, TlsRootStore};
