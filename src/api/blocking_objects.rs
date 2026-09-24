//! Blocking object operations.

use std::io::Read;
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, StatusCode};

use super::blocking_common::{parse_blocking_xml_response, read_response_error};
#[cfg(test)]
use super::common::parse_xml_or_service_error;
use super::common::{
    ByteRange, ObjectConditions, PutObjectHeaders, apply_copy_metadata_headers,
    apply_metadata_headers, insert_header, insert_optional_header, next_list_v2_continuation_token,
    push_delete_object, push_metadata, validate_content_length_matches_body, validate_header_value,
    validate_max_keys, validate_query_token, validate_query_value, xml_body_headers,
};
#[cfg(feature = "multipart")]
use super::common::{
    prepare_completed_parts, push_completed_part, validate_max_parts, validate_part_number_marker,
    validate_upload_id, validate_upload_part_number,
};

use crate::{
    client::BlockingClient,
    error::{Error, Result},
    transport::blocking_transport::BlockingBody,
    types::{
        BlockingByteStream, BlockingGetObjectOutput, CopyObjectOutput, DeleteObjectIdentifier,
        DeleteObjectOutput, DeleteObjectsOutput, HeadObjectOutput, ListObjectsV2Output,
        PresignedRequest, PutObjectOutput,
    },
};

const MAX_ERROR_RESPONSE_BODY_BYTES: usize = 256 * 1024;

#[cfg(feature = "multipart")]
use crate::types::{
    AbortMultipartUploadOutput, CompleteMultipartUploadOutput, CompletedPart,
    CreateMultipartUploadOutput, ListPartsOutput, UploadPartCopyOutput, UploadPartOutput,
};

/// Object operations service (blocking).
///
/// Created by [`BlockingClient::objects`](crate::BlockingClient::objects).
///
/// Start here for common object flows:
///
/// - [`get`](crate::api::BlockingObjectsService::get) to download object bytes
/// - [`put`](crate::api::BlockingObjectsService::put) to upload bytes or readers
/// - [`list_v2`](crate::api::BlockingObjectsService::list_v2) to list object keys
/// - [`presign_get`](crate::api::BlockingObjectsService::presign_get) to build a presigned
///   download URL
#[derive(Clone)]
pub struct BlockingObjectsService {
    client: BlockingClient,
}

impl BlockingObjectsService {
    pub(crate) fn new(client: BlockingClient) -> Self {
        Self { client }
    }

    /// Starts a request to GET an object.
    pub fn get(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingGetObjectRequest {
        BlockingGetObjectRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            range: None,
            conditions: ObjectConditions::default(),
            if_modified_since: None,
            if_unmodified_since: None,
        }
    }

    /// Starts a request to HEAD an object.
    pub fn head(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingHeadObjectRequest {
        BlockingHeadObjectRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
        }
    }

    /// Starts a request to PUT an object.
    pub fn put(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingPutObjectRequest {
        BlockingPutObjectRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            headers: PutObjectHeaders::default(),
            content_length: None,
            #[cfg(feature = "checksums")]
            checksum: None,
            body: BlockingBody::Empty,
        }
    }

    /// Starts a request to DELETE an object.
    pub fn delete(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingDeleteObjectRequest {
        BlockingDeleteObjectRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
        }
    }

    /// Starts a request to DELETE multiple objects.
    pub fn delete_objects(&self, bucket: impl Into<String>) -> BlockingDeleteObjectsRequest {
        BlockingDeleteObjectsRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            objects: Vec::new(),
            quiet: false,
        }
    }

    /// Starts a request to copy an object.
    pub fn copy(
        &self,
        source_bucket: impl Into<String>,
        source_key: impl Into<String>,
        destination_bucket: impl Into<String>,
        destination_key: impl Into<String>,
    ) -> BlockingCopyObjectRequest {
        BlockingCopyObjectRequest {
            client: self.client.clone(),
            source_bucket: source_bucket.into(),
            source_key: source_key.into(),
            source_version_id: None,
            destination_bucket: destination_bucket.into(),
            destination_key: destination_key.into(),
            replace_metadata: false,
            metadata: Vec::new(),
            content_type: None,
        }
    }

    #[cfg(feature = "multipart")]
    /// Starts a multipart upload.
    pub fn create_multipart_upload(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingCreateMultipartUploadRequest {
        BlockingCreateMultipartUploadRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            content_type: None,
            metadata: Vec::new(),
        }
    }

    #[cfg(feature = "multipart")]
    /// Starts a request to upload a multipart part.
    pub fn upload_part(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
        upload_id: impl Into<String>,
        part_number: u32,
    ) -> BlockingUploadPartRequest {
        BlockingUploadPartRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            upload_id: upload_id.into(),
            part_number,
            body: BlockingBody::Empty,
        }
    }

    #[cfg(feature = "multipart")]
    /// Starts a request to copy data into a multipart part.
    pub fn upload_part_copy(
        &self,
        source_bucket: impl Into<String>,
        source_key: impl Into<String>,
        destination_bucket: impl Into<String>,
        destination_key: impl Into<String>,
        upload_id: impl Into<String>,
        part_number: u32,
    ) -> BlockingUploadPartCopyRequest {
        BlockingUploadPartCopyRequest {
            client: self.client.clone(),
            source_bucket: source_bucket.into(),
            source_key: source_key.into(),
            source_version_id: None,
            destination_bucket: destination_bucket.into(),
            destination_key: destination_key.into(),
            upload_id: upload_id.into(),
            part_number,
            copy_source_range: None,
        }
    }

    #[cfg(feature = "multipart")]
    /// Starts a request to complete a multipart upload.
    pub fn complete_multipart_upload(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
        upload_id: impl Into<String>,
    ) -> BlockingCompleteMultipartUploadRequest {
        BlockingCompleteMultipartUploadRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            upload_id: upload_id.into(),
            parts: Vec::new(),
        }
    }

    #[cfg(feature = "multipart")]
    /// Starts a request to abort a multipart upload.
    pub fn abort_multipart_upload(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
        upload_id: impl Into<String>,
    ) -> BlockingAbortMultipartUploadRequest {
        BlockingAbortMultipartUploadRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            upload_id: upload_id.into(),
        }
    }

    #[cfg(feature = "multipart")]
    /// Starts a request to list multipart parts.
    pub fn list_parts(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
        upload_id: impl Into<String>,
    ) -> BlockingListPartsRequest {
        BlockingListPartsRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            upload_id: upload_id.into(),
            max_parts: None,
            part_number_marker: None,
        }
    }

    /// Starts a ListObjectsV2 request.
    pub fn list_v2(&self, bucket: impl Into<String>) -> BlockingListObjectsV2Request {
        BlockingListObjectsV2Request {
            client: self.client.clone(),
            bucket: bucket.into(),
            prefix: None,
            delimiter: None,
            continuation_token: None,
            start_after: None,
            max_keys: None,
        }
    }

    /// Starts a generic presign request builder.
    pub fn presign(
        &self,
        method: Method,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingPresignObjectRequest {
        BlockingPresignObjectRequest {
            client: self.client.clone(),
            method,
            bucket: bucket.into(),
            key: key.into(),
            expires_in: Duration::from_secs(900),
            query_params: Vec::new(),
            headers: HeaderMap::new(),
            metadata: Vec::new(),
        }
    }

    /// Starts a presigned GET request builder.
    pub fn presign_get(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingPresignGetObjectRequest {
        BlockingPresignGetObjectRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            expires_in: Duration::from_secs(900),
            query_params: Vec::new(),
            headers: HeaderMap::new(),
            metadata: Vec::new(),
        }
    }

    /// Starts a presigned PUT request builder.
    pub fn presign_put(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingPresignPutObjectRequest {
        BlockingPresignPutObjectRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            expires_in: Duration::from_secs(900),
            query_params: Vec::new(),
            headers: HeaderMap::new(),
            metadata: Vec::new(),
        }
    }

    /// Starts a presigned HEAD request builder.
    pub fn presign_head(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingPresignHeadObjectRequest {
        BlockingPresignHeadObjectRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            expires_in: Duration::from_secs(900),
            query_params: Vec::new(),
            headers: HeaderMap::new(),
        }
    }

    /// Starts a presigned DELETE request builder.
    pub fn presign_delete(
        &self,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> BlockingPresignDeleteObjectRequest {
        BlockingPresignDeleteObjectRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            key: key.into(),
            expires_in: Duration::from_secs(900),
            query_params: Vec::new(),
            headers: HeaderMap::new(),
        }
    }
}

