//! Blocking bucket operations.

use bytes::Bytes;
use http::{HeaderMap, Method};

use super::common::{
    create_bucket_location_constraint, parse_xml_response, require_configured,
    validate_subresource, xml_body_headers,
};

use crate::{
    client::BlockingClient,
    error::{Error, Result},
    transport::blocking_transport::BlockingBody,
    types::{
        BucketCorsConfiguration, BucketEncryptionConfiguration, BucketLifecycleConfiguration,
        BucketPublicAccessBlockConfiguration, BucketTagging, BucketVersioningConfiguration,
        HeadBucketOutput, ListBucketsOutput,
    },
};

/// Bucket operations service (blocking).
///
/// Created by [`BlockingClient::buckets`](crate::BlockingClient::buckets).
///
/// Start here for common bucket flows:
///
/// - [`list`](crate::api::BlockingBucketsService::list) to enumerate accessible buckets
/// - [`create`](crate::api::BlockingBucketsService::create) to create a bucket
/// - [`put_lifecycle`](crate::api::BlockingBucketsService::put_lifecycle) to manage lifecycle
///   rules
/// - [`put_cors`](crate::api::BlockingBucketsService::put_cors) and
///   [`put_encryption`](crate::api::BlockingBucketsService::put_encryption) for bucket settings
#[derive(Clone)]
pub struct BlockingBucketsService {
    client: BlockingClient,
}

impl BlockingBucketsService {
    pub(crate) fn new(client: BlockingClient) -> Self {
        Self { client }
    }

    /// Starts a request to list buckets.
    pub fn list(&self) -> BlockingListBucketsRequest {
        BlockingListBucketsRequest {
            client: self.client.clone(),
        }
    }

    /// Starts a request to check if a bucket exists.
    pub fn head(&self, bucket: impl Into<String>) -> BlockingHeadBucketRequest {
        BlockingHeadBucketRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to create a bucket.
    pub fn create(&self, bucket: impl Into<String>) -> BlockingCreateBucketRequest {
        BlockingCreateBucketRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            location_constraint: None,
        }
    }

