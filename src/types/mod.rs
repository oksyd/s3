//! Public request and response types shared across operations.
//!
//! Callers should work with these stable domain types. Protocol-specific XML DTOs and parsing
//! helpers live in a private submodule so the public API stays focused on S3 concepts.
//!
//! Common entry points in this module include:
//!
//! - [`GetObjectOutput`](crate::types::GetObjectOutput) and
//!   [`BlockingGetObjectOutput`](crate::types::BlockingGetObjectOutput) for object downloads
//! - [`ListObjectsV2Output`](crate::types::ListObjectsV2Output) and
//!   [`Object`](crate::types::Object) for object listings
//! - [`ListBucketsOutput`](crate::types::ListBucketsOutput) and
//!   [`Bucket`](crate::types::Bucket) for bucket listings
//! - [`PresignedRequest`](crate::types::PresignedRequest) for presigned URL generation

use http::{HeaderMap, Method};
use url::Url;

#[cfg(any(
    feature = "checksums",
    feature = "multipart",
    feature = "async",
    feature = "blocking"
))]
use crate::error::Error;
use crate::error::Result;
#[cfg(any(feature = "async", feature = "blocking"))]
use bytes::Bytes;

#[cfg(any(test, feature = "async", feature = "blocking"))]
pub(crate) const MAX_DELETE_OBJECTS_PER_REQUEST: usize = 1_000;

#[cfg(any(test, feature = "async", feature = "blocking"))]
pub(crate) mod xml;

/// Fully resolved presigned request.
#[derive(Clone, Debug)]
pub struct PresignedRequest {
    /// HTTP method to use.
    pub method: Method,
    /// Fully signed request URL.
    pub url: Url,
    /// Headers that must accompany the request.
    pub headers: HeaderMap,
}

#[cfg(feature = "async")]
/// Output from a GET object request.
pub struct GetObjectOutput {
    /// Response body reader for Tokio I/O.
    pub body: std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send + 'static>>,
    /// Entity tag, if provided.
    pub etag: Option<String>,
    /// Content length, if known.
    pub content_length: Option<u64>,
    /// Content type, if provided.
    pub content_type: Option<String>,
}

#[cfg(feature = "blocking")]
/// Output from a blocking GET object request.
pub struct BlockingGetObjectOutput {
    /// Response body reader.
    pub body: Box<dyn std::io::Read + 'static>,
    /// Entity tag, if provided.
    pub etag: Option<String>,
    /// Content length, if known.
    pub content_length: Option<u64>,
    /// Content type, if provided.
    pub content_type: Option<String>,
}

#[cfg(feature = "blocking")]
impl std::fmt::Debug for BlockingGetObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockingGetObjectOutput")
            .field("body", &"<reader>")
            .field("etag", &self.etag)
            .field("content_length", &self.content_length)
            .field("content_type", &self.content_type)
            .finish()
    }
}

#[cfg(feature = "blocking")]
impl BlockingGetObjectOutput {
    /// Reads the full response body into memory.
    pub fn bytes(mut self) -> Result<Bytes> {
        use std::io::Read as _;

        let mut out = Vec::new();
        self.body
            .read_to_end(&mut out)
            .map_err(|e| Error::transport("failed to read response body", Some(Box::new(e))))?;
        Ok(Bytes::from(out))
    }

    /// Copies the response body into a writer without flushing it.
    pub fn write_to<W>(mut self, writer: &mut W) -> Result<u64>
    where
        W: std::io::Write + ?Sized,
    {
        std::io::copy(&mut self.body, writer)
            .map_err(|e| Error::transport("failed to write response body", Some(Box::new(e))))
    }
}

#[cfg(feature = "async")]
impl std::fmt::Debug for GetObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetObjectOutput")
            .field("body", &"<reader>")
            .field("etag", &self.etag)
            .field("content_length", &self.content_length)
            .field("content_type", &self.content_type)
            .finish()
    }
}