/// Request builder for fetching an object.
///
/// Created by [`BlockingObjectsService::get`](crate::api::BlockingObjectsService::get).
///
/// # Example
///
/// ```no_run
/// # fn demo() -> Result<(), s3::Error> {
/// use s3::{Auth, BlockingClient};
///
/// let client = BlockingClient::builder("https://s3.example.com")?
///     .region("us-east-1")
///     .auth(Auth::from_env()?)
///     .build()?;
///
/// let output = client
///     .objects()
///     .get("my-bucket", "logs/app.log")
///     .send()?;
/// let bytes = output.bytes()?;
/// # let _ = bytes;
/// # Ok(())
/// # }
/// ```
pub struct BlockingGetObjectRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    range: Option<ByteRange>,
    conditions: ObjectConditions,
    if_modified_since: Option<String>,
    if_unmodified_since: Option<String>,
}

impl BlockingGetObjectRequest {
    /// Sets an inclusive byte range.
    pub fn range_bytes(mut self, start: u64, end_inclusive: u64) -> Result<Self> {
        self.range = Some(ByteRange::new(start, end_inclusive)?);
        Ok(self)
    }

    /// Adds an If-Match condition.
    pub fn if_match(mut self, value: impl Into<String>) -> Result<Self> {
        self.conditions.set_if_match(value)?;
        Ok(self)
    }

    /// Adds an If-None-Match condition.
    pub fn if_none_match(mut self, value: impl Into<String>) -> Result<Self> {
        self.conditions.set_if_none_match(value)?;
        Ok(self)
    }

    /// Adds an If-Modified-Since condition.
    pub fn if_modified_since(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_header_value(&value, "invalid If-Modified-Since header")?;
        self.if_modified_since = Some(value);
        Ok(self)
    }

    /// Adds an If-Unmodified-Since condition.
    pub fn if_unmodified_since(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_header_value(&value, "invalid If-Unmodified-Since header")?;
        self.if_unmodified_since = Some(value);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<BlockingGetObjectOutput> {
        let mut headers = HeaderMap::new();
        if let Some(range) = self.range {
            headers.insert(
                http::header::RANGE,
                range.header_value("invalid Range header")?,
            );
        }
        self.conditions.apply(&mut headers)?;
        insert_optional_header(
            &mut headers,
            http::header::IF_MODIFIED_SINCE,
            self.if_modified_since,
            "invalid If-Modified-Since header",
        )?;
        insert_optional_header(
            &mut headers,
            http::header::IF_UNMODIFIED_SINCE,
            self.if_unmodified_since,
            "invalid If-Unmodified-Since header",
        )?;

        let resp = self.client.execute_stream(
            Method::GET,
            Some(&self.bucket),
            Some(&self.key),
            Vec::new(),
            headers,
            BlockingBody::Empty,
        )?;

        if !resp.status().is_success() {
            let resp = resp
                .into_response_limited(MAX_ERROR_RESPONSE_BODY_BYTES)
                .map_err(|e| Error::transport("failed to read response body", Some(Box::new(e))))?;
            return Err(crate::transport::response_error_from_body(
                resp.status(),
                resp.headers(),
                resp.body(),
            ));
        }

        let etag = crate::util::headers::header_string(resp.headers(), http::header::ETAG);
        let content_length =
            crate::util::headers::header_u64(resp.headers(), http::header::CONTENT_LENGTH);
        let content_type =
            crate::util::headers::header_string(resp.headers(), http::header::CONTENT_TYPE);

        Ok(BlockingGetObjectOutput {
            body: BlockingByteStream::new(resp),
            etag,
            content_length,
            content_type,
        })
    }
}

/// Request builder for fetching object metadata via HEAD.
pub struct BlockingHeadObjectRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
}

impl BlockingHeadObjectRequest {
    /// Sends the request.
    pub fn send(self) -> Result<HeadObjectOutput> {
        let resp = self.client.execute(
            Method::HEAD,
            Some(&self.bucket),
            Some(&self.key),
            Vec::new(),
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        Ok(HeadObjectOutput {
            etag: crate::util::headers::header_string(resp.headers(), http::header::ETAG),
            content_length: crate::util::headers::header_u64(
                resp.headers(),
                http::header::CONTENT_LENGTH,
            ),
            content_type: crate::util::headers::header_string(
                resp.headers(),
                http::header::CONTENT_TYPE,
            ),
        })
    }
}

/// Request builder for uploading an object.
///
/// Created by [`BlockingObjectsService::put`](crate::api::BlockingObjectsService::put).
///
/// # Example
///
/// ```no_run
/// # fn demo() -> Result<(), s3::Error> {
/// use s3::{Auth, BlockingClient};
///
/// let client = BlockingClient::builder("https://s3.example.com")?
///     .region("us-east-1")
///     .auth(Auth::from_env()?)
///     .build()?;
///
/// let output = client
///     .objects()
///     .put("my-bucket", "notes/hello.txt")
///     .content_type("text/plain; charset=utf-8")?
///     .body_bytes("hello from s3-rs")
///     .send()?;
/// # let _ = output;
/// # Ok(())
/// # }
/// ```
pub struct BlockingPutObjectRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    headers: PutObjectHeaders,
    content_length: Option<u64>,
    #[cfg(feature = "checksums")]
    checksum: Option<crate::types::Checksum>,
    body: BlockingBody,
}

