//! Lean, modern, unofficial S3-compatible client for Rust.
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
//! - Optional `providers` presets bootstrap common S3-compatible endpoints.
//!
//! ## Quick start (async)
//!
//! ```no_run
//! # #[cfg(feature = "async")]
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
//! # #[cfg(feature = "blocking")]
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
//! ## Feature visibility
//!
//! Docs.rs builds this crate with all features enabled. Feature-gated items are labeled on their
//! docs pages so you can tell whether an API requires `async`, `blocking`, `providers`,
//! `multipart`, or one of the optional credential features.
//!
//! ## Examples and docs
//!
//! See the README and `examples/README.md` for a usage guide organized around auth, client
//! builders, object and bucket services, TLS policy, and blocking variants.

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(all(
    feature = "rustls",
    feature = "native-tls",
    not(feature = "allow-both-tls")
))]
compile_error!("Enable only one of: rustls, native-tls.");

#[cfg(all(
    any(feature = "async", feature = "blocking"),
    not(any(feature = "rustls", feature = "native-tls"))
))]
compile_error!("Enable one of: rustls, native-tls.");

#[cfg(all(
    any(feature = "credentials-imds", feature = "credentials-sts"),
    not(any(feature = "async", feature = "blocking"))
))]
compile_error!("Enable `async` or `blocking` when using `credentials-imds` or `credentials-sts`.");

#[cfg(any(feature = "async", feature = "blocking"))]
/// Service entry points and request builders.
pub mod api;
#[cfg(feature = "providers")]
/// Endpoint presets for common S3-compatible services.
pub mod providers;
/// Shared request/response types.
pub mod types;

mod auth;
#[cfg(any(test, feature = "async", feature = "blocking"))]
mod client;
mod credentials;
mod error;
mod tls;
#[cfg(any(test, feature = "async", feature = "blocking"))]
mod transport;
mod util;

#[cfg(any(feature = "async", feature = "blocking"))]
pub use auth::CachedProvider;
#[cfg(feature = "async")]
pub use auth::CredentialsFuture;
pub use auth::{
    AddressingStyle, Auth, Credentials, CredentialsProvider, CredentialsSnapshot,
    DynCredentialsProvider, Region,
};
#[cfg(feature = "blocking")]
pub use client::{BlockingClient, BlockingClientBuilder};
#[cfg(feature = "async")]
pub use client::{Client, ClientBuilder};
pub use error::{ApiError, Error, Result};
pub use tls::TlsRootStore;
