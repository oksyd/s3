//! Service entry points and request builders.
//!
//! Client instances expose `objects()` and `buckets()` services from this module. Request
//! builders live here; shared request and response models live in [`crate::types`].
//!
//! ## Start here
//!
//! Async clients usually begin with:
//!
//! - [`crate::Client::objects`] -> [`ObjectsService`](crate::api::ObjectsService)
//! - [`crate::Client::buckets`] -> [`BucketsService`](crate::api::BucketsService)
//!
//! Blocking clients usually begin with:
//!
//! - [`crate::BlockingClient::objects`] ->
//!   [`BlockingObjectsService`](crate::api::BlockingObjectsService)
//! - [`crate::BlockingClient::buckets`] ->
//!   [`BlockingBucketsService`](crate::api::BlockingBucketsService)
//!
//! Each service returns typed request builders such as
//! [`GetObjectRequest`](crate::api::GetObjectRequest),
//! [`PutObjectRequest`](crate::api::PutObjectRequest),
//! [`ListObjectsV2Request`](crate::api::ListObjectsV2Request), and
//! [`PutBucketLifecycleRequest`](crate::api::PutBucketLifecycleRequest).

#[cfg(feature = "blocking")]
mod blocking_buckets;
#[cfg(feature = "blocking")]
mod blocking_objects;
#[cfg(feature = "async")]
mod buckets;
mod common;
#[cfg(feature = "async")]
mod objects;

#[cfg(feature = "async")]
pub use buckets::{
    BucketsService, CreateBucketRequest, DeleteBucketConfigRawRequest, DeleteBucketCorsRequest,
    DeleteBucketEncryptionRequest, DeleteBucketLifecycleRequest,
    DeleteBucketPublicAccessBlockRequest, DeleteBucketRequest, DeleteBucketTaggingRequest,
    GetBucketConfigRawRequest, GetBucketCorsRequest, GetBucketEncryptionRequest,
    GetBucketLifecycleRequest, GetBucketPublicAccessBlockRequest, GetBucketTaggingRequest,
    GetBucketVersioningRequest, HeadBucketRequest, ListBucketsRequest, PutBucketConfigRawRequest,
    PutBucketCorsRequest, PutBucketEncryptionRequest, PutBucketLifecycleRequest,
    PutBucketPublicAccessBlockRequest, PutBucketTaggingRequest, PutBucketVersioningRequest,
};

#[cfg(feature = "async")]
pub use objects::{
    CopyObjectRequest, DeleteObjectRequest, DeleteObjectsRequest, GetObjectRequest,
    HeadObjectRequest, ListObjectsV2Pager, ListObjectsV2Request, ObjectsService,
    PresignObjectRequest, PutObjectRequest,
};

#[cfg(all(feature = "async", feature = "multipart"))]
pub use objects::{
    AbortMultipartUploadRequest, CompleteMultipartUploadRequest, CreateMultipartUploadRequest,
    ListPartsRequest, UploadPartCopyRequest, UploadPartRequest,
};

#[cfg(feature = "blocking")]
pub use blocking_buckets::{
    BlockingBucketsService, BlockingCreateBucketRequest, BlockingDeleteBucketConfigRawRequest,
    BlockingDeleteBucketCorsRequest, BlockingDeleteBucketEncryptionRequest,
    BlockingDeleteBucketLifecycleRequest, BlockingDeleteBucketPublicAccessBlockRequest,
    BlockingDeleteBucketRequest, BlockingDeleteBucketTaggingRequest,
    BlockingGetBucketConfigRawRequest, BlockingGetBucketCorsRequest,
    BlockingGetBucketEncryptionRequest, BlockingGetBucketLifecycleRequest,
    BlockingGetBucketPublicAccessBlockRequest, BlockingGetBucketTaggingRequest,
    BlockingGetBucketVersioningRequest, BlockingHeadBucketRequest, BlockingListBucketsRequest,
    BlockingPutBucketConfigRawRequest, BlockingPutBucketCorsRequest,
    BlockingPutBucketEncryptionRequest, BlockingPutBucketLifecycleRequest,
    BlockingPutBucketPublicAccessBlockRequest, BlockingPutBucketTaggingRequest,
    BlockingPutBucketVersioningRequest,
};

#[cfg(feature = "blocking")]
pub use blocking_objects::{
    BlockingCopyObjectRequest, BlockingDeleteObjectRequest, BlockingDeleteObjectsRequest,
    BlockingGetObjectRequest, BlockingHeadObjectRequest, BlockingListObjectsV2Pager,
    BlockingListObjectsV2Request, BlockingObjectsService, BlockingPresignObjectRequest,
    BlockingPutObjectRequest,
};

#[cfg(all(feature = "blocking", feature = "multipart"))]
pub use blocking_objects::{
    BlockingAbortMultipartUploadRequest, BlockingCompleteMultipartUploadRequest,
    BlockingCreateMultipartUploadRequest, BlockingListPartsRequest, BlockingUploadPartCopyRequest,
    BlockingUploadPartRequest,
};