impl BlockingPutObjectRequest {
    /// Sets the Content-Type header.
    pub fn content_type(mut self, value: impl Into<String>) -> Result<Self> {
        self.headers.content_type(value)?;
        Ok(self)
    }

    /// Sets the Cache-Control header.
    pub fn cache_control(mut self, value: impl Into<String>) -> Result<Self> {
        self.headers.cache_control(value)?;
        Ok(self)
    }

    /// Sets the Content-Disposition header.
    pub fn content_disposition(mut self, value: impl Into<String>) -> Result<Self> {
        self.headers.content_disposition(value)?;
        Ok(self)
    }

    /// Sets the Content-Encoding header.
    pub fn content_encoding(mut self, value: impl Into<String>) -> Result<Self> {
        self.headers.content_encoding(value)?;
        Ok(self)
    }

    /// Sets the Content-Language header.
    pub fn content_language(mut self, value: impl Into<String>) -> Result<Self> {
        self.headers.content_language(value)?;
        Ok(self)
    }

    /// Sets the Expires header.
    pub fn expires(mut self, value: impl Into<String>) -> Result<Self> {
        self.headers.expires(value)?;
        Ok(self)
    }

    /// Adds an If-Match precondition.
    pub fn if_match(mut self, value: impl Into<String>) -> Result<Self> {
        self.headers.if_match(value)?;
        Ok(self)
    }

    /// Adds an If-None-Match precondition.
    ///
    /// Use `*` for create-if-absent writes.
    pub fn if_none_match(mut self, value: impl Into<String>) -> Result<Self> {
        self.headers.if_none_match(value)?;
        Ok(self)
    }

    /// Sets the content length (required for reader bodies).
    pub fn content_length(mut self, value: u64) -> Self {
        self.content_length = Some(value);
        self
    }

    /// Adds a user metadata entry.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        self.headers.metadata(key, value)?;
        Ok(self)
    }

    #[cfg(feature = "checksums")]
    /// Sets a checksum to be sent with the upload.
    pub fn checksum(mut self, checksum: crate::types::Checksum) -> Self {
        self.checksum = Some(checksum);
        self
    }

    /// Sets the request body from bytes.
    pub fn body_bytes(mut self, body: impl Into<Bytes>) -> Self {
        self.body = BlockingBody::Bytes(body.into());
        self
    }

    /// Sets the request body from a reader.
    pub fn body_reader<R>(mut self, reader: R) -> Self
    where
        R: Read + Send + 'static,
    {
        self.body = BlockingBody::Reader {
            reader: Box::new(reader),
            content_length: None,
        };
        self
    }

    /// Sets a reader body with a known content length.
    pub fn body_reader_sized<R>(mut self, reader: R, content_length: u64) -> Self
    where
        R: Read + Send + 'static,
    {
        self.content_length = Some(content_length);
        self.body_reader(reader)
    }

    /// Sends the request.
    pub fn send(self) -> Result<PutObjectOutput> {
        let headers = self.headers.into_header_map()?;

        #[cfg(feature = "checksums")]
        let headers = {
            let mut headers = headers;
            if let Some(checksum) = self.checksum {
                checksum.apply(&mut headers)?;
            }
            headers
        };

        let body = match self.body {
            BlockingBody::Empty => {
                validate_content_length_matches_body(self.content_length, 0, "put_object")?;
                BlockingBody::Bytes(Bytes::new())
            }
            BlockingBody::Bytes(bytes) => {
                validate_content_length_matches_body(
                    self.content_length,
                    bytes.len(),
                    "put_object",
                )?;
                BlockingBody::Bytes(bytes)
            }
            BlockingBody::Reader { reader, .. } => {
                let content_length = self
                    .content_length
                    .ok_or_else(|| Error::invalid_config("reader put requires content_length"))?;
                BlockingBody::Reader {
                    reader,
                    content_length: Some(content_length),
                }
            }
        };

        let resp = self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            Some(&self.key),
            Vec::new(),
            headers,
            body,
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        Ok(PutObjectOutput {
            etag: crate::util::headers::header_string(resp.headers(), http::header::ETAG),
        })
    }
}

/// Request builder for deleting a single object.
pub struct BlockingDeleteObjectRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
}

impl BlockingDeleteObjectRequest {
    /// Sends the request.
    pub fn send(self) -> Result<DeleteObjectOutput> {
        let resp = self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            Some(&self.key),
            Vec::new(),
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        let status = resp.status();
        if status == StatusCode::NO_CONTENT || status.is_success() {
            return Ok(DeleteObjectOutput);
        }
        Err(read_response_error(resp)?)
    }
}

/// Request builder for deleting multiple objects.
pub struct BlockingDeleteObjectsRequest {
    client: BlockingClient,
    bucket: String,
    objects: Vec<DeleteObjectIdentifier>,
    quiet: bool,
}

impl BlockingDeleteObjectsRequest {
    /// Adds an object key to delete.
    pub fn object(mut self, key: impl Into<String>) -> Result<Self> {
        push_delete_object(&mut self.objects, DeleteObjectIdentifier::new(key)?)?;
        Ok(self)
    }

    /// Adds an object key and version id to delete.
    pub fn object_with_version(
        mut self,
        key: impl Into<String>,
        version_id: impl Into<String>,
    ) -> Result<Self> {
        let object = DeleteObjectIdentifier::new(key)?.with_version_id(version_id)?;
        push_delete_object(&mut self.objects, object)?;
        Ok(self)
    }

    /// Adds multiple object keys to delete.
    pub fn objects<I, S>(mut self, iter: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for key in iter {
            push_delete_object(&mut self.objects, DeleteObjectIdentifier::new(key)?)?;
        }
        Ok(self)
    }