#[cfg(feature = "async")]
impl GetObjectOutput {
    /// Reads the full response body into memory.
    pub async fn bytes(mut self) -> Result<Bytes> {
        use tokio::io::AsyncReadExt as _;

        let mut out = Vec::new();
        self.body.read_to_end(&mut out).await.map_err(|error| {
            Error::transport("failed to read response body", Some(Box::new(error)))
        })?;
        Ok(Bytes::from(out))
    }

    /// Copies the response body into a Tokio writer without flushing it.
    pub async fn write_to<W>(mut self, writer: &mut W) -> Result<u64>
    where
        W: tokio::io::AsyncWrite + Unpin + ?Sized,
    {
        tokio::io::copy(&mut self.body, writer)
            .await
            .map_err(|error| {
                Error::transport("failed to write response body", Some(Box::new(error)))
            })
    }
}

/// Output from a HEAD object request.
#[derive(Debug)]
pub struct HeadObjectOutput {
    /// Entity tag, if provided.
    pub etag: Option<String>,
    /// Content length, if known.
    pub content_length: Option<u64>,
    /// Content type, if provided.
    pub content_type: Option<String>,
}

/// Output from a PUT object request.
#[derive(Debug)]
pub struct PutObjectOutput {
    /// Entity tag, if provided.
    pub etag: Option<String>,
}

#[cfg(feature = "checksums")]
/// Supported checksum algorithms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChecksumAlgorithm {
    /// CRC32 (ISO HDLC).
    Crc32,
    /// CRC32C (Castagnoli).
    Crc32c,
    /// CRC64 NVME.
    Crc64Nvme,
    /// SHA-1.
    Sha1,
    /// SHA-256.
    Sha256,
}

#[cfg(feature = "checksums")]
impl ChecksumAlgorithm {
    /// Returns the checksum header name for this algorithm.
    pub fn header_name(self) -> http::header::HeaderName {
        match self {
            Self::Crc32 => http::header::HeaderName::from_static("x-amz-checksum-crc32"),
            Self::Crc32c => http::header::HeaderName::from_static("x-amz-checksum-crc32c"),
            Self::Crc64Nvme => http::header::HeaderName::from_static("x-amz-checksum-crc64nvme"),
            Self::Sha1 => http::header::HeaderName::from_static("x-amz-checksum-sha1"),
            Self::Sha256 => http::header::HeaderName::from_static("x-amz-checksum-sha256"),
        }
    }

    const fn digest_len(self) -> usize {
        match self {
            Self::Crc32 | Self::Crc32c => 4,
            Self::Crc64Nvme => 8,
            Self::Sha1 => 20,
            Self::Sha256 => 32,
        }
    }
}

#[cfg(feature = "checksums")]
/// Checksum value to send with a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checksum {
    algorithm: ChecksumAlgorithm,
    value: String,
}

#[cfg(feature = "checksums")]
impl Checksum {
    /// Creates a checksum from a standard base64-encoded digest.
    pub fn new(algorithm: ChecksumAlgorithm, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_checksum_value(algorithm, &value)?;
        Ok(Self { algorithm, value })
    }

    /// Returns the checksum algorithm.
    pub fn algorithm(&self) -> ChecksumAlgorithm {
        self.algorithm
    }

    /// Returns the standard base64-encoded checksum value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Computes a checksum from raw bytes.
    pub fn from_bytes(algorithm: ChecksumAlgorithm, bytes: impl AsRef<[u8]>) -> Self {
        use base64::Engine as _;

        let bytes = bytes.as_ref();
        let value = match algorithm {
            ChecksumAlgorithm::Crc32 => {
                let checksum = crc_fast::crc32_iso_hdlc(bytes).to_be_bytes();
                base64::engine::general_purpose::STANDARD.encode(checksum)
            }
            ChecksumAlgorithm::Crc32c => {
                let checksum = crc_fast::crc32_iscsi(bytes).to_be_bytes();
                base64::engine::general_purpose::STANDARD.encode(checksum)
            }
            ChecksumAlgorithm::Crc64Nvme => {
                let checksum = crc_fast::crc64_nvme(bytes).to_be_bytes();
                base64::engine::general_purpose::STANDARD.encode(checksum)
            }
            ChecksumAlgorithm::Sha1 => {
                use sha1::Digest as _;

                let digest = sha1::Sha1::digest(bytes);
                base64::engine::general_purpose::STANDARD.encode(digest)
            }
            ChecksumAlgorithm::Sha256 => {
                use graviola::hashing::{Hash as _, Sha256};

                let digest = Sha256::hash(bytes);
                base64::engine::general_purpose::STANDARD.encode(digest.as_ref())
            }
        };

        Self { algorithm, value }
    }