    /// Starts a request to delete a bucket.
    pub fn delete(&self, bucket: impl Into<String>) -> BlockingDeleteBucketRequest {
        BlockingDeleteBucketRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to get bucket versioning.
    pub fn get_versioning(&self, bucket: impl Into<String>) -> BlockingGetBucketVersioningRequest {
        BlockingGetBucketVersioningRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to set bucket versioning.
    pub fn put_versioning(&self, bucket: impl Into<String>) -> BlockingPutBucketVersioningRequest {
        BlockingPutBucketVersioningRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            body: None,
        }
    }

    /// Starts a request to get bucket lifecycle configuration.
    pub fn get_lifecycle(&self, bucket: impl Into<String>) -> BlockingGetBucketLifecycleRequest {
        BlockingGetBucketLifecycleRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to set bucket lifecycle configuration.
    pub fn put_lifecycle(&self, bucket: impl Into<String>) -> BlockingPutBucketLifecycleRequest {
        BlockingPutBucketLifecycleRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            body: None,
        }
    }

    /// Starts a request to delete bucket lifecycle configuration.
    pub fn delete_lifecycle(
        &self,
        bucket: impl Into<String>,
    ) -> BlockingDeleteBucketLifecycleRequest {
        BlockingDeleteBucketLifecycleRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to get bucket CORS rules.
    pub fn get_cors(&self, bucket: impl Into<String>) -> BlockingGetBucketCorsRequest {
        BlockingGetBucketCorsRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to set bucket CORS rules.
    pub fn put_cors(&self, bucket: impl Into<String>) -> BlockingPutBucketCorsRequest {
        BlockingPutBucketCorsRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            body: None,
        }
    }

    /// Starts a request to delete bucket CORS rules.
    pub fn delete_cors(&self, bucket: impl Into<String>) -> BlockingDeleteBucketCorsRequest {
        BlockingDeleteBucketCorsRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to get bucket tags.
    pub fn get_tagging(&self, bucket: impl Into<String>) -> BlockingGetBucketTaggingRequest {
        BlockingGetBucketTaggingRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to set bucket tags.
    pub fn put_tagging(&self, bucket: impl Into<String>) -> BlockingPutBucketTaggingRequest {
        BlockingPutBucketTaggingRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            body: None,
        }
    }

    /// Starts a request to delete bucket tags.
    pub fn delete_tagging(&self, bucket: impl Into<String>) -> BlockingDeleteBucketTaggingRequest {
        BlockingDeleteBucketTaggingRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to get bucket encryption configuration.
    pub fn get_encryption(&self, bucket: impl Into<String>) -> BlockingGetBucketEncryptionRequest {
        BlockingGetBucketEncryptionRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to set bucket encryption configuration.
    pub fn put_encryption(&self, bucket: impl Into<String>) -> BlockingPutBucketEncryptionRequest {
        BlockingPutBucketEncryptionRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            body: None,
        }
    }

    /// Starts a request to delete bucket encryption configuration.
    pub fn delete_encryption(
        &self,
        bucket: impl Into<String>,
    ) -> BlockingDeleteBucketEncryptionRequest {
        BlockingDeleteBucketEncryptionRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to get public access block settings.
    pub fn get_public_access_block(
        &self,
        bucket: impl Into<String>,
    ) -> BlockingGetBucketPublicAccessBlockRequest {
        BlockingGetBucketPublicAccessBlockRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to set public access block settings.
    pub fn put_public_access_block(
        &self,
        bucket: impl Into<String>,
    ) -> BlockingPutBucketPublicAccessBlockRequest {
        BlockingPutBucketPublicAccessBlockRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            configuration: None,
        }
    }

    /// Starts a request to delete public access block settings.
    pub fn delete_public_access_block(
        &self,
        bucket: impl Into<String>,
    ) -> BlockingDeleteBucketPublicAccessBlockRequest {
        BlockingDeleteBucketPublicAccessBlockRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
        }
    }

    /// Starts a request to read a raw bucket config subresource.
    pub fn get_config_raw(
        &self,
        bucket: impl Into<String>,
        subresource: impl Into<String>,
    ) -> BlockingGetBucketConfigRawRequest {
        BlockingGetBucketConfigRawRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            subresource: subresource.into(),
        }
    }

    /// Starts a request to write a raw bucket config subresource.
    pub fn put_config_raw(
        &self,
        bucket: impl Into<String>,
        subresource: impl Into<String>,
    ) -> BlockingPutBucketConfigRawRequest {
        BlockingPutBucketConfigRawRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            subresource: subresource.into(),
            body: Bytes::new(),
        }
    }

    /// Starts a request to delete a raw bucket config subresource.
    pub fn delete_config_raw(
        &self,
        bucket: impl Into<String>,
        subresource: impl Into<String>,
    ) -> BlockingDeleteBucketConfigRawRequest {
        BlockingDeleteBucketConfigRawRequest {
            client: self.client.clone(),
            bucket: bucket.into(),
            subresource: subresource.into(),
        }
    }
}

/// Request builder for listing buckets.
///
/// Created by [`BlockingBucketsService::list`](crate::api::BlockingBucketsService::list).
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
/// let buckets = client.buckets().list().send()?;
/// # let _ = buckets;
/// # Ok(())
/// # }
/// ```
pub struct BlockingListBucketsRequest {
    client: BlockingClient,
}

impl BlockingListBucketsRequest {
    /// Sends the request.
    pub fn send(self) -> Result<ListBucketsOutput> {
        let resp = self.client.execute(
            Method::GET,
            None,
            None,
            Vec::new(),
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        parse_xml_response(resp, crate::util::xml::parse_list_buckets)
    }
}

/// Request builder for checking bucket existence.
pub struct BlockingHeadBucketRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingHeadBucketRequest {
    /// Sends the request.
    pub fn send(self) -> Result<HeadBucketOutput> {
        let resp = self.client.execute(
            Method::HEAD,
            Some(&self.bucket),
            None,
            Vec::new(),
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        Ok(HeadBucketOutput {
            region: crate::util::headers::header_string(resp.headers(), "x-amz-bucket-region"),
        })
    }
}

/// Request builder for creating a bucket.
pub struct BlockingCreateBucketRequest {
    client: BlockingClient,
    bucket: String,
    location_constraint: Option<String>,
}

impl BlockingCreateBucketRequest {
    /// Sets the location constraint for bucket creation.
    pub fn location_constraint(mut self, region: impl Into<String>) -> Result<Self> {
        let region = region.into();
        crate::util::validation::validate_region("region", &region)?;
        self.location_constraint = Some(region);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<()> {
        let mut headers = HeaderMap::new();
        let location_constraint =
            create_bucket_location_constraint(self.location_constraint, self.client.region())?;
        let body = match location_constraint {
            Some(region) => {
                let body = crate::util::xml::encode_create_bucket_configuration(&region)?;
                headers = xml_body_headers(body.as_ref())?;
                BlockingBody::Bytes(body)
            }
            None => BlockingBody::Empty,
        };

        self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            None,
            Vec::new(),
            headers,
            body,
        )?;

        Ok(())
    }
}

/// Request builder for deleting a bucket.
pub struct BlockingDeleteBucketRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingDeleteBucketRequest {
    /// Sends the request.
    pub fn send(self) -> Result<()> {
        self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            None,
            Vec::new(),
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        Ok(())
    }
}

/// Request builder for reading bucket versioning settings.
pub struct BlockingGetBucketVersioningRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingGetBucketVersioningRequest {
    /// Sends the request.
    pub fn send(self) -> Result<BucketVersioningConfiguration> {
        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            None,
            vec![("versioning".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        parse_xml_response(resp, crate::util::xml::parse_bucket_versioning)
    }
}

/// Request builder for updating bucket versioning settings.
pub struct BlockingPutBucketVersioningRequest {
    client: BlockingClient,
    bucket: String,
    body: Option<Bytes>,
}

impl BlockingPutBucketVersioningRequest {
    /// Sets the versioning configuration to apply.
    pub fn configuration(mut self, value: BucketVersioningConfiguration) -> Result<Self> {
        self.body = Some(crate::util::xml::encode_bucket_versioning(&value)?);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<()> {
        let body = require_configured(self.body, "put_versioning requires a configuration")?;
        let headers = xml_body_headers(body.as_ref())?;

        self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            None,
            vec![("versioning".to_string(), String::new())],
            headers,
            BlockingBody::Bytes(body),
        )?;

        Ok(())
    }
}

/// Request builder for reading bucket lifecycle configuration.
pub struct BlockingGetBucketLifecycleRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingGetBucketLifecycleRequest {
    /// Sends the request.
    pub fn send(self) -> Result<BucketLifecycleConfiguration> {
        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            None,
            vec![("lifecycle".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        parse_xml_response(resp, crate::util::xml::parse_bucket_lifecycle)
    }
}

/// Request builder for updating bucket lifecycle configuration.
///
/// Created by [`BlockingBucketsService::put_lifecycle`](crate::api::BlockingBucketsService::put_lifecycle).
///
/// # Example
///
/// ```no_run
/// # fn demo() -> Result<(), s3::Error> {
/// use s3::{
///     types::{BucketLifecycleConfiguration, BucketLifecycleRule, BucketLifecycleStatus},
///     Auth, BlockingClient,
/// };
///
/// let client = BlockingClient::builder("https://s3.example.com")?
///     .region("us-east-1")
///     .auth(Auth::from_env()?)
///     .build()?;
///
/// let config = BucketLifecycleConfiguration {
///     rules: vec![BucketLifecycleRule {
///         id: Some("expire-logs".into()),
///         status: BucketLifecycleStatus::Enabled,
///         prefix: Some("logs/".into()),
///         expiration_days: Some(30),
///         expiration_date: None,
///     }],
/// };
///
/// let output = client
///     .buckets()
///     .put_lifecycle("my-bucket")
///     .configuration(config)?
///     .send()?;
/// # let _ = output;
/// # Ok(())
/// # }
/// ```
pub struct BlockingPutBucketLifecycleRequest {
    client: BlockingClient,
    bucket: String,
    body: Option<Bytes>,
}

impl BlockingPutBucketLifecycleRequest {
    /// Sets the lifecycle configuration to apply.
    pub fn configuration(mut self, value: BucketLifecycleConfiguration) -> Result<Self> {
        self.body = Some(crate::util::xml::encode_bucket_lifecycle(&value)?);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<()> {
        let body = require_configured(self.body, "put_lifecycle requires a configuration")?;
        let headers = xml_body_headers(body.as_ref())?;

        self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            None,
            vec![("lifecycle".to_string(), String::new())],
            headers,
            BlockingBody::Bytes(body),
        )?;

        Ok(())
    }
}

/// Request builder for deleting bucket lifecycle configuration.
pub struct BlockingDeleteBucketLifecycleRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingDeleteBucketLifecycleRequest {
    /// Sends the request.
    pub fn send(self) -> Result<()> {
        self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            None,
            vec![("lifecycle".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        Ok(())
    }
}

/// Request builder for reading bucket CORS configuration.
pub struct BlockingGetBucketCorsRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingGetBucketCorsRequest {
    /// Sends the request.
    pub fn send(self) -> Result<BucketCorsConfiguration> {
        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            None,
            vec![("cors".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        parse_xml_response(resp, crate::util::xml::parse_bucket_cors)
    }
}

/// Request builder for updating bucket CORS configuration.
pub struct BlockingPutBucketCorsRequest {
    client: BlockingClient,
    bucket: String,
    body: Option<Bytes>,
}

impl BlockingPutBucketCorsRequest {
    /// Sets the CORS configuration to apply.
    pub fn configuration(mut self, value: BucketCorsConfiguration) -> Result<Self> {
        self.body = Some(crate::util::xml::encode_bucket_cors(&value)?);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<()> {
        let body = require_configured(self.body, "put_cors requires a configuration")?;
        let headers = xml_body_headers(body.as_ref())?;

        self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            None,
            vec![("cors".to_string(), String::new())],
            headers,
            BlockingBody::Bytes(body),
        )?;

        Ok(())
    }
}

/// Request builder for deleting bucket CORS configuration.
pub struct BlockingDeleteBucketCorsRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingDeleteBucketCorsRequest {
    /// Sends the request.
    pub fn send(self) -> Result<()> {
        self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            None,
            vec![("cors".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        Ok(())
    }
}

/// Request builder for reading bucket tags.
pub struct BlockingGetBucketTaggingRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingGetBucketTaggingRequest {
    /// Sends the request.
    pub fn send(self) -> Result<BucketTagging> {
        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            None,
            vec![("tagging".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        parse_xml_response(resp, crate::util::xml::parse_bucket_tagging)
    }
}

/// Request builder for updating bucket tags.
pub struct BlockingPutBucketTaggingRequest {
    client: BlockingClient,
    bucket: String,
    body: Option<Bytes>,
}

impl BlockingPutBucketTaggingRequest {
    /// Sets the tag set to apply.
    pub fn tagging(mut self, value: BucketTagging) -> Result<Self> {
        self.body = Some(crate::util::xml::encode_bucket_tagging(&value)?);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<()> {
        let body = require_configured(self.body, "put_tagging requires a tag set")?;
        let headers = xml_body_headers(body.as_ref())?;

        self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            None,
            vec![("tagging".to_string(), String::new())],
            headers,
            BlockingBody::Bytes(body),
        )?;

        Ok(())
    }
}

/// Request builder for deleting bucket tags.
pub struct BlockingDeleteBucketTaggingRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingDeleteBucketTaggingRequest {
    /// Sends the request.
    pub fn send(self) -> Result<()> {
        self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            None,
            vec![("tagging".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        Ok(())
    }
}

/// Request builder for reading bucket encryption configuration.
pub struct BlockingGetBucketEncryptionRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingGetBucketEncryptionRequest {
    /// Sends the request.
    pub fn send(self) -> Result<BucketEncryptionConfiguration> {
        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            None,
            vec![("encryption".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        parse_xml_response(resp, crate::util::xml::parse_bucket_encryption)
    }
}

/// Request builder for updating bucket encryption configuration.
pub struct BlockingPutBucketEncryptionRequest {
    client: BlockingClient,
    bucket: String,
    body: Option<Bytes>,
}

impl BlockingPutBucketEncryptionRequest {
    /// Sets the encryption configuration to apply.
    pub fn configuration(mut self, value: BucketEncryptionConfiguration) -> Result<Self> {
        self.body = Some(crate::util::xml::encode_bucket_encryption(&value)?);
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<()> {
        let body = require_configured(self.body, "put_encryption requires a configuration")?;
        let headers = xml_body_headers(body.as_ref())?;

        self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            None,
            vec![("encryption".to_string(), String::new())],
            headers,
            BlockingBody::Bytes(body),
        )?;

        Ok(())
    }
}

/// Request builder for deleting bucket encryption configuration.
pub struct BlockingDeleteBucketEncryptionRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingDeleteBucketEncryptionRequest {
    /// Sends the request.
    pub fn send(self) -> Result<()> {
        self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            None,
            vec![("encryption".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        Ok(())
    }
}

/// Request builder for reading public access block settings.
pub struct BlockingGetBucketPublicAccessBlockRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingGetBucketPublicAccessBlockRequest {
    /// Sends the request.
    pub fn send(self) -> Result<BucketPublicAccessBlockConfiguration> {
        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            None,
            vec![("publicAccessBlock".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        parse_xml_response(resp, crate::util::xml::parse_bucket_public_access_block)
    }
}

/// Request builder for updating public access block settings.
pub struct BlockingPutBucketPublicAccessBlockRequest {
    client: BlockingClient,
    bucket: String,
    configuration: Option<BucketPublicAccessBlockConfiguration>,
}

impl BlockingPutBucketPublicAccessBlockRequest {
    /// Sets the public access block configuration to apply.
    pub fn configuration(mut self, value: BucketPublicAccessBlockConfiguration) -> Self {
        self.configuration = Some(value);
        self
    }

    /// Sends the request.
    pub fn send(self) -> Result<()> {
        let configuration = require_configured(
            self.configuration,
            "put_public_access_block requires a configuration",
        )?;
        let body = crate::util::xml::encode_bucket_public_access_block(&configuration)?;
        let headers = xml_body_headers(body.as_ref())?;

        self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            None,
            vec![("publicAccessBlock".to_string(), String::new())],
            headers,
            BlockingBody::Bytes(body),
        )?;

        Ok(())
    }
}

/// Request builder for deleting public access block settings.
pub struct BlockingDeleteBucketPublicAccessBlockRequest {
    client: BlockingClient,
    bucket: String,
}

impl BlockingDeleteBucketPublicAccessBlockRequest {
    /// Sends the request.
    pub fn send(self) -> Result<()> {
        self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            None,
            vec![("publicAccessBlock".to_string(), String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        Ok(())
    }
}

/// Request builder for fetching raw bucket config XML.
pub struct BlockingGetBucketConfigRawRequest {
    client: BlockingClient,
    bucket: String,
    subresource: String,
}

impl BlockingGetBucketConfigRawRequest {
    /// Sends the request.
    pub fn send(self) -> Result<String> {
        validate_subresource(&self.subresource)?;

        let resp = self.client.execute(
            Method::GET,
            Some(&self.bucket),
            None,
            vec![(self.subresource, String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        crate::util::text::decode_utf8_response_body(resp.body()).map(str::to_owned)
    }
}

/// Request builder for writing raw bucket config XML.
pub struct BlockingPutBucketConfigRawRequest {
    client: BlockingClient,
    bucket: String,
    subresource: String,
    body: Bytes,
}

impl BlockingPutBucketConfigRawRequest {
    /// Sets the request body from an XML string.
    pub fn body_xml(mut self, xml: impl Into<String>) -> Result<Self> {
        let xml = xml.into();
        if xml.is_empty() {
            return Err(Error::invalid_config(
                "put_config_raw requires a request body",
            ));
        }
        self.body = Bytes::from(xml);
        Ok(self)
    }

    /// Sets the request body from raw bytes.
    pub fn body_bytes(mut self, bytes: impl Into<Bytes>) -> Result<Self> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(Error::invalid_config(
                "put_config_raw requires a request body",
            ));
        }
        self.body = bytes;
        Ok(self)
    }

    /// Sends the request.
    pub fn send(self) -> Result<()> {
        validate_subresource(&self.subresource)?;
        if self.body.is_empty() {
            return Err(Error::invalid_config(
                "put_config_raw requires a request body",
            ));
        }

        let headers = xml_body_headers(self.body.as_ref())?;

        self.client.execute(
            Method::PUT,
            Some(&self.bucket),
            None,
            vec![(self.subresource, String::new())],
            headers,
            BlockingBody::Bytes(self.body),
        )?;

        Ok(())
    }
}

/// Request builder for deleting raw bucket config.
pub struct BlockingDeleteBucketConfigRawRequest {
    client: BlockingClient,
    bucket: String,
    subresource: String,
}

impl BlockingDeleteBucketConfigRawRequest {
    /// Sends the request.
    pub fn send(self) -> Result<()> {
        validate_subresource(&self.subresource)?;

        self.client.execute(
            Method::DELETE,
            Some(&self.bucket),
            None,
            vec![(self.subresource, String::new())],
            HeaderMap::new(),
            BlockingBody::Empty,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Auth,
        types::{
            BucketCorsConfiguration, BucketEncryptionConfiguration, BucketLifecycleConfiguration,
            BucketTagging, BucketVersioningConfiguration, Tag,
        },
    };

    fn test_client() -> BlockingClient {
        BlockingClient::builder("https://s3.example.com")
            .expect("builder should parse")
            .region("us-east-1")
            .auth(Auth::Anonymous)
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
    fn bucket_config_setters_reject_invalid_values() {
        let buckets = test_client().buckets();

        assert_invalid_config(
            buckets.create("bucket").location_constraint(" eu-west-1"),
            "region",
        );
        assert_invalid_config(
            buckets
                .put_versioning("bucket")
                .configuration(BucketVersioningConfiguration::default()),
            "versioning",
        );
        assert_invalid_config(
            buckets
                .put_lifecycle("bucket")
                .configuration(BucketLifecycleConfiguration::default()),
            "lifecycle",
        );
        assert_invalid_config(
            buckets
                .put_cors("bucket")
                .configuration(BucketCorsConfiguration::default()),
            "cors",
        );
        assert_invalid_config(
            buckets.put_tagging("bucket").tagging(BucketTagging {
                tags: vec![Tag {
                    key: String::new(),
                    value: "value".to_string(),
                }],
            }),
            "tag key",
        );
        assert_invalid_config(
            buckets
                .put_encryption("bucket")
                .configuration(BucketEncryptionConfiguration::default()),
            "encryption",
        );
        assert_invalid_config(
            buckets.put_config_raw("bucket", "versioning").body_xml(""),
            "request body",
        );
        assert_invalid_config(
            buckets
                .put_config_raw("bucket", "versioning")
                .body_bytes(Bytes::new()),
            "request body",
        );
    }

    #[test]
    fn bucket_config_send_requires_explicit_configuration() {
        let buckets = test_client().buckets();

        assert_invalid_config(
            buckets.put_versioning("bucket").send(),
            "requires a configuration",
        );
        assert_invalid_config(
            buckets.put_lifecycle("bucket").send(),
            "requires a configuration",
        );
        assert_invalid_config(
            buckets.put_cors("bucket").send(),
            "requires a configuration",
        );
        assert_invalid_config(buckets.put_tagging("bucket").send(), "requires a tag set");
        assert_invalid_config(
            buckets.put_encryption("bucket").send(),
            "requires a configuration",
        );
        assert_invalid_config(
            buckets.put_public_access_block("bucket").send(),
            "requires a configuration",
        );
    }
}