    /// Toggles quiet response mode.
    pub fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Sends the request.
    pub fn send(self) -> Result<DeleteObjectsOutput> {
        let body = crate::util::xml::encode_delete_objects(&self.objects, self.quiet)?;
        let headers = xml_body_headers(body.as_ref())?;

        let resp = self.client.execute(
            Method::POST,
            Some(&self.bucket),
            None,
            vec![("delete".to_string(), String::new())],
            headers,
            BlockingBody::Bytes(body),
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        parse_blocking_xml_response(resp, crate::util::xml::parse_delete_objects)
    }
}

/// Request builder for copying an object.
pub struct BlockingCopyObjectRequest {
    client: BlockingClient,
    source_bucket: String,
    source_key: String,
    source_version_id: Option<String>,
    destination_bucket: String,
    destination_key: String,
    replace_metadata: bool,
    metadata: Vec<(String, String)>,
    content_type: Option<String>,
}

impl BlockingCopyObjectRequest {
    /// Sets a source version id to copy.
    pub fn source_version_id(mut self, version_id: impl Into<String>) -> Result<Self> {
        let version_id = version_id.into();
        crate::util::validation::validate_version_id(&version_id)?;
        self.source_version_id = Some(version_id);
        Ok(self)
    }

    /// Replaces metadata on the destination object.
    pub fn replace_metadata(mut self) -> Self {
        self.replace_metadata = true;
        self
    }

    /// Adds a user metadata entry.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        push_metadata(&mut self.metadata, key, value)?;
        Ok(self)
    }

    /// Sets the Content-Type for the destination object.
    pub fn content_type(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_header_value(&value, "invalid Content-Type header")?;
        self.content_type = Some(value);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<CopyObjectOutput> {
        let mut headers = HeaderMap::new();

        let copy_source = crate::util::headers::copy_source_header_value(
            &self.source_bucket,
            &self.source_key,
            self.source_version_id.as_deref(),
        )?;
        insert_header(
            &mut headers,
            "x-amz-copy-source",
            copy_source,
            "invalid x-amz-copy-source header",
        )?;

        apply_copy_metadata_headers(
            &mut headers,
            self.replace_metadata,
            self.content_type,
            self.metadata,
        )?;

        let resp = self.client.execute(
            Method::PUT,
            Some(&self.destination_bucket),
            Some(&self.destination_key),
            Vec::new(),
            headers,
            BlockingBody::Empty,
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        parse_blocking_xml_response(resp, crate::util::xml::parse_copy_object)
    }
}

#[cfg(feature = "multipart")]
/// Request builder for initiating a multipart upload.
pub struct BlockingCreateMultipartUploadRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    content_type: Option<String>,
    metadata: Vec<(String, String)>,
}

#[cfg(feature = "multipart")]
impl BlockingCreateMultipartUploadRequest {
    /// Sets the Content-Type header.
    pub fn content_type(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_header_value(&value, "invalid Content-Type header")?;
        self.content_type = Some(value);
        Ok(self)
    }

    /// Adds a user metadata entry.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        push_metadata(&mut self.metadata, key, value)?;
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<CreateMultipartUploadOutput> {
        let mut headers = HeaderMap::new();
        insert_optional_header(
            &mut headers,
            http::header::CONTENT_TYPE,
            self.content_type,
            "invalid Content-Type header",
        )?;

        apply_metadata_headers(&mut headers, self.metadata)?;

        let resp = self.client.execute(
            Method::POST,
            Some(&self.bucket),
            Some(&self.key),
            vec![("uploads".to_string(), String::new())],
            headers,
            BlockingBody::Empty,
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        parse_blocking_xml_response(resp, crate::util::xml::parse_create_multipart_upload)
    }
}

#[cfg(feature = "multipart")]
/// Request builder for uploading a multipart part.
pub struct BlockingUploadPartRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    upload_id: String,
    part_number: u32,
    body: BlockingBody,
}

#[cfg(feature = "multipart")]
impl BlockingUploadPartRequest {
    /// Sets the request body from bytes.
    pub fn body_bytes(mut self, body: impl Into<Bytes>) -> Self {
        self.body = BlockingBody::Bytes(body.into());
        self
    }

    /// Sets a reader request body with a known content length.
    pub fn body_reader_sized<R>(mut self, reader: R, content_length: u64) -> Self
    where
        R: Read + Send + 'static,
    {
        self.body = BlockingBody::Reader {
            reader: Box::new(reader),
            content_length: Some(content_length),
        };
        self
    }

    /// Sends the request.
    pub fn send(self) -> Result<UploadPartOutput> {
        validate_upload_part_body(&self.body)?;
        validate_upload_part_number(self.part_number)?;
        validate_upload_id(&self.upload_id)?;

        let query = vec![
            ("partNumber".to_string(), self.part_number.to_string()),
            ("uploadId".to_string(), self.upload_id),
        ];

        let resp = self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            Some(&self.key),
            query,
            HeaderMap::new(),
            self.body,
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        Ok(UploadPartOutput {
            etag: crate::util::headers::header_string(resp.headers(), http::header::ETAG),
        })
    }
}

#[cfg(feature = "multipart")]
fn validate_upload_part_body(body: &BlockingBody) -> Result<()> {
    match body {
        BlockingBody::Empty => {
            return Err(Error::invalid_config("upload_part requires a request body"));
        }
        BlockingBody::Reader {
            content_length: None,
            ..
        } => {
            return Err(Error::invalid_config(
                "reader upload_part requires content_length",
            ));
        }
        BlockingBody::Bytes(_)
        | BlockingBody::Reader {
            content_length: Some(_),
            ..
        } => {}
    }
    Ok(())
}

#[cfg(feature = "multipart")]
/// Request builder for uploading a copied multipart part.
pub struct BlockingUploadPartCopyRequest {
    client: BlockingClient,
    source_bucket: String,
    source_key: String,
    source_version_id: Option<String>,
    destination_bucket: String,
    destination_key: String,
    upload_id: String,
    part_number: u32,
    copy_source_range: Option<ByteRange>,
}

#[cfg(feature = "multipart")]
impl BlockingUploadPartCopyRequest {
    /// Sets the source version id to copy.
    pub fn source_version_id(mut self, version_id: impl Into<String>) -> Result<Self> {
        let version_id = version_id.into();
        crate::util::validation::validate_version_id(&version_id)?;
        self.source_version_id = Some(version_id);
        Ok(self)
    }