    pub(crate) fn apply(&self, headers: &mut HeaderMap) -> Result<()> {
        let value = http::HeaderValue::from_str(&self.value)
            .map_err(|_| Error::invalid_config("invalid checksum header value"))?;
        headers.insert(self.algorithm.header_name(), value);
        Ok(())
    }
}

#[cfg(feature = "checksums")]
fn validate_checksum_value(algorithm: ChecksumAlgorithm, value: &str) -> Result<()> {
    use base64::Engine as _;

    if value.is_empty() {
        return Err(Error::invalid_config("checksum value must not be empty"));
    }
    if value.trim() != value {
        return Err(Error::invalid_config(
            "checksum value must not include leading or trailing whitespace",
        ));
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value.as_bytes())
        .map_err(|_| Error::invalid_config("checksum value must be standard base64"))?;

    if decoded.len() != algorithm.digest_len() {
        return Err(Error::invalid_config(
            "checksum value length does not match checksum algorithm",
        ));
    }

    Ok(())
}

#[cfg(feature = "multipart")]
fn validate_completed_part_number(part_number: u32) -> Result<()> {
    if part_number == 0 || part_number > 10_000 {
        return Err(Error::invalid_config(
            "completed part number must be in the range 1..=10000",
        ));
    }
    Ok(())
}

#[cfg(feature = "multipart")]
fn validate_completed_part_etag(etag: &str) -> Result<()> {
    if etag.is_empty() {
        return Err(Error::invalid_config(
            "completed part etag must not be empty",
        ));
    }
    if etag.trim() != etag {
        return Err(Error::invalid_config(
            "completed part etag must not include leading or trailing whitespace",
        ));
    }
    if etag
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(Error::invalid_config(
            "completed part etag must not contain ASCII control or whitespace characters",
        ));
    }
    if !etag.starts_with('"') || !etag.ends_with('"') || etag.len() < 2 {
        return Err(Error::invalid_config("completed part etag must be quoted"));
    }
    let inner = &etag[1..etag.len() - 1];
    if inner.is_empty() || inner.contains('"') {
        return Err(Error::invalid_config(
            "completed part etag must contain a non-empty quoted token",
        ));
    }
    Ok(())
}

/// Output from a multi-delete request.
#[derive(Debug)]
pub struct DeleteObjectsOutput {
    /// Successfully deleted objects.
    pub deleted: Vec<DeletedObject>,
    /// Per-object errors.
    pub errors: Vec<DeleteObjectError>,
}

/// Identifier for an object in delete requests.
#[derive(Clone, Debug)]
pub struct DeleteObjectIdentifier {
    key: String,
    version_id: Option<String>,
}

impl DeleteObjectIdentifier {
    /// Creates an identifier from an object key.
    pub fn new(key: impl Into<String>) -> Result<Self> {
        let key = key.into();
        crate::util::validation::validate_object_key(&key)?;
        Ok(Self {
            key,
            version_id: None,
        })
    }

    /// Sets the version id for this identifier.
    pub fn with_version_id(mut self, version_id: impl Into<String>) -> Result<Self> {
        let version_id = version_id.into();
        crate::util::validation::validate_version_id(&version_id)?;
        self.version_id = Some(version_id);
        Ok(self)
    }

