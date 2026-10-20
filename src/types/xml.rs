use serde::Deserialize;

use super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct XmlError {
    #[serde(rename = "Code")]
    pub(crate) code: Option<String>,
    #[serde(rename = "Message")]
    pub(crate) message: Option<String>,
    #[serde(rename = "RequestId")]
    pub(crate) request_id: Option<String>,
    #[serde(rename = "HostId")]
    pub(crate) host_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlListBucketResult {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "Prefix")]
    pub(crate) prefix: Option<String>,
    #[serde(rename = "Delimiter")]
    pub(crate) delimiter: Option<String>,
    #[serde(rename = "IsTruncated")]
    pub(crate) is_truncated: bool,
    #[serde(rename = "KeyCount")]
    pub(crate) key_count: Option<u32>,
    #[serde(rename = "MaxKeys")]
    pub(crate) max_keys: Option<u32>,
    #[serde(rename = "ContinuationToken")]
    pub(crate) continuation_token: Option<String>,
    #[serde(rename = "NextContinuationToken")]
    pub(crate) next_continuation_token: Option<String>,
    #[serde(rename = "Contents", default)]
    pub(crate) contents: Vec<XmlObject>,
    #[serde(rename = "CommonPrefixes", default)]
    pub(crate) common_prefixes: Vec<XmlCommonPrefixes>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlObject {
    #[serde(rename = "Key")]
    pub(crate) key: String,
    #[serde(rename = "LastModified")]
    pub(crate) last_modified: Option<String>,
    #[serde(rename = "ETag")]
    pub(crate) etag: Option<String>,
    #[serde(rename = "Size")]
    pub(crate) size: u64,
    #[serde(rename = "StorageClass")]
    pub(crate) storage_class: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlCommonPrefixes {
    #[serde(rename = "Prefix")]
    pub(crate) prefix: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlDeleteResult {
    #[serde(rename = "Deleted", default)]
    pub(crate) deleted: Vec<XmlDeleted>,
    #[serde(rename = "Error", default)]
    pub(crate) errors: Vec<XmlDeleteError>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlDeleted {
    #[serde(rename = "Key")]
    pub(crate) key: Option<String>,
    #[serde(rename = "VersionId")]
    pub(crate) version_id: Option<String>,
    #[serde(rename = "DeleteMarker")]
    pub(crate) delete_marker: Option<bool>,
    #[serde(rename = "DeleteMarkerVersionId")]
    pub(crate) delete_marker_version_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlDeleteError {
    #[serde(rename = "Key")]
    pub(crate) key: Option<String>,
    #[serde(rename = "VersionId")]
    pub(crate) version_id: Option<String>,
    #[serde(rename = "Code")]
    pub(crate) code: Option<String>,
    #[serde(rename = "Message")]
    pub(crate) message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlCopyObjectResult {
    #[serde(rename = "ETag")]
    pub(crate) etag: Option<String>,
    #[serde(rename = "LastModified")]
    pub(crate) last_modified: Option<String>,
}

#[cfg(feature = "multipart")]
#[derive(Debug, Deserialize)]
pub(crate) struct XmlInitiateMultipartUploadResult {
    #[serde(rename = "Bucket")]
    pub(crate) bucket: Option<String>,
    #[serde(rename = "Key")]
    pub(crate) key: Option<String>,
    #[serde(rename = "UploadId")]
    pub(crate) upload_id: String,
}

#[cfg(feature = "multipart")]
#[derive(Debug, Deserialize)]
pub(crate) struct XmlCompleteMultipartUploadResult {
    #[serde(rename = "Location")]
    pub(crate) location: Option<String>,
    #[serde(rename = "Bucket")]
    pub(crate) bucket: Option<String>,
    #[serde(rename = "Key")]
    pub(crate) key: Option<String>,
    #[serde(rename = "ETag")]
    pub(crate) etag: Option<String>,
}

#[cfg(feature = "multipart")]
#[derive(Debug, Deserialize)]
pub(crate) struct XmlListPartsResult {
    #[serde(rename = "Bucket")]
    pub(crate) bucket: Option<String>,
    #[serde(rename = "Key")]
    pub(crate) key: Option<String>,
    #[serde(rename = "UploadId")]
    pub(crate) upload_id: Option<String>,
    #[serde(rename = "IsTruncated")]
    pub(crate) is_truncated: bool,
    #[serde(rename = "PartNumberMarker")]
    pub(crate) part_number_marker: Option<u32>,
    #[serde(rename = "NextPartNumberMarker")]
    pub(crate) next_part_number_marker: Option<u32>,
    #[serde(rename = "MaxParts")]
    pub(crate) max_parts: Option<u32>,
    #[serde(rename = "Part", default)]
    pub(crate) parts: Vec<XmlPart>,
}

#[cfg(feature = "multipart")]
#[derive(Debug, Deserialize)]
pub(crate) struct XmlPart {
    #[serde(rename = "PartNumber")]
    pub(crate) part_number: u32,
    #[serde(rename = "ETag")]
    pub(crate) etag: Option<String>,
    #[serde(rename = "Size")]
    pub(crate) size: u64,
    #[serde(rename = "LastModified")]
    pub(crate) last_modified: Option<String>,
}

#[cfg(feature = "multipart")]
#[derive(Debug, Deserialize)]
pub(crate) struct XmlCopyPartResult {
    #[serde(rename = "ETag")]
    pub(crate) etag: Option<String>,
    #[serde(rename = "LastModified")]
    pub(crate) last_modified: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlListAllMyBucketsResult {
    #[serde(rename = "Owner")]
    pub(crate) owner: Option<XmlOwner>,
    #[serde(rename = "Buckets")]
    pub(crate) buckets: XmlBuckets,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlOwner {
    #[serde(rename = "ID")]
    pub(crate) id: Option<String>,
    #[serde(rename = "DisplayName")]
    pub(crate) display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlBuckets {
    #[serde(rename = "Bucket", default)]
    pub(crate) buckets: Vec<XmlBucket>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlBucket {
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "CreationDate")]
    pub(crate) creation_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlVersioningConfiguration {
    #[serde(rename = "Status")]
    pub(crate) status: Option<String>,
    #[serde(rename = "MfaDelete")]
    pub(crate) mfa_delete: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlLifecycleConfiguration {
    #[serde(rename = "Rule", default)]
    pub(crate) rules: Vec<XmlLifecycleRule>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlLifecycleRule {
    #[serde(rename = "ID")]
    pub(crate) id: Option<String>,
    #[serde(rename = "Status")]
    pub(crate) status: String,
    #[serde(rename = "Prefix")]
    pub(crate) prefix: Option<String>,
    #[serde(rename = "Filter")]
    pub(crate) filter: Option<XmlLifecycleFilter>,
    #[serde(rename = "Expiration")]
    pub(crate) expiration: Option<XmlExpiration>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlLifecycleFilter {
    #[serde(rename = "Prefix")]
    pub(crate) prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlExpiration {
    #[serde(rename = "Days")]
    pub(crate) days: Option<u32>,
    #[serde(rename = "Date")]
    pub(crate) date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlCorsConfiguration {
    #[serde(rename = "CORSRule", default)]
    pub(crate) rules: Vec<XmlCorsRule>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlCorsRule {
    #[serde(rename = "ID")]
    pub(crate) id: Option<String>,
    #[serde(rename = "AllowedOrigin", default)]
    pub(crate) allowed_origins: Vec<String>,
    #[serde(rename = "AllowedMethod", default)]
    pub(crate) allowed_methods: Vec<String>,
    #[serde(rename = "AllowedHeader", default)]
    pub(crate) allowed_headers: Vec<String>,
    #[serde(rename = "ExposeHeader", default)]
    pub(crate) expose_headers: Vec<String>,
    #[serde(rename = "MaxAgeSeconds")]
    pub(crate) max_age_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlTagging {
    #[serde(rename = "TagSet")]
    pub(crate) tag_set: XmlTagSet,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlTagSet {
    #[serde(rename = "Tag", default)]
    pub(crate) tags: Vec<XmlTag>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlTag {
    #[serde(rename = "Key")]
    pub(crate) key: String,
    #[serde(rename = "Value")]
    pub(crate) value: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlServerSideEncryptionConfiguration {
    #[serde(rename = "Rule", default)]
    pub(crate) rules: Vec<XmlServerSideEncryptionRule>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlServerSideEncryptionRule {
    #[serde(rename = "ApplyServerSideEncryptionByDefault")]
    pub(crate) apply: Option<XmlApplyServerSideEncryptionByDefault>,
    #[serde(rename = "BucketKeyEnabled")]
    pub(crate) bucket_key_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlApplyServerSideEncryptionByDefault {
    #[serde(rename = "SSEAlgorithm")]
    pub(crate) sse_algorithm: String,
    #[serde(rename = "KMSMasterKeyID")]
    pub(crate) kms_master_key_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlPublicAccessBlockConfiguration {
    #[serde(rename = "BlockPublicAcls")]
    pub(crate) block_public_acls: Option<bool>,
    #[serde(rename = "IgnorePublicAcls")]
    pub(crate) ignore_public_acls: Option<bool>,
    #[serde(rename = "BlockPublicPolicy")]
    pub(crate) block_public_policy: Option<bool>,
    #[serde(rename = "RestrictPublicBuckets")]
    pub(crate) restrict_public_buckets: Option<bool>,
}

impl From<XmlListBucketResult> for ListObjectsV2Output {
    fn from(value: XmlListBucketResult) -> Self {
        Self {
            name: value.name,
            prefix: value.prefix,
            delimiter: value.delimiter,
            is_truncated: value.is_truncated,
            key_count: value.key_count,
            max_keys: value.max_keys,
            continuation_token: value.continuation_token,
            next_continuation_token: value.next_continuation_token,
            contents: value
                .contents
                .into_iter()
                .map(|o| Object {
                    key: o.key,
                    size: o.size,
                    etag: o.etag,
                    last_modified: o.last_modified,
                    storage_class: o.storage_class,
                })
                .collect(),
            common_prefixes: value
                .common_prefixes
                .into_iter()
                .map(|p| p.prefix)
                .collect(),
        }
    }
}

impl From<XmlListAllMyBucketsResult> for ListBucketsOutput {
    fn from(value: XmlListAllMyBucketsResult) -> Self {
        let buckets = value
            .buckets
            .buckets
            .into_iter()
            .map(|bucket| Bucket {
                name: bucket.name,
                creation_date: bucket.creation_date,
            })
            .collect();

        Self {
            owner: value.owner.map(|o| BucketOwner {
                id: o.id,
                display_name: o.display_name,
            }),
            buckets,
        }
    }
}

impl From<XmlDeleteResult> for DeleteObjectsOutput {
    fn from(value: XmlDeleteResult) -> Self {
        Self {
            deleted: value
                .deleted
                .into_iter()
                .map(|d| DeletedObject {
                    key: d.key,
                    version_id: d.version_id,
                    delete_marker: d.delete_marker,
                    delete_marker_version_id: d.delete_marker_version_id,
                })
                .collect(),
            errors: value
                .errors
                .into_iter()
                .map(|e| DeleteObjectError {
                    key: e.key,
                    version_id: e.version_id,
                    code: e.code,
                    message: e.message,
                })
                .collect(),
        }
    }
}

impl From<XmlCopyObjectResult> for CopyObjectOutput {
    fn from(value: XmlCopyObjectResult) -> Self {
        Self {
            etag: value.etag,
            last_modified: value.last_modified,
        }
    }
}

#[cfg(feature = "multipart")]
impl From<XmlInitiateMultipartUploadResult> for CreateMultipartUploadOutput {
    fn from(value: XmlInitiateMultipartUploadResult) -> Self {
        Self {
            bucket: value.bucket,
            key: value.key,
            upload_id: value.upload_id,
        }
    }
}

#[cfg(feature = "multipart")]
impl From<XmlCompleteMultipartUploadResult> for CompleteMultipartUploadOutput {
    fn from(value: XmlCompleteMultipartUploadResult) -> Self {
        Self {
            location: value.location,
            bucket: value.bucket,
            key: value.key,
            etag: value.etag,
        }
    }
}

#[cfg(feature = "multipart")]
impl From<XmlListPartsResult> for ListPartsOutput {
    fn from(value: XmlListPartsResult) -> Self {
        Self {
            bucket: value.bucket,
            key: value.key,
            upload_id: value.upload_id,
            is_truncated: value.is_truncated,
            part_number_marker: value.part_number_marker,
            next_part_number_marker: value.next_part_number_marker,
            max_parts: value.max_parts,
            parts: value
                .parts
                .into_iter()
                .map(|p| Part {
                    part_number: p.part_number,
                    etag: p.etag,
                    size: p.size,
                    last_modified: p.last_modified,
                })
                .collect(),
        }
    }
}

#[cfg(feature = "multipart")]
impl From<XmlCopyPartResult> for UploadPartCopyOutput {
    fn from(value: XmlCopyPartResult) -> Self {
        Self {
            etag: value.etag,
            last_modified: value.last_modified,
        }
    }
}