    /// Sets a byte range for the copy source.
    pub fn copy_source_range_bytes(mut self, start: u64, end_inclusive: u64) -> Result<Self> {
        self.copy_source_range = Some(ByteRange::new(start, end_inclusive)?);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<UploadPartCopyOutput> {
        validate_upload_part_number(self.part_number)?;
        validate_upload_id(&self.upload_id)?;

        let mut headers = HeaderMap::new();

        let copy_source = crate::util::headers::copy_source_header_value(
            &self.source_bucket,
            &self.source_key,
            self.source_version_id.as_deref(),
        )?;
        insert_header(
            &mut headers,
            "x-amz-copy-source",
            copy_source,
            "invalid x-amz-copy-source header",
        )?;

        if let Some(range) = self.copy_source_range {
            headers.insert(
                "x-amz-copy-source-range",
                range.header_value("invalid x-amz-copy-source-range header")?,
            );
        }

        let query = vec![
            ("partNumber".to_string(), self.part_number.to_string()),
            ("uploadId".to_string(), self.upload_id),
        ];

        let resp = self.client.execute(
            Method::PUT,
            Some(&self.destination_bucket),
            Some(&self.destination_key),
            query,
            headers,
            BlockingBody::Empty,
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        parse_blocking_xml_response(resp, crate::util::xml::parse_upload_part_copy)
    }
}

#[cfg(feature = "multipart")]
/// Request builder for completing a multipart upload.
pub struct BlockingCompleteMultipartUploadRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    upload_id: String,
    parts: Vec<CompletedPart>,
}

#[cfg(feature = "multipart")]
impl BlockingCompleteMultipartUploadRequest {
    /// Adds a completed part by number and etag.
    pub fn part(mut self, part_number: u32, etag: impl Into<String>) -> Result<Self> {
        push_completed_part(&mut self.parts, CompletedPart::new(part_number, etag)?)?;
        Ok(self)
    }

    /// Adds multiple completed parts.
    pub fn parts<I>(mut self, iter: I) -> Result<Self>
    where
        I: IntoIterator<Item = CompletedPart>,
    {
        for part in iter {
            push_completed_part(&mut self.parts, part)?;
        }
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<CompleteMultipartUploadOutput> {
        validate_upload_id(&self.upload_id)?;
        let parts = prepare_completed_parts(self.parts)?;
        let body = crate::util::xml::encode_complete_multipart_upload(&parts)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/xml"),
        );

        let resp = self.client.execute(
            Method::POST,
            Some(&self.bucket),
            Some(&self.key),
            vec![("uploadId".to_string(), self.upload_id)],
            headers,
            BlockingBody::Bytes(body),
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        parse_blocking_xml_response(resp, crate::util::xml::parse_complete_multipart_upload)
    }
}

#[cfg(feature = "multipart")]
/// Request builder for aborting a multipart upload.
pub struct BlockingAbortMultipartUploadRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    upload_id: String,
}

#[cfg(feature = "multipart")]
impl BlockingAbortMultipartUploadRequest {
    /// Sends the request.
    pub fn send(self) -> Result<AbortMultipartUploadOutput> {
        validate_upload_id(&self.upload_id)?;

        let resp = self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            Some(&self.key),
            vec![("uploadId".to_string(), self.upload_id)],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        if resp.status() == StatusCode::NO_CONTENT || resp.status().is_success() {
            return Ok(AbortMultipartUploadOutput);
        }
        Err(read_response_error(resp)?)
    }
}

#[cfg(feature = "multipart")]
/// Request builder for listing multipart parts.
pub struct BlockingListPartsRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    upload_id: String,
    max_parts: Option<u32>,
    part_number_marker: Option<u32>,
}

#[cfg(feature = "multipart")]
impl BlockingListPartsRequest {
    /// Sets the maximum number of parts to return.
    pub fn max_parts(mut self, value: u32) -> Result<Self> {
        validate_max_parts(value)?;
        self.max_parts = Some(value);
        Ok(self)
    }

    /// Sets the part number marker for pagination.
    pub fn part_number_marker(mut self, value: u32) -> Result<Self> {
        validate_part_number_marker(value)?;
        self.part_number_marker = Some(value);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<ListPartsOutput> {
        validate_upload_id(&self.upload_id)?;

        let mut query = vec![("uploadId".to_string(), self.upload_id)];
        if let Some(v) = self.max_parts {
            validate_max_parts(v)?;
            query.push(("max-parts".to_string(), v.to_string()));
        }
        if let Some(v) = self.part_number_marker {
            validate_part_number_marker(v)?;
            query.push(("part-number-marker".to_string(), v.to_string()));
        }

        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            Some(&self.key),
            query,
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        parse_blocking_xml_response(resp, crate::util::xml::parse_list_parts)
    }
}

/// Request builder for ListObjectsV2.
///
/// Created by [`BlockingObjectsService::list_v2`](crate::api::BlockingObjectsService::list_v2).
///
/// # Example
///
/// ```no_run
/// # fn demo() -> Result<(), s3::Error> {
/// use s3::{Auth, BlockingClient};
///
/// let client = BlockingClient::builder("https://s3.example.com")?
///     .region("us-east-1")
///     .auth(Auth::from_env()?)
///     .build()?;
///
/// let page = client
///     .objects()
///     .list_v2("my-bucket")
///     .prefix("logs/")?
///     .max_keys(100)?
///     .send()?;
/// # let _ = page;
/// # Ok(())
/// # }
/// ```
pub struct BlockingListObjectsV2Request {
    client: BlockingClient,
    bucket: String,
    prefix: Option<String>,
    delimiter: Option<String>,
    continuation_token: Option<String>,
    start_after: Option<String>,
    max_keys: Option<u32>,
}

impl BlockingListObjectsV2Request {
    /// Filters by key prefix.
    pub fn prefix(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_query_value("prefix", &value)?;
        self.prefix = Some(value);
        Ok(self)
    }

    /// Groups keys by delimiter.
    pub fn delimiter(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_query_value("delimiter", &value)?;
        self.delimiter = Some(value);
        Ok(self)
    }

    /// Sets the continuation token for pagination.
    pub fn continuation_token(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_query_token("continuation_token", &value)?;
        self.continuation_token = Some(value);
        Ok(self)
    }

    /// Starts listing after the given key.
    pub fn start_after(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        crate::util::validation::validate_object_key(&value)?;
        self.start_after = Some(value);
        Ok(self)
    }

    /// Sets the maximum number of keys to return.
    pub fn max_keys(mut self, value: u32) -> Result<Self> {
        validate_max_keys(value)?;
        self.max_keys = Some(value);
        Ok(self)
    }

    /// Converts this request into a pager.
    pub fn pager(self) -> BlockingListObjectsV2Pager {
        BlockingListObjectsV2Pager {
            client: self.client,
            bucket: self.bucket,
            prefix: self.prefix,
            delimiter: self.delimiter,
            continuation_token: self.continuation_token,
            start_after: self.start_after,
            max_keys: self.max_keys,
            done: false,
        }
    }

