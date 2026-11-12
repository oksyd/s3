# s3-rs

[![crates.io](https://img.shields.io/crates/v/s3.svg)](https://crates.io/crates/s3)
[![docs.rs](https://docs.rs/s3/badge.svg)](https://docs.rs/s3)
[![CI](https://github.com/oksyd/s3/actions/workflows/ci.yaml/badge.svg)](https://github.com/oksyd/s3/actions/workflows/ci.yaml)

An unofficial S3-compatible client for Rust, with async and blocking APIs,
presigned URLs, and multipart uploads.

[API documentation](https://docs.rs/s3) · [Feature flags](https://docs.rs/s3/latest/s3/#features) · [Examples](examples/README.md)

## Installation

Choose a client mode and TLS backend:

```sh
# Async + rustls / ring (default)
cargo add s3

# Async + rustls / Graviola
cargo add s3 --no-default-features --features async-tls-rustls-graviola

# Async + native-tls
cargo add s3 --no-default-features --features async-tls-native

# Blocking + rustls / ring
cargo add s3 --no-default-features --features blocking-tls-rustls-ring

# Blocking + rustls / Graviola
cargo add s3 --no-default-features --features blocking-tls-rustls-graviola

# Blocking + native-tls
cargo add s3 --no-default-features --features blocking-tls-native

# Async and blocking + rustls / ring
cargo add s3 --no-default-features --features async-tls-rustls-ring,blocking-tls-rustls-ring
```

Each mode enables only its own HTTP client. See [feature flags](https://docs.rs/s3/latest/s3/#features)
for multipart uploads, checksums, credential providers, tracing, and metrics.

## Usage

This example uses the async client and Tokio:

```sh
cargo add tokio --features macros,rt-multi-thread
```

```rust
use s3::{Auth, Client};

#[tokio::main]
async fn main() -> s3::Result<()> {
    let client = Client::builder("https://s3.example.com")?
        .region("us-east-1")
        .auth(Auth::from_env()?)
        .build()?;

    let object = client
        .objects()
        .get("my-bucket", "path/to/object.txt")
        .send()
        .await?;
    let bytes = object.bytes().await?;
    println!("{} bytes", bytes.len());
    Ok(())
}
```

Set your endpoint, region, bucket, and object key. `Auth::from_env()` reads
`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and optionally `AWS_SESSION_TOKEN`.

See the [API documentation](https://docs.rs/s3) for blocking clients,
authentication, TLS configuration, and individual operations.

## License

[Apache-2.0](LICENSE)
