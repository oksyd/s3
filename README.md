# s3-rs

[![crates.io](https://img.shields.io/crates/v/s3.svg)](https://crates.io/crates/s3)
[![docs.rs](https://docs.rs/s3/badge.svg)](https://docs.rs/s3)
[![CI](https://github.com/oksyd/s3-rs/actions/workflows/ci.yaml/badge.svg)](https://github.com/oksyd/s3-rs/actions/workflows/ci.yaml)

Lean, modern, unofficial S3-compatible client for Rust.

![s3-rs logo](examples/assets/s3-rs-logo.svg)

## Highlights

- Async + blocking clients with closely aligned APIs
- Presigned URLs and multipart upload
- Optional checksums, tracing, and metrics
- Integration-tested against MinIO and RustFS
- Small dependency surface (feature-gated)
- Structured errors (status/code/request id/body snippet)

## Install

```bash
# async + rustls (default)
cargo add s3

# blocking (disable defaults, pick one TLS backend)
cargo add s3 --no-default-features --features blocking,rustls

# async + native-tls
cargo add s3 --no-default-features --features async,native-tls
```

## Mental model

Most applications only touch a small set of layers:

- `Auth` decides how requests are signed: anonymous, env credentials, a custom provider, or optional IMDS/STS/profile flows
- `Client::builder(...)` / `BlockingClient::builder(...)` capture endpoint, region, retry, TLS, and addressing policy once
- `client.objects()` and `client.buckets()` return typed request builders for object and bucket operations
- `s3::types` contains the public request/response models you work with; protocol XML DTOs stay internal
- `s3::providers` offers endpoint presets for common S3-compatible services when enabled

## Usage

Most code follows the same flow: choose `Auth`, build a client, then use `objects()` or
`buckets()` to send requests and consume public output types from `s3::types`.

### Async

```rust,no_run
use s3::{Auth, Client};

# async fn demo() -> Result<(), s3::Error> {
let client = Client::builder("https://s3.example.com")?
    .region("us-east-1")
    .auth(Auth::from_env()?)
    .build()?;

let obj = client.objects().get("my-bucket", "path/to/object.txt").send().await?;
let bytes = obj.bytes().await?;
println!("{} bytes", bytes.len());
# Ok(())
# }
```

`Auth::from_env()` reads `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and optionally `AWS_SESSION_TOKEN`.

### Blocking

```rust,no_run
use s3::{Auth, BlockingClient};

fn demo() -> Result<(), s3::Error> {
    let client = BlockingClient::builder("https://s3.example.com")?
        .region("us-east-1")
        .auth(Auth::from_env()?)
        .build()?;

    let obj = client.objects().get("my-bucket", "path/to/object.txt").send()?;
    let bytes = obj.bytes()?;
    println!("{} bytes", bytes.len());
    Ok(())
}
```

### Configuration

```rust,no_run
use std::time::Duration;

use s3::{AddressingStyle, Auth, Client, TlsRootStore};

# async fn demo() -> Result<(), s3::Error> {
let client = Client::builder("https://s3.example.com")?
    .region("us-east-1")
    .auth(Auth::from_env()?)
    .addressing_style(AddressingStyle::Auto)
    .tls_root_store(TlsRootStore::BackendDefault)
    .timeout(Duration::from_secs(30))
    .max_attempts(3)
    .build()?;
# Ok(())
# }
```

### Operation services

```rust,no_run
use s3::{Auth, Client};

# async fn demo() -> Result<(), s3::Error> {
let client = Client::builder("https://s3.example.com")?
    .region("us-east-1")
    .auth(Auth::from_env()?)
    .build()?;

let buckets = client.buckets().list().send().await?;
let objects = client
    .objects()
    .list_v2("my-bucket")
    .prefix("logs/")?
    .send()
    .await?;

println!("{} buckets, {} objects", buckets.buckets.len(), objects.contents.len());
# Ok(())
# }
```

### Presign

```rust,no_run
use s3::{Auth, Client};

# async fn demo() -> Result<(), s3::Error> {
let client = Client::builder("https://s3.example.com")?
    .region("us-east-1")
    .auth(Auth::from_env()?)
    .build()?;

let presigned = client
    .objects()
    .presign_get("my-bucket", "path/to/object.txt")
    .build()?;

println!("GET {}", presigned.url);
# Ok(())
# }
```

### Endpoint presets (feature = `providers`)

```rust,no_run
use s3::{Auth, providers};

# async fn demo() -> Result<(), s3::Error> {
let preset = providers::minio_local();
let client = preset
    .async_client_builder()?
    .auth(Auth::from_env()?)
    .build()?;
# Ok(())
# }
```

## Authentication and credentials

Use `Auth` as the single entry point for signing strategy. Static credentials, refreshable
providers, and optional cloud credential flows all feed the same client builder API.

- `Auth::Anonymous`: unsigned requests (for public buckets / anonymous endpoints)
- `Auth::from_env()`: static credentials from env vars
- `Auth::provider(...)`: plug in your own refreshable provider (cached/singleflight refresh)
- `Auth::from_imds_with_tls_root_store(...)` / `Auth::from_web_identity_env_with_tls_root_store(...)`: explicit TLS root policy for credentials fetch
- Optional features:
  - `credentials-profile`: shared config/profile loader
  - `credentials-imds`: container metadata or IMDSv2 credentials (async/blocking APIs)
  - `credentials-sts`: web identity / STS flows using regional endpoints; web identity requires
    `AWS_REGION` or `AWS_DEFAULT_REGION`

## Compatibility

- Addressing styles: `AddressingStyle::Auto` (default), `Path`, `VirtualHosted`
- Targets: any S3-compatible service; CI runs against MinIO and RustFS

## Feature flags

- Modes: `async` (default), `blocking`
- TLS: `rustls` (default), `native-tls`
- Optional: `multipart`, `checksums`, `providers`, `credentials-profile`, `credentials-imds`, `credentials-sts`, `tracing`, `metrics`

## Examples

Examples follow the same layering as the crate docs: choose `Auth`, build a client, then work
through `objects()` or `buckets()`.

### Core object and bucket flows

- Basic object lifecycle: `examples/async_put_get_delete.rs`
- List buckets: `examples/async_list_buckets.rs`
- List objects v2 + pagination: `examples/async_list_objects.rs`
- Batch delete: `examples/async_delete_objects_batch.rs`
- Copy object + replace metadata: `examples/async_copy_object.rs`
- Streaming upload (requires Content-Length): `examples/async_put_stream.rs`
- Multipart upload (feature = `multipart`): `examples/async_multipart_upload.rs`

### Auth, presign, and endpoint presets

- Presign with static credentials: `examples/presign_get.rs`
- Presign with a refreshable provider: `examples/async_presign_build_async.rs`
- IMDS credentials (feature = `credentials-imds`): `examples/async_auth_imds.rs`
- Web identity credentials (feature = `credentials-sts`): `examples/async_auth_web_identity.rs`
- MinIO local preset (feature = `providers`): `examples/minio_local_put_get_delete.rs`
- Cloudflare R2 preset (feature = `providers`): `examples/r2_put_get_delete.rs`

### TLS and blocking flows

- Async request TLS root policy: `examples/async_tls_root_store.rs`
- Blocking request TLS root policy: `examples/blocking_tls_root_store.rs`
- Blocking object lifecycle: `examples/blocking_put_get_delete.rs`
- Blocking list buckets: `examples/blocking_list_buckets.rs`
- Blocking presign: `examples/blocking_presign_get.rs`

See `examples/README.md` for environment variables and feature requirements.