    /// Sends the request.
    pub fn send(self) -> Result<ListObjectsV2Output> {
        let mut query = Vec::new();
        query.push(("list-type".to_string(), "2".to_string()));
        if let Some(v) = self.prefix {
            validate_query_value("prefix", &v)?;
            query.push(("prefix".to_string(), v));
        }
        if let Some(v) = self.delimiter {
            validate_query_value("delimiter", &v)?;
            query.push(("delimiter".to_string(), v));
        }
        if let Some(v) = self.continuation_token {
            validate_query_token("continuation_token", &v)?;
            query.push(("continuation-token".to_string(), v));
        }
        if let Some(v) = self.start_after {
            crate::util::url::validate_object_key(&v)?;
            query.push(("start-after".to_string(), v));
        }
        if let Some(v) = self.max_keys {
            validate_max_keys(v)?;
            query.push(("max-keys".to_string(), v.to_string()));
        }

        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            None,
            query,
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        if !resp.status().is_success() {
            return Err(read_response_error(resp)?);
        }

        parse_blocking_xml_response(resp, crate::util::xml::parse_list_objects_v2)
    }
}

/// Pager for ListObjectsV2 responses.
pub struct BlockingListObjectsV2Pager {
    client: BlockingClient,
    bucket: String,
    prefix: Option<String>,
    delimiter: Option<String>,
    continuation_token: Option<String>,
    start_after: Option<String>,
    max_keys: Option<u32>,
    done: bool,
}

impl Iterator for BlockingListObjectsV2Pager {
    type Item = Result<ListObjectsV2Output>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let start_after = if self.continuation_token.is_some() {
            None
        } else {
            self.start_after.clone()
        };

        let page = BlockingListObjectsV2Request {
            client: self.client.clone(),
            bucket: self.bucket.clone(),
            prefix: self.prefix.clone(),
            delimiter: self.delimiter.clone(),
            continuation_token: self.continuation_token.clone(),
            start_after,
            max_keys: self.max_keys,
        }
        .send();

        match page {
            Ok(page) => {
                let next = next_list_v2_continuation_token(
                    self.continuation_token.as_deref(),
                    page.next_continuation_token.as_deref(),
                    page.is_truncated,
                );
                match next {
                    Ok(next) => {
                        self.continuation_token = next;
                        if self.continuation_token.is_none() {
                            self.done = true;
                        }
                        Some(Ok(page))
                    }
                    Err(err) => {
                        self.done = true;
                        Some(Err(err))
                    }
                }
            }
            Err(err) => {
                self.done = true;
                Some(Err(err))
            }
        }
    }
}

impl BlockingListObjectsV2Pager {
    /// Returns the configured maximum number of keys per request.
    ///
    /// Returns `None` if [`BlockingListObjectsV2Request::max_keys`] was not set. This
    /// limit does not indicate how many pages remain.
    pub fn max_keys(&self) -> Option<u32> {
        self.max_keys
    }
}

/// Request builder for presigned requests with a custom method.
pub struct BlockingPresignObjectRequest {
    client: BlockingClient,
    method: Method,
    bucket: String,
    key: String,
    expires_in: Duration,
    query_params: Vec<(String, String)>,
    headers: HeaderMap,
    metadata: Vec<(String, String)>,
}

impl BlockingPresignObjectRequest {
    /// Sets the expiry duration.
    pub fn expires_in(mut self, duration: Duration) -> Result<Self> {
        crate::util::signing::validate_presign_expires(duration)?;
        self.expires_in = duration;
        Ok(self)
    }

    /// Adds a query parameter to the presigned URL.
    pub fn query_param(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into();
        let value = value.into();
        crate::util::signing::validate_presign_query_param(&name, &value)?;
        self.query_params.push((name, value));
        Ok(self)
    }

    /// Adds an HTTP header to sign.
    pub fn header(mut self, name: http::header::HeaderName, value: HeaderValue) -> Result<Self> {
        crate::util::signing::validate_presign_header(&name, &value)?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Adds a user metadata entry to sign.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        push_metadata(&mut self.metadata, key, value)?;
        Ok(self)
    }

    /// Builds the presigned request.
    pub fn build(self) -> Result<PresignedRequest> {
        let mut headers = self.headers;
        apply_metadata_headers(&mut headers, self.metadata)?;

        self.client.presign(
            self.method,
            &self.bucket,
            &self.key,
            self.expires_in,
            self.query_params,
            headers,
        )
    }
}

/// Request builder for presigned GET requests.
///
/// Created by [`BlockingObjectsService::presign_get`](crate::api::BlockingObjectsService::presign_get).
///
/// # Example
///
/// ```no_run
/// # fn demo() -> Result<(), s3::Error> {
/// use std::time::Duration;
///
/// use s3::{Auth, BlockingClient};
///
/// let client = BlockingClient::builder("https://s3.example.com")?
///     .region("us-east-1")
///     .auth(Auth::from_env()?)
///     .build()?;
///
/// let presigned = client
///     .objects()
///     .presign_get("my-bucket", "reports/q1.csv")
///     .expires_in(Duration::from_secs(300))?
///     .build()?;
/// # let _ = presigned;
/// # Ok(())
/// # }
/// ```
pub struct BlockingPresignGetObjectRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    expires_in: Duration,
    query_params: Vec<(String, String)>,
    headers: HeaderMap,
    metadata: Vec<(String, String)>,
}

impl BlockingPresignGetObjectRequest {
    /// Sets the expiry duration.
    pub fn expires_in(mut self, duration: Duration) -> Result<Self> {
        crate::util::signing::validate_presign_expires(duration)?;
        self.expires_in = duration;
        Ok(self)
    }

    /// Adds a query parameter to the presigned URL.
    pub fn query_param(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into();
        let value = value.into();
        crate::util::signing::validate_presign_query_param(&name, &value)?;
        self.query_params.push((name, value));
        Ok(self)
    }

    /// Adds an HTTP header to sign.
    pub fn header(mut self, name: http::header::HeaderName, value: HeaderValue) -> Result<Self> {
        crate::util::signing::validate_presign_header(&name, &value)?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Adds a user metadata entry to sign.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        push_metadata(&mut self.metadata, key, value)?;
        Ok(self)
    }

    /// Builds the presigned request.
    pub fn build(self) -> Result<PresignedRequest> {
        let mut headers = self.headers;
        apply_metadata_headers(&mut headers, self.metadata)?;

        self.client.presign(
            Method::GET,
            &self.bucket,
            &self.key,
            self.expires_in,
            self.query_params,
            headers,
        )
    }
}

/// Request builder for presigned PUT requests.
pub struct BlockingPresignPutObjectRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    expires_in: Duration,
    query_params: Vec<(String, String)>,
    headers: HeaderMap,
    metadata: Vec<(String, String)>,
}

impl BlockingPresignPutObjectRequest {
    /// Sets the expiry duration.
    pub fn expires_in(mut self, duration: Duration) -> Result<Self> {
        crate::util::signing::validate_presign_expires(duration)?;
        self.expires_in = duration;
        Ok(self)
    }