    /// Returns the object key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the optional version id.
    pub fn version_id(&self) -> Option<&str> {
        self.version_id.as_deref()
    }
}

/// Successfully deleted object metadata.
#[derive(Debug)]
pub struct DeletedObject {
    /// Object key, if reported.
    pub key: Option<String>,
    /// Version id, if reported.
    pub version_id: Option<String>,
    /// Whether a delete marker was created.
    pub delete_marker: Option<bool>,
    /// Delete marker version id, if reported.
    pub delete_marker_version_id: Option<String>,
}

/// Error metadata for a failed delete entry.
#[derive(Debug)]
pub struct DeleteObjectError {
    /// Object key, if reported.
    pub key: Option<String>,
    /// Version id, if reported.
    pub version_id: Option<String>,
    /// Error code, if reported.
    pub code: Option<String>,
    /// Error message, if reported.
    pub message: Option<String>,
}

/// Output from a copy object request.
#[derive(Debug)]
pub struct CopyObjectOutput {
    /// Entity tag, if provided.
    pub etag: Option<String>,
    /// Last-modified timestamp, if provided.
    pub last_modified: Option<String>,
}

#[cfg(feature = "multipart")]
/// Output from initiating a multipart upload.
#[derive(Debug)]
pub struct CreateMultipartUploadOutput {
    /// Bucket name, if provided.
    pub bucket: Option<String>,
    /// Object key, if provided.
    pub key: Option<String>,
    /// Upload id to use for subsequent part uploads.
    pub upload_id: String,
}

#[cfg(feature = "multipart")]
/// Output from uploading a multipart part.
#[derive(Debug)]
pub struct UploadPartOutput {
    /// Entity tag for the uploaded part.
    pub etag: Option<String>,
}

#[cfg(feature = "multipart")]
/// Completed part descriptor for multipart completion.
#[derive(Clone, Debug)]
pub struct CompletedPart {
    part_number: u32,
    etag: String,
}

#[cfg(feature = "multipart")]
impl CompletedPart {
    /// Creates a completed part descriptor.
    pub fn new(part_number: u32, etag: impl Into<String>) -> Result<Self> {
        validate_completed_part_number(part_number)?;
        let etag = etag.into();
        validate_completed_part_etag(&etag)?;
        Ok(Self { part_number, etag })
    }

    /// Returns the part number.
    pub fn part_number(&self) -> u32 {
        self.part_number
    }

    /// Returns the quoted ETag for this part.
    pub fn etag(&self) -> &str {
        &self.etag
    }
}

#[cfg(feature = "multipart")]
/// Output from completing a multipart upload.
#[derive(Debug)]
pub struct CompleteMultipartUploadOutput {
    /// Object location, if provided.
    pub location: Option<String>,
    /// Bucket name, if provided.
    pub bucket: Option<String>,
    /// Object key, if provided.
    pub key: Option<String>,
    /// Entity tag, if provided.
    pub etag: Option<String>,
}

#[cfg(feature = "multipart")]
/// Output from listing multipart parts.
#[derive(Debug)]
pub struct ListPartsOutput {
    /// Bucket name, if provided.
    pub bucket: Option<String>,
    /// Object key, if provided.
    pub key: Option<String>,
    /// Upload id, if provided.
    pub upload_id: Option<String>,
    /// Whether the listing is truncated.
    pub is_truncated: bool,
    /// Marker for the current page.
    pub part_number_marker: Option<u32>,
    /// Marker for the next page.
    pub next_part_number_marker: Option<u32>,
    /// Maximum number of parts requested.
    pub max_parts: Option<u32>,
    /// Listed parts.
    pub parts: Vec<Part>,
}

#[cfg(feature = "multipart")]
/// Metadata for a multipart part.
#[derive(Debug)]
pub struct Part {
    /// Part number.
    pub part_number: u32,
    /// Entity tag, if provided.
    pub etag: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// Last-modified timestamp, if provided.
    pub last_modified: Option<String>,
}

