# Examples

Run commands from the repository root. The examples below use rustls with ring;
async is enabled by default, while blocking commands select their feature explicitly.

## Setup

Configure your service and an existing bucket:

```sh
export S3_TEST_ENDPOINT="http://127.0.0.1:9000"
export S3_TEST_REGION="us-east-1"
export S3_TEST_BUCKET="my-bucket"
export AWS_ACCESS_KEY_ID="<access-key-id>"
export AWS_SECRET_ACCESS_KEY="<secret-access-key>"
```

Replace these values with your configuration. Use a service endpoint without the
bucket name. Temporary credentials also require `AWS_SESSION_TOKEN`.
Bucket-listing examples do not need `S3_TEST_BUCKET`.

## Objects and buckets

Choose an operation:

```sh
# Async
cargo run --example async_list_buckets
cargo run --example async_list_objects
cargo run --example async_put_get_delete
cargo run --example async_put_stream
cargo run --example async_copy_object
cargo run --example async_delete_objects_batch
cargo run --example async_multipart_upload --features multipart

# Blocking
cargo run --example blocking_list_buckets --no-default-features --features blocking-tls-rustls-ring
cargo run --example blocking_put_get_delete --no-default-features --features blocking-tls-rustls-ring
```

## Presigned URLs

These examples generate URLs locally with embedded dummy credentials. No setup or
service connection is required.

```sh
cargo run --example presign_get
cargo run --example async_presign_build_async
cargo run --example blocking_presign_get --no-default-features --features blocking-tls-rustls-ring
```

## Credential providers

These examples list buckets using provider credentials. Set `S3_TEST_ENDPOINT` and
`S3_TEST_REGION` for the service accessible to those credentials.

**Container metadata / IMDSv2:** run on EC2 with an IAM role and IMDSv2 enabled,
or with `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` or `AWS_CONTAINER_CREDENTIALS_FULL_URI`
configured by your container environment.

```sh
cargo run --example async_auth_imds --features credentials-imds
```

**Web identity / STS:** provide a role ARN, token file, and STS region.

```sh
export AWS_ROLE_ARN="arn:aws:iam::123456789012:role/my-role"
export AWS_WEB_IDENTITY_TOKEN_FILE="/path/to/token"
export AWS_REGION="$S3_TEST_REGION"
cargo run --example async_auth_web_identity --features credentials-sts
```

## Endpoint presets

**Local MinIO:** uses `http://127.0.0.1:9000` and `us-east-1`, plus the bucket and
AWS credentials from Setup. It ignores `S3_TEST_ENDPOINT` and `S3_TEST_REGION`.

```sh
cargo run --example minio_local_put_get_delete
```

**Cloudflare R2:** uses its own credentials and an existing R2 bucket.

```sh
export R2_ACCOUNT_ID="<account-id>"
export R2_BUCKET="my-bucket"
export R2_ACCESS_KEY_ID="<access-key-id>"
export R2_SECRET_ACCESS_KEY="<secret-access-key>"
cargo run --example r2_put_get_delete
```

Optional: `R2_SESSION_TOKEN` for temporary credentials; `R2_JURISDICTION` for
`global` (default), `eu`, or `fedramp`.

## TLS trust roots

Use the Setup configuration with an HTTPS endpoint. These examples list buckets
using system trust roots:

```sh
S3_TLS_ROOT_STORE=system cargo run --example async_tls_root_store
S3_TLS_ROOT_STORE=system cargo run --example blocking_tls_root_store --no-default-features --features blocking-tls-rustls-ring
```

`S3_TLS_ROOT_STORE` accepts `backend-default` (default), `webpki`, or `system`.
The credential-provider examples also accept this setting for S3 requests and
`S3_CREDENTIALS_TLS_ROOT_STORE` for credential requests.