    /// Adds a query parameter to the presigned URL.
    pub fn query_param(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into();
        let value = value.into();
        crate::util::signing::validate_presign_query_param(&name, &value)?;
        self.query_params.push((name, value));
        Ok(self)
    }

    /// Adds an HTTP header to sign.
    pub fn header(mut self, name: http::header::HeaderName, value: HeaderValue) -> Result<Self> {
        crate::util::signing::validate_presign_header(&name, &value)?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Adds a user metadata entry to sign.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        push_metadata(&mut self.metadata, key, value)?;
        Ok(self)
    }

    /// Builds the presigned request.
    pub fn build(self) -> Result<PresignedRequest> {
        let mut headers = self.headers;
        apply_metadata_headers(&mut headers, self.metadata)?;

        self.client.presign(
            Method::PUT,
            &self.bucket,
            &self.key,
            self.expires_in,
            self.query_params,
            headers,
        )
    }
}

/// Request builder for presigned HEAD requests.
pub struct BlockingPresignHeadObjectRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    expires_in: Duration,
    query_params: Vec<(String, String)>,
    headers: HeaderMap,
}

impl BlockingPresignHeadObjectRequest {
    /// Sets the expiry duration.
    pub fn expires_in(mut self, duration: Duration) -> Result<Self> {
        crate::util::signing::validate_presign_expires(duration)?;
        self.expires_in = duration;
        Ok(self)
    }

    /// Adds a query parameter to the presigned URL.
    pub fn query_param(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into();
        let value = value.into();
        crate::util::signing::validate_presign_query_param(&name, &value)?;
        self.query_params.push((name, value));
        Ok(self)
    }

    /// Adds an HTTP header to sign.
    pub fn header(mut self, name: http::header::HeaderName, value: HeaderValue) -> Result<Self> {
        crate::util::signing::validate_presign_header(&name, &value)?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Builds the presigned request.
    pub fn build(self) -> Result<PresignedRequest> {
        self.client.presign(
            Method::HEAD,
            &self.bucket,
            &self.key,
            self.expires_in,
            self.query_params,
            self.headers,
        )
    }
}

/// Request builder for presigned DELETE requests.
pub struct BlockingPresignDeleteObjectRequest {
    client: BlockingClient,
    bucket: String,
    key: String,
    expires_in: Duration,
    query_params: Vec<(String, String)>,
    headers: HeaderMap,
}

impl BlockingPresignDeleteObjectRequest {
    /// Sets the expiry duration.
    pub fn expires_in(mut self, duration: Duration) -> Result<Self> {
        crate::util::signing::validate_presign_expires(duration)?;
        self.expires_in = duration;
        Ok(self)
    }

    /// Adds a query parameter to the presigned URL.
    pub fn query_param(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into();
        let value = value.into();
        crate::util::signing::validate_presign_query_param(&name, &value)?;
        self.query_params.push((name, value));
        Ok(self)
    }