#[cfg(feature = "multipart")]
/// Output from copying a multipart part.
#[derive(Debug)]
pub struct UploadPartCopyOutput {
    /// Entity tag, if provided.
    pub etag: Option<String>,
    /// Last-modified timestamp, if provided.
    pub last_modified: Option<String>,
}

/// Output from a ListObjectsV2 request.
#[derive(Debug)]
pub struct ListObjectsV2Output {
    /// Bucket name.
    pub name: String,
    /// Prefix filter, if any.
    pub prefix: Option<String>,
    /// Delimiter used for grouping, if any.
    pub delimiter: Option<String>,
    /// Whether the listing is truncated.
    pub is_truncated: bool,
    /// Number of keys returned, if reported.
    pub key_count: Option<u32>,
    /// Maximum number of keys requested.
    pub max_keys: Option<u32>,
    /// Continuation token used for this response, if any.
    pub continuation_token: Option<String>,
    /// Continuation token for the next page, if any.
    pub next_continuation_token: Option<String>,
    /// Listed objects.
    pub contents: Vec<Object>,
    /// Common prefixes when using delimiters.
    pub common_prefixes: Vec<String>,
}

/// Object metadata returned by list operations.
#[derive(Debug)]
pub struct Object {
    /// Object key.
    pub key: String,
    /// Object size in bytes.
    pub size: u64,
    /// Entity tag, if provided.
    pub etag: Option<String>,
    /// Last-modified timestamp, if provided.
    pub last_modified: Option<String>,
    /// Storage class, if provided.
    pub storage_class: Option<String>,
}

/// Output from listing buckets.
#[derive(Debug)]
pub struct ListBucketsOutput {
    /// Owner information, if provided.
    pub owner: Option<BucketOwner>,
    /// Buckets returned in the response.
    pub buckets: Vec<Bucket>,
}

/// Bucket owner metadata.
#[derive(Debug)]
pub struct BucketOwner {
    /// Owner id, if provided.
    pub id: Option<String>,
    /// Owner display name, if provided.
    pub display_name: Option<String>,
}

/// Bucket listing entry.
#[derive(Debug)]
pub struct Bucket {
    /// Bucket name.
    pub name: String,
    /// Creation date, if provided.
    pub creation_date: Option<String>,
}

/// Output from a HEAD bucket request.
#[derive(Debug)]
pub struct HeadBucketOutput {
    /// Bucket region, if provided.
    pub region: Option<String>,
}

/// Bucket versioning configuration.
#[derive(Clone, Debug, Default)]
pub struct BucketVersioningConfiguration {
    /// Versioning status.
    pub status: Option<BucketVersioningStatus>,
    /// MFA delete status.
    pub mfa_delete: Option<BucketMfaDeleteStatus>,
}

/// Versioning status values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BucketVersioningStatus {
    /// Enable versioning.
    Enabled,
    /// Suspend versioning.
    Suspended,
}

/// MFA delete status values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BucketMfaDeleteStatus {
    /// Enable MFA delete.
    Enabled,
    /// Disable MFA delete.
    Disabled,
}

/// Bucket lifecycle configuration.
#[derive(Clone, Debug, Default)]
pub struct BucketLifecycleConfiguration {
    /// Lifecycle rules.
    pub rules: Vec<BucketLifecycleRule>,
}

/// Lifecycle rule definition.
#[derive(Clone, Debug)]
pub struct BucketLifecycleRule {
    /// Optional rule id.
    pub id: Option<String>,
    /// Rule status.
    pub status: BucketLifecycleStatus,
    /// Prefix filter.
    pub prefix: Option<String>,
    /// Expiration in days.
    pub expiration_days: Option<u32>,
    /// Expiration date (ISO 8601), if provided.
    pub expiration_date: Option<String>,
}

/// Lifecycle rule status values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BucketLifecycleStatus {
    /// Enable the rule.
    #[default]
    Enabled,
    /// Disable the rule.
    Disabled,
}