    /// Adds an HTTP header to sign.
    pub fn header(mut self, name: http::header::HeaderName, value: HeaderValue) -> Result<Self> {
        crate::util::signing::validate_presign_header(&name, &value)?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Builds the presigned request.
    pub fn build(self) -> Result<PresignedRequest> {
        self.client.presign(
            Method::DELETE,
            &self.bucket,
            &self.key,
            self.expires_in,
            self.query_params,
            self.headers,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> BlockingClient {
        BlockingClient::builder("https://s3.example.com")
            .expect("builder should parse")
            .region("us-east-1")
            .auth(crate::Auth::Anonymous)
            .build()
            .expect("client should build")
    }

    fn assert_invalid_config<T>(result: Result<T>, expected: &str) {
        match result {
            Err(Error::InvalidConfig { message }) => assert!(
                message.contains(expected),
                "expected {message:?} to contain {expected:?}"
            ),
            Err(other) => panic!("expected InvalidConfig, got {other:?}"),
            Ok(_) => panic!("expected InvalidConfig"),
        }
    }

    #[test]
    fn parse_xml_or_service_error_maps_error_xml_as_api_error() {
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-request-id", HeaderValue::from_static("req-1"));
        let body = r#"
<Error>
  <Code>InternalError</Code>
  <Message>backend failure</Message>
</Error>
"#;

        let err = parse_xml_or_service_error::<()>(StatusCode::OK, &headers, body, |_xml| {
            Err(Error::decode("failed to parse success XML", None))
        })
        .expect_err("expected service error mapping");

        match err {
            Error::Api {
                status,
                code,
                message,
                request_id,
                ..
            } => {
                assert_eq!(status, StatusCode::OK);
                assert_eq!(code.as_deref(), Some("InternalError"));
                assert_eq!(message.as_deref(), Some("backend failure"));
                assert_eq!(request_id.as_deref(), Some("req-1"));
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[test]
    fn parse_xml_or_service_error_preserves_decode_error_for_plain_body() {
        let err = parse_xml_or_service_error::<()>(
            StatusCode::OK,
            &HeaderMap::new(),
            "not-xml",
            |_xml| Err(Error::decode("failed to parse success XML", None)),
        )
        .expect_err("expected parse failure");

        match err {
            Error::Decode { .. } => {}
            other => panic!("expected Decode error, got {other:?}"),
        }
    }

    #[test]
    fn list_v2_setters_reject_invalid_values() {
        let objects = test_client().objects();

        assert_invalid_config(objects.list_v2("bucket").prefix(""), "prefix");
        assert_invalid_config(objects.list_v2("bucket").delimiter(""), "delimiter");
        assert_invalid_config(
            objects.list_v2("bucket").continuation_token(" token"),
            "continuation_token",
        );
        assert_invalid_config(
            objects.list_v2("bucket").start_after("a/../b"),
            "object key",
        );
        assert_invalid_config(objects.list_v2("bucket").max_keys(0), "max_keys");
    }

    #[test]
    fn delete_objects_setters_reject_oversized_batches() {
        let objects = test_client().objects();
        let keys =
            (0..=crate::types::MAX_DELETE_OBJECTS_PER_REQUEST).map(|idx| format!("key-{idx}"));

        assert_invalid_config(
            objects.delete_objects("bucket").objects(keys),
            "at most 1000",
        );
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn complete_multipart_setters_reject_duplicate_parts() {
        let objects = test_client().objects();

        assert_invalid_config(
            objects
                .complete_multipart_upload("bucket", "key", "upload-id")
                .part(1, "\"etag-1\"")
                .unwrap()
                .part(1, "\"etag-duplicate\""),
            "unique",
        );
        assert_invalid_config(
            objects
                .complete_multipart_upload("bucket", "key", "upload-id")
                .parts(vec![
                    CompletedPart::new(1, "\"etag-1\"").unwrap(),
                    CompletedPart::new(1, "\"etag-duplicate\"").unwrap(),
                ]),
            "unique",
        );
    }

    #[test]
    fn copy_source_version_id_setters_reject_invalid_values() {
        let objects = test_client().objects();

        assert_invalid_config(
            objects
                .copy("source-bucket", "source-key", "bucket", "key")
                .source_version_id(" version"),
            "version_id",
        );

        #[cfg(feature = "multipart")]
        assert_invalid_config(
            objects
                .upload_part_copy(
                    "source-bucket",
                    "source-key",
                    "bucket",
                    "key",
                    "upload-id",
                    1,
                )
                .source_version_id("version "),
            "version_id",
        );
    }

    #[test]
    fn presign_setters_reject_invalid_values() {
        let objects = test_client().objects();

        assert_invalid_config(
            objects
                .presign_get("bucket", "key")
                .expires_in(Duration::ZERO),
            "expires_in",
        );
        assert_invalid_config(
            objects
                .presign_get("bucket", "key")
                .query_param(" x", "value"),
            "query parameter",
        );
        assert_invalid_config(
            objects
                .presign_get("bucket", "key")
                .query_param("x-amz-user", "value"),
            "reserved",
        );
        assert_invalid_config(
            objects
                .presign_get("bucket", "key")
                .header(http::header::HOST, HeaderValue::from_static("example.com")),
            "SigV4-managed",
        );
        assert_invalid_config(
            objects.presign_get("bucket", "key").header(
                http::header::HeaderName::from_static("x-amz-meta-bin"),
                HeaderValue::from_bytes(&[0xff]).unwrap(),
            ),
            "header",
        );
    }

    #[test]
    fn header_and_metadata_setters_reject_invalid_values() {
        let objects = test_client().objects();

        assert_invalid_config(
            objects.get("bucket", "key").if_match(" \"etag\""),
            "If-Match",
        );
        assert_invalid_config(
            objects.get("bucket", "key").if_none_match(""),
            "If-None-Match",
        );
        assert_invalid_config(
            objects.get("bucket", "key").if_modified_since(" date"),
            "If-Modified-Since",
        );
        assert_invalid_config(
            objects.get("bucket", "key").if_unmodified_since("date "),
            "If-Unmodified-Since",
        );
        assert_invalid_config(
            objects.put("bucket", "key").content_type(" text/plain"),
            "Content-Type",
        );
        assert_invalid_config(
            objects.put("bucket", "key").if_match(" \"etag\""),
            "If-Match",
        );
        assert_invalid_config(
            objects.put("bucket", "key").if_none_match(""),
            "If-None-Match",
        );
        assert_invalid_config(
            objects.put("bucket", "key").metadata("", "value"),
            "metadata key",
        );
        assert_invalid_config(
            objects
                .put("bucket", "key")
                .metadata("Trace", "a")
                .unwrap()
                .metadata("trace", "b"),
            "unique",
        );
        assert_invalid_config(
            objects
                .copy("source-bucket", "source-key", "bucket", "key")
                .content_type(""),
            "Content-Type",
        );

        #[cfg(feature = "multipart")]
        assert_invalid_config(
            objects
                .create_multipart_upload("bucket", "key")
                .metadata("bad key", "value"),
            "metadata key",
        );
    }

    #[test]
    fn put_object_request_applies_conditional_headers() {
        let request = test_client()
            .objects()
            .put("bucket", "locks/my-lock")
            .if_none_match("*")
            .expect("If-None-Match wildcard should be valid")
            .if_match("\"etag\"")
            .expect("If-Match should be valid");

        let headers = request
            .headers
            .into_header_map()
            .expect("headers should be valid");

        assert_eq!(
            headers
                .get(http::header::IF_NONE_MATCH)
                .and_then(|value| value.to_str().ok()),
            Some("*")
        );
        assert_eq!(
            headers
                .get(http::header::IF_MATCH)
                .and_then(|value| value.to_str().ok()),
            Some("\"etag\"")
        );
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn list_parts_setters_reject_invalid_values() {
        let objects = test_client().objects();

        assert_invalid_config(
            objects
                .list_parts("bucket", "key", "upload-id")
                .max_parts(0),
            "max_parts",
        );
        assert_invalid_config(
            objects
                .list_parts("bucket", "key", "upload-id")
                .part_number_marker(0),
            "part_number_marker",
        );
    }

    #[test]
    fn blocking_put_reader_requires_content_length() {
        let client = BlockingClient::builder("https://s3.example.com")
            .expect("builder should parse")
            .region("us-east-1")
            .auth(crate::Auth::Anonymous)
            .build()
            .expect("client should build");

        let err = client
            .objects()
            .put("bucket", "key")
            .body_reader(std::io::Cursor::new(b"payload".to_vec()))
            .send()
            .expect_err("reader put without content length must fail before transport");

        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("content_length"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[test]
    fn blocking_put_bytes_rejects_mismatched_content_length() {
        let client = BlockingClient::builder("https://s3.example.com")
            .expect("builder should parse")
            .region("us-east-1")
            .auth(crate::Auth::Anonymous)
            .build()
            .expect("client should build");

        let err = client
            .objects()
            .put("bucket", "key")
            .content_length(4)
            .body_bytes("abc")
            .send()
            .expect_err("mismatched byte body content length must fail before transport");

        match err {
            Error::InvalidConfig { message } => assert!(message.contains("content_length")),
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn validate_upload_part_body_rejects_empty() {
        let err =
            validate_upload_part_body(&BlockingBody::Empty).expect_err("expected invalid body");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("upload_part requires a request body"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn validate_upload_part_number_rejects_out_of_range() {
        let err = validate_upload_part_number(0).expect_err("expected invalid part_number");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("part_number"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }

        let err = validate_upload_part_number(10_001).expect_err("expected invalid part_number");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("part_number"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn validate_upload_id_rejects_empty() {
        let err = validate_upload_id("  ").expect_err("expected invalid upload_id");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("upload_id"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn validate_max_parts_rejects_out_of_range() {
        let err = validate_max_parts(0).expect_err("expected invalid max_parts");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("max_parts"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }

        let err = validate_max_parts(1_001).expect_err("expected invalid max_parts");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("max_parts"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[test]
    fn validate_max_keys_rejects_out_of_range() {
        let err = validate_max_keys(0).expect_err("expected invalid max_keys");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("max_keys"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }

        let err = validate_max_keys(1_001).expect_err("expected invalid max_keys");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("max_keys"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }
}