/// Bucket CORS configuration.
#[derive(Clone, Debug, Default)]
pub struct BucketCorsConfiguration {
    /// CORS rules.
    pub rules: Vec<BucketCorsRule>,
}

/// Bucket CORS rule definition.
#[derive(Clone, Debug)]
pub struct BucketCorsRule {
    /// Optional rule id.
    pub id: Option<String>,
    /// Allowed origins.
    pub allowed_origins: Vec<String>,
    /// Allowed methods.
    pub allowed_methods: Vec<CorsMethod>,
    /// Allowed headers.
    pub allowed_headers: Vec<String>,
    /// Exposed headers.
    pub expose_headers: Vec<String>,
    /// Max age in seconds.
    pub max_age_seconds: Option<u32>,
}

/// Allowed CORS method.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorsMethod {
    /// HTTP GET.
    Get,
    /// HTTP PUT.
    Put,
    /// HTTP POST.
    Post,
    /// HTTP DELETE.
    Delete,
    /// HTTP HEAD.
    Head,
    /// Custom method.
    Other(String),
}

impl CorsMethod {
    /// Returns the wire value for this method.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Put => "PUT",
            Self::Post => "POST",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Other(v) => v.as_str(),
        }
    }
}

/// Bucket tag set.
#[derive(Clone, Debug, Default)]
pub struct BucketTagging {
    /// Tags associated with the bucket.
    pub tags: Vec<Tag>,
}

/// Tag key/value pair.
#[derive(Clone, Debug)]
pub struct Tag {
    /// Tag key.
    pub key: String,
    /// Tag value.
    pub value: String,
}

/// Bucket encryption configuration.
#[derive(Clone, Debug, Default)]
pub struct BucketEncryptionConfiguration {
    /// Encryption rules.
    pub rules: Vec<BucketEncryptionRule>,
}

/// Bucket encryption rule definition.
#[derive(Clone, Debug)]
pub struct BucketEncryptionRule {
    /// Default server-side encryption settings.
    pub apply: ApplyServerSideEncryptionByDefault,
    /// Whether bucket keys are enabled.
    pub bucket_key_enabled: Option<bool>,
}

/// Default server-side encryption settings.
#[derive(Clone, Debug)]
pub struct ApplyServerSideEncryptionByDefault {
    /// Server-side encryption algorithm.
    pub sse_algorithm: SseAlgorithm,
    /// KMS master key id, if applicable.
    pub kms_master_key_id: Option<String>,
}

/// Server-side encryption algorithm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseAlgorithm {
    /// AES-256.
    Aes256,
    /// AWS KMS.
    AwsKms,
    /// AWS KMS with DSSE.
    AwsKmsDsse,
    /// Custom algorithm.
    Other(String),
}

impl SseAlgorithm {
    /// Returns the wire value for this algorithm.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Aes256 => "AES256",
            Self::AwsKms => "aws:kms",
            Self::AwsKmsDsse => "aws:kms:dsse",
            Self::Other(v) => v.as_str(),
        }
    }
}

/// Bucket public access block configuration.
#[derive(Clone, Debug, Default)]
pub struct BucketPublicAccessBlockConfiguration {
    /// Block public ACLs.
    pub block_public_acls: bool,
    /// Ignore public ACLs.
    pub ignore_public_acls: bool,
    /// Block public bucket policies.
    pub block_public_policy: bool,
    /// Restrict public buckets.
    pub restrict_public_buckets: bool,
}

#[cfg(all(test, feature = "checksums"))]
mod checksum_tests {
    use super::{Checksum, ChecksumAlgorithm};
    use http::HeaderMap;

    #[test]
    fn from_bytes_matches_known_vectors() {
        let bytes = b"hello";

        assert_eq!(
            Checksum::from_bytes(ChecksumAlgorithm::Sha256, bytes).value,
            "LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ="
        );
        assert_eq!(
            Checksum::from_bytes(ChecksumAlgorithm::Sha1, bytes).value,
            "qvTGHdzF6KLavt4PO0gs2a6pQ00="
        );
        assert_eq!(
            Checksum::from_bytes(ChecksumAlgorithm::Crc32, bytes).value,
            "NhCmhg=="
        );
        assert_eq!(
            Checksum::from_bytes(ChecksumAlgorithm::Crc32c, bytes).value,
            "mnG7TA=="
        );
    }

    #[test]
    fn crc_from_bytes_matches_standard_vectors() {
        let cases: &[(ChecksumAlgorithm, &[u8], &str)] = &[
            (ChecksumAlgorithm::Crc32, b"123456789", "y/Q5Jg=="),
            (ChecksumAlgorithm::Crc32c, b"123456789", "4waSgw=="),
            (ChecksumAlgorithm::Crc64Nvme, b"123456789", "rosUhgp5mIg="),
            (ChecksumAlgorithm::Crc32, b"", "AAAAAA=="),
            (ChecksumAlgorithm::Crc32c, b"", "AAAAAA=="),
            (ChecksumAlgorithm::Crc64Nvme, b"", "AAAAAAAAAAA="),
        ];

        for &(algorithm, bytes, expected) in cases {
            let checksum = Checksum::from_bytes(algorithm, bytes);

            assert_eq!(checksum.algorithm(), algorithm);
            assert_eq!(checksum.value(), expected);
        }
    }

    #[test]
    fn checksum_algorithms_use_s3_header_names() {
        let cases = [
            (ChecksumAlgorithm::Crc32, "x-amz-checksum-crc32"),
            (ChecksumAlgorithm::Crc32c, "x-amz-checksum-crc32c"),
            (ChecksumAlgorithm::Crc64Nvme, "x-amz-checksum-crc64nvme"),
            (ChecksumAlgorithm::Sha1, "x-amz-checksum-sha1"),
            (ChecksumAlgorithm::Sha256, "x-amz-checksum-sha256"),
        ];

        for (algorithm, expected) in cases {
            assert_eq!(algorithm.header_name().as_str(), expected);
        }
    }

    #[test]
    fn apply_writes_checksum_header() {
        let cases = [
            (ChecksumAlgorithm::Crc32, "x-amz-checksum-crc32"),
            (ChecksumAlgorithm::Crc32c, "x-amz-checksum-crc32c"),
            (ChecksumAlgorithm::Crc64Nvme, "x-amz-checksum-crc64nvme"),
            (ChecksumAlgorithm::Sha1, "x-amz-checksum-sha1"),
            (ChecksumAlgorithm::Sha256, "x-amz-checksum-sha256"),
        ];

        for (algorithm, header_name) in cases {
            let mut headers = HeaderMap::new();
            let checksum = Checksum::from_bytes(algorithm, b"hello");
            let expected = checksum.value().to_string();
            checksum
                .apply(&mut headers)
                .expect("checksum header should be valid");

            let value = headers
                .get(header_name)
                .expect("checksum header should be present");
            assert_eq!(value.to_str().ok(), Some(expected.as_str()));
        }
    }

    #[test]
    fn new_accepts_valid_pre_encoded_checksum() {
        let checksum = Checksum::new(ChecksumAlgorithm::Crc32, "NhCmhg==")
            .expect("valid checksum should be accepted");

        assert_eq!(checksum.algorithm(), ChecksumAlgorithm::Crc32);
        assert_eq!(checksum.value(), "NhCmhg==");
    }

    #[test]
    fn new_rejects_invalid_checksum_values() {
        let cases = [
            (ChecksumAlgorithm::Crc32, ""),
            (ChecksumAlgorithm::Crc32, " AAAAAA=="),
            (ChecksumAlgorithm::Crc32, "invalid\nvalue"),
            (ChecksumAlgorithm::Crc32, "not-base64"),
            (ChecksumAlgorithm::Crc32, "AAAAAAAAAAA="),
        ];

        for (algorithm, value) in cases {
            assert!(Checksum::new(algorithm, value).is_err());
        }
    }
}
