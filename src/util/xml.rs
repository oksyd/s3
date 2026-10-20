use bytes::Bytes;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    error::Error,
    types::{self, xml},
};

const S3_XMLNS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

fn parse_response_xml<T: serde::de::DeserializeOwned>(body: &str, root: &str) -> Result<T, Error> {
    use quick_xml::{Reader, events::Event};

    // Serde deserializes fields without checking the XML document's root name.
    // Check it before optional fields can turn an unrelated document into success.
    let mut reader = Reader::from_str(body);
    loop {
        match reader.read_event().map_err(|error| {
            Error::decode(
                format!("invalid {root} XML response"),
                Some(Box::new(error)),
            )
        })? {
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) => continue,
            Event::Text(text) if text.as_ref().bytes().all(|b| b.is_ascii_whitespace()) => continue,
            Event::Start(element) | Event::Empty(element)
                if element.local_name().as_ref() == root =>
            {
                break;
            }
            _ => return Err(Error::decode(format!("expected {root} XML response"), None)),
        }
    }

    quick_xml::de::from_str(body).map_err(|error| {
        Error::decode(
            format!("failed to parse {root} XML response"),
            Some(Box::new(error)),
        )
    })
}

#[derive(serde::Deserialize)]
enum XmlServiceError {
    Error(xml::XmlError),
    ErrorResponse {
        #[serde(rename = "Error")]
        error: xml::XmlError,
        #[serde(rename = "RequestId")]
        request_id: Option<String>,
        #[serde(rename = "HostId")]
        host_id: Option<String>,
    },
}

pub(crate) fn parse_error_xml(body: &str) -> Option<xml::XmlError> {
    let error = match quick_xml::de::from_str::<XmlServiceError>(body).ok()? {
        XmlServiceError::Error(error) => error,
        XmlServiceError::ErrorResponse {
            mut error,
            request_id,
            host_id,
        } => {
            trim_optional_string(&mut error.request_id);
            trim_optional_string(&mut error.host_id);
            error.request_id = error.request_id.or(request_id);
            error.host_id = error.host_id.or(host_id);
            error
        }
    };
    normalize_error_fields(error)
}

fn normalize_error_fields(mut error: xml::XmlError) -> Option<xml::XmlError> {
    trim_optional_string(&mut error.code);
    trim_optional_string(&mut error.message);
    trim_optional_string(&mut error.request_id);
    trim_optional_string(&mut error.host_id);

    if error.code.is_none()
        && error.message.is_none()
        && error.request_id.is_none()
        && error.host_id.is_none()
    {
        return None;
    }

    Some(error)
}

fn trim_optional_string(value: &mut Option<String>) {
    let trimmed = value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    *value = trimmed;
}

pub(crate) fn parse_list_objects_v2(body: &str) -> Result<types::ListObjectsV2Output, Error> {
    let parsed = parse_response_xml::<xml::XmlListBucketResult>(body, "ListBucketResult")?;
    Ok(types::ListObjectsV2Output::from(parsed))
}

pub(crate) fn parse_list_buckets(body: &str) -> Result<types::ListBucketsOutput, Error> {
    let parsed =
        parse_response_xml::<xml::XmlListAllMyBucketsResult>(body, "ListAllMyBucketsResult")?;
    Ok(types::ListBucketsOutput::from(parsed))
}

pub(crate) fn parse_bucket_versioning(
    body: &str,
) -> Result<types::BucketVersioningConfiguration, Error> {
    let parsed =
        parse_response_xml::<xml::XmlVersioningConfiguration>(body, "VersioningConfiguration")?;

    Ok(types::BucketVersioningConfiguration {
        status: parsed
            .status
            .as_deref()
            .map(parse_versioning_status)
            .transpose()?,
        mfa_delete: parsed
            .mfa_delete
            .as_deref()
            .map(parse_mfa_delete)
            .transpose()?,
    })
}

pub(crate) fn parse_bucket_lifecycle(
    body: &str,
) -> Result<types::BucketLifecycleConfiguration, Error> {
    let parsed =
        parse_response_xml::<xml::XmlLifecycleConfiguration>(body, "LifecycleConfiguration")?;

    let rules = parsed
        .rules
        .into_iter()
        .map(parse_lifecycle_rule)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(types::BucketLifecycleConfiguration { rules })
}

pub(crate) fn parse_bucket_cors(body: &str) -> Result<types::BucketCorsConfiguration, Error> {
    let parsed = parse_response_xml::<xml::XmlCorsConfiguration>(body, "CORSConfiguration")?;

    Ok(types::BucketCorsConfiguration {
        rules: parsed
            .rules
            .into_iter()
            .map(|r| types::BucketCorsRule {
                id: r.id,
                allowed_origins: r.allowed_origins,
                allowed_methods: r
                    .allowed_methods
                    .into_iter()
                    .map(parse_cors_method)
                    .collect(),
                allowed_headers: r.allowed_headers,
                expose_headers: r.expose_headers,
                max_age_seconds: r.max_age_seconds,
            })
            .collect(),
    })
}

pub(crate) fn parse_bucket_tagging(body: &str) -> Result<types::BucketTagging, Error> {
    let parsed = parse_response_xml::<xml::XmlTagging>(body, "Tagging")?;

    let tags = parsed
        .tag_set
        .tags
        .into_iter()
        .map(|t| types::Tag {
            key: t.key,
            value: t.value,
        })
        .collect();

    Ok(types::BucketTagging { tags })
}

pub(crate) fn parse_bucket_encryption(
    body: &str,
) -> Result<types::BucketEncryptionConfiguration, Error> {
    let parsed = parse_response_xml::<xml::XmlServerSideEncryptionConfiguration>(
        body,
        "ServerSideEncryptionConfiguration",
    )?;

    let rules = parsed
        .rules
        .into_iter()
        .map(parse_encryption_rule)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(types::BucketEncryptionConfiguration { rules })
}

pub(crate) fn parse_bucket_public_access_block(
    body: &str,
) -> Result<types::BucketPublicAccessBlockConfiguration, Error> {
    let parsed = parse_response_xml::<xml::XmlPublicAccessBlockConfiguration>(
        body,
        "PublicAccessBlockConfiguration",
    )?;

    Ok(types::BucketPublicAccessBlockConfiguration {
        block_public_acls: required_public_access_block_field(
            parsed.block_public_acls,
            "BlockPublicAcls",
        )?,
        ignore_public_acls: required_public_access_block_field(
            parsed.ignore_public_acls,
            "IgnorePublicAcls",
        )?,
        block_public_policy: required_public_access_block_field(
            parsed.block_public_policy,
            "BlockPublicPolicy",
        )?,
        restrict_public_buckets: required_public_access_block_field(
            parsed.restrict_public_buckets,
            "RestrictPublicBuckets",
        )?,
    })
}

pub(crate) fn parse_delete_objects(body: &str) -> Result<types::DeleteObjectsOutput, Error> {
    let parsed = parse_response_xml::<xml::XmlDeleteResult>(body, "DeleteResult")?;
    Ok(types::DeleteObjectsOutput::from(parsed))
}

pub(crate) fn parse_copy_object(body: &str) -> Result<types::CopyObjectOutput, Error> {
    let parsed = parse_response_xml::<xml::XmlCopyObjectResult>(body, "CopyObjectResult")?;
    Ok(types::CopyObjectOutput::from(parsed))
}

#[cfg(all(feature = "multipart", any(feature = "async", feature = "blocking")))]
pub(crate) fn parse_create_multipart_upload(
    body: &str,
) -> Result<types::CreateMultipartUploadOutput, Error> {
    let parsed = parse_response_xml::<xml::XmlInitiateMultipartUploadResult>(
        body,
        "InitiateMultipartUploadResult",
    )?;
    Ok(types::CreateMultipartUploadOutput::from(parsed))
}

#[cfg(all(feature = "multipart", any(feature = "async", feature = "blocking")))]
pub(crate) fn parse_complete_multipart_upload(
    body: &str,
) -> Result<types::CompleteMultipartUploadOutput, Error> {
    let parsed = parse_response_xml::<xml::XmlCompleteMultipartUploadResult>(
        body,
        "CompleteMultipartUploadResult",
    )?;
    Ok(types::CompleteMultipartUploadOutput::from(parsed))
}

#[cfg(all(feature = "multipart", any(feature = "async", feature = "blocking")))]
pub(crate) fn parse_list_parts(body: &str) -> Result<types::ListPartsOutput, Error> {
    let parsed = parse_response_xml::<xml::XmlListPartsResult>(body, "ListPartsResult")?;
    Ok(types::ListPartsOutput::from(parsed))
}

#[cfg(all(feature = "multipart", any(feature = "async", feature = "blocking")))]
pub(crate) fn parse_upload_part_copy(body: &str) -> Result<types::UploadPartCopyOutput, Error> {
    let parsed = parse_response_xml::<xml::XmlCopyPartResult>(body, "CopyPartResult")?;
    Ok(types::UploadPartCopyOutput::from(parsed))
}

pub(crate) fn encode_create_bucket_configuration(region: &str) -> Result<Bytes, Error> {
    validate_non_empty_trimmed_field(
        region,
        "create bucket location constraint must not be empty",
        "create bucket location constraint must not include leading or trailing whitespace",
    )?;
    crate::auth::Region::new(region)?;

    #[derive(serde::Serialize)]
    #[serde(rename = "CreateBucketConfiguration")]
    struct XmlCreateBucketConfiguration<'a> {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "LocationConstraint")]
        location_constraint: &'a str,
    }

    let xml = quick_xml::se::to_string(&XmlCreateBucketConfiguration {
        xmlns: S3_XMLNS,
        location_constraint: region,
    })
    .map_err(|e| {
        Error::decode(
            "failed to encode CreateBucketConfiguration XML",
            Some(Box::new(e)),
        )
    })?;
    Ok(Bytes::from(xml))
}

#[cfg(feature = "multipart")]
pub(crate) fn encode_complete_multipart_upload<'a>(
    parts: impl IntoIterator<Item = &'a types::CompletedPart>,
) -> Result<Bytes, Error> {
    #[derive(serde::Serialize)]
    #[serde(rename = "CompleteMultipartUpload")]
    struct XmlOut<'a> {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "Part")]
        parts: Vec<XmlPart<'a>>,
    }

    #[derive(serde::Serialize)]
    struct XmlPart<'a> {
        #[serde(rename = "PartNumber")]
        part_number: u32,
        #[serde(rename = "ETag")]
        etag: &'a str,
    }

    let parts: Vec<_> = parts
        .into_iter()
        .map(|p| XmlPart {
            part_number: p.part_number(),
            etag: p.etag(),
        })
        .collect();
    if parts.is_empty() {
        return Err(Error::invalid_config(
            "complete_multipart_upload requires at least one completed part",
        ));
    }

    let xml = quick_xml::se::to_string(&XmlOut {
        xmlns: S3_XMLNS,
        parts,
    })
    .map_err(|e| {
        Error::decode(
            "failed to encode CompleteMultipartUpload XML",
            Some(Box::new(e)),
        )
    })?;

    Ok(Bytes::from(xml))
}

pub(crate) fn encode_delete_objects(
    objects: &[types::DeleteObjectIdentifier],
    quiet: bool,
) -> Result<Bytes, Error> {
    if objects.is_empty() {
        return Err(Error::invalid_config(
            "delete_objects requires at least one object",
        ));
    }
    if objects.len() > types::MAX_DELETE_OBJECTS_PER_REQUEST {
        return Err(Error::invalid_config(
            "delete_objects supports at most 1000 objects per request",
        ));
    }
    #[derive(serde::Serialize)]
    #[serde(rename = "Delete")]
    struct XmlOut<'a> {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "Object")]
        objects: Vec<XmlObject<'a>>,
        #[serde(rename = "Quiet")]
        quiet: bool,
    }

    #[derive(serde::Serialize)]
    struct XmlObject<'a> {
        #[serde(rename = "Key")]
        key: &'a str,
        #[serde(rename = "VersionId", skip_serializing_if = "Option::is_none")]
        version_id: Option<&'a str>,
    }

    let xml = quick_xml::se::to_string(&XmlOut {
        xmlns: S3_XMLNS,
        objects: objects
            .iter()
            .map(|o| XmlObject {
                key: o.key(),
                version_id: o.version_id(),
            })
            .collect(),
        quiet,
    })
    .map_err(|e| Error::decode("failed to encode DeleteObjects XML", Some(Box::new(e))))?;

    Ok(Bytes::from(xml))
}

pub(crate) fn encode_bucket_versioning(
    configuration: &types::BucketVersioningConfiguration,
) -> Result<Bytes, Error> {
    validate_bucket_versioning(configuration)?;

    #[derive(serde::Serialize)]
    #[serde(rename = "VersioningConfiguration")]
    struct XmlOut {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "Status", skip_serializing_if = "Option::is_none")]
        status: Option<&'static str>,
        #[serde(rename = "MfaDelete", skip_serializing_if = "Option::is_none")]
        mfa_delete: Option<&'static str>,
    }

    let xml = quick_xml::se::to_string(&XmlOut {
        xmlns: S3_XMLNS,
        status: configuration.status.map(versioning_status_str),
        mfa_delete: configuration.mfa_delete.map(mfa_delete_str),
    })
    .map_err(|e| {
        Error::decode(
            "failed to encode VersioningConfiguration XML",
            Some(Box::new(e)),
        )
    })?;
    Ok(Bytes::from(xml))
}

pub(crate) fn validate_bucket_versioning(
    configuration: &types::BucketVersioningConfiguration,
) -> Result<(), Error> {
    if configuration.status.is_none() {
        return Err(Error::invalid_config(
            "bucket versioning configuration must include status",
        ));
    }
    Ok(())
}

pub(crate) fn encode_bucket_lifecycle(
    configuration: &types::BucketLifecycleConfiguration,
) -> Result<Bytes, Error> {
    validate_bucket_lifecycle(configuration)?;

    #[derive(serde::Serialize)]
    #[serde(rename = "LifecycleConfiguration")]
    struct XmlOut<'a> {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "Rule")]
        rules: Vec<XmlRuleOut<'a>>,
    }

    #[derive(serde::Serialize)]
    struct XmlRuleOut<'a> {
        #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
        id: Option<&'a str>,
        #[serde(rename = "Status")]
        status: &'static str,
        #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
        filter: Option<XmlFilterOut<'a>>,
        #[serde(rename = "Expiration", skip_serializing_if = "Option::is_none")]
        expiration: Option<XmlExpirationOut<'a>>,
    }

    #[derive(serde::Serialize)]
    struct XmlFilterOut<'a> {
        #[serde(rename = "Prefix")]
        prefix: &'a str,
    }

    #[derive(serde::Serialize)]
    struct XmlExpirationOut<'a> {
        #[serde(rename = "Days", skip_serializing_if = "Option::is_none")]
        days: Option<u32>,
        #[serde(rename = "Date", skip_serializing_if = "Option::is_none")]
        date: Option<&'a str>,
    }

    let rules = configuration
        .rules
        .iter()
        .map(|r| XmlRuleOut {
            id: r.id.as_deref(),
            status: lifecycle_status_str(r.status),
            filter: r.prefix.as_deref().map(|prefix| XmlFilterOut { prefix }),
            expiration: if r.expiration_days.is_some() || r.expiration_date.is_some() {
                Some(XmlExpirationOut {
                    days: r.expiration_days,
                    date: r.expiration_date.as_deref(),
                })
            } else {
                None
            },
        })
        .collect::<Vec<_>>();

    let xml = quick_xml::se::to_string(&XmlOut {
        xmlns: S3_XMLNS,
        rules,
    })
    .map_err(|e| {
        Error::decode(
            "failed to encode LifecycleConfiguration XML",
            Some(Box::new(e)),
        )
    })?;
    Ok(Bytes::from(xml))
}

pub(crate) fn encode_bucket_cors(
    configuration: &types::BucketCorsConfiguration,
) -> Result<Bytes, Error> {
    validate_bucket_cors(configuration)?;

    #[derive(serde::Serialize)]
    #[serde(rename = "CORSConfiguration")]
    struct XmlOut<'a> {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "CORSRule")]
        rules: Vec<XmlRuleOut<'a>>,
    }

    #[derive(serde::Serialize)]
    struct XmlRuleOut<'a> {
        #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
        id: Option<&'a str>,
        #[serde(rename = "AllowedOrigin")]
        allowed_origins: &'a [String],
        #[serde(rename = "AllowedMethod")]
        allowed_methods: Vec<&'a str>,
        #[serde(rename = "AllowedHeader", skip_serializing_if = "<[String]>::is_empty")]
        allowed_headers: &'a [String],
        #[serde(rename = "ExposeHeader", skip_serializing_if = "<[String]>::is_empty")]
        expose_headers: &'a [String],
        #[serde(rename = "MaxAgeSeconds", skip_serializing_if = "Option::is_none")]
        max_age_seconds: Option<u32>,
    }

    let xml = quick_xml::se::to_string(&XmlOut {
        xmlns: S3_XMLNS,
        rules: configuration
            .rules
            .iter()
            .map(|r| XmlRuleOut {
                id: r.id.as_deref(),
                allowed_origins: &r.allowed_origins,
                allowed_methods: r.allowed_methods.iter().map(|m| m.as_str()).collect(),
                allowed_headers: &r.allowed_headers,
                expose_headers: &r.expose_headers,
                max_age_seconds: r.max_age_seconds,
            })
            .collect(),
    })
    .map_err(|e| Error::decode("failed to encode CORSConfiguration XML", Some(Box::new(e))))?;
    Ok(Bytes::from(xml))
}

pub(crate) fn encode_bucket_tagging(tagging: &types::BucketTagging) -> Result<Bytes, Error> {
    validate_bucket_tagging(tagging)?;

    #[derive(serde::Serialize)]
    #[serde(rename = "Tagging")]
    struct XmlOut<'a> {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "TagSet")]
        tag_set: XmlTagSet<'a>,
    }

    #[derive(serde::Serialize)]
    struct XmlTagSet<'a> {
        #[serde(rename = "Tag")]
        tags: Vec<XmlTag<'a>>,
    }

    #[derive(serde::Serialize)]
    struct XmlTag<'a> {
        #[serde(rename = "Key")]
        key: &'a str,
        #[serde(rename = "Value")]
        value: &'a str,
    }

    let xml = quick_xml::se::to_string(&XmlOut {
        xmlns: S3_XMLNS,
        tag_set: XmlTagSet {
            tags: tagging
                .tags
                .iter()
                .map(|t| XmlTag {
                    key: &t.key,
                    value: &t.value,
                })
                .collect(),
        },
    })
    .map_err(|e| Error::decode("failed to encode Tagging XML", Some(Box::new(e))))?;
    Ok(Bytes::from(xml))
}

pub(crate) fn encode_bucket_encryption(
    configuration: &types::BucketEncryptionConfiguration,
) -> Result<Bytes, Error> {
    validate_bucket_encryption(configuration)?;

    #[derive(serde::Serialize)]
    #[serde(rename = "ServerSideEncryptionConfiguration")]
    struct XmlOut<'a> {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "Rule")]
        rules: Vec<XmlRuleOut<'a>>,
    }

    #[derive(serde::Serialize)]
    struct XmlRuleOut<'a> {
        #[serde(rename = "ApplyServerSideEncryptionByDefault")]
        apply: XmlApplyOut<'a>,
        #[serde(rename = "BucketKeyEnabled", skip_serializing_if = "Option::is_none")]
        bucket_key_enabled: Option<bool>,
    }

    #[derive(serde::Serialize)]
    struct XmlApplyOut<'a> {
        #[serde(rename = "SSEAlgorithm")]
        sse_algorithm: &'a str,
        #[serde(rename = "KMSMasterKeyID", skip_serializing_if = "Option::is_none")]
        kms_master_key_id: Option<&'a str>,
    }

    let rules = configuration
        .rules
        .iter()
        .map(|r| XmlRuleOut {
            apply: XmlApplyOut {
                sse_algorithm: r.apply.sse_algorithm.as_str(),
                kms_master_key_id: r.apply.kms_master_key_id.as_deref(),
            },
            bucket_key_enabled: r.bucket_key_enabled,
        })
        .collect();

    let xml = quick_xml::se::to_string(&XmlOut {
        xmlns: S3_XMLNS,
        rules,
    })
    .map_err(|e| {
        Error::decode(
            "failed to encode ServerSideEncryptionConfiguration XML",
            Some(Box::new(e)),
        )
    })?;
    Ok(Bytes::from(xml))
}

pub(crate) fn encode_bucket_public_access_block(
    configuration: &types::BucketPublicAccessBlockConfiguration,
) -> Result<Bytes, Error> {
    #[derive(serde::Serialize)]
    #[serde(rename = "PublicAccessBlockConfiguration")]
    struct XmlOut {
        #[serde(rename = "@xmlns")]
        xmlns: &'static str,
        #[serde(rename = "BlockPublicAcls")]
        block_public_acls: bool,
        #[serde(rename = "IgnorePublicAcls")]
        ignore_public_acls: bool,
        #[serde(rename = "BlockPublicPolicy")]
        block_public_policy: bool,
        #[serde(rename = "RestrictPublicBuckets")]
        restrict_public_buckets: bool,
    }

    let xml = quick_xml::se::to_string(&XmlOut {
        xmlns: S3_XMLNS,
        block_public_acls: configuration.block_public_acls,
        ignore_public_acls: configuration.ignore_public_acls,
        block_public_policy: configuration.block_public_policy,
        restrict_public_buckets: configuration.restrict_public_buckets,
    })
    .map_err(|e| {
        Error::decode(
            "failed to encode PublicAccessBlockConfiguration XML",
            Some(Box::new(e)),
        )
    })?;
    Ok(Bytes::from(xml))
}

fn parse_lifecycle_rule(r: xml::XmlLifecycleRule) -> Result<types::BucketLifecycleRule, Error> {
    let status = parse_lifecycle_status(&r.status)?;
    let prefix = r
        .filter
        .and_then(|f| f.prefix)
        .or(r.prefix)
        .filter(|v| !v.is_empty());
    let (expiration_days, expiration_date) = match r.expiration {
        Some(exp) => (exp.days, exp.date),
        None => (None, None),
    };

    Ok(types::BucketLifecycleRule {
        id: r.id,
        status,
        prefix,
        expiration_days,
        expiration_date,
    })
}

fn parse_encryption_rule(
    r: xml::XmlServerSideEncryptionRule,
) -> Result<types::BucketEncryptionRule, Error> {
    let apply = r.apply.ok_or_else(|| {
        Error::decode(
            "missing ApplyServerSideEncryptionByDefault in encryption rule",
            None,
        )
    })?;

    Ok(types::BucketEncryptionRule {
        apply: types::ApplyServerSideEncryptionByDefault {
            sse_algorithm: parse_sse_algorithm(&apply.sse_algorithm),
            kms_master_key_id: apply.kms_master_key_id,
        },
        bucket_key_enabled: r.bucket_key_enabled,
    })
}

fn required_public_access_block_field(value: Option<bool>, field: &str) -> Result<bool, Error> {
    value.ok_or_else(|| {
        Error::decode(
            format!("missing {field} in PublicAccessBlockConfiguration"),
            None,
        )
    })
}

pub(crate) fn validate_bucket_lifecycle(
    configuration: &types::BucketLifecycleConfiguration,
) -> Result<(), Error> {
    if configuration.rules.is_empty() {
        return Err(Error::invalid_config(
            "bucket lifecycle configuration must include at least one rule",
        ));
    }

    let mut rule_ids = std::collections::BTreeSet::new();
    for rule in &configuration.rules {
        if let Some(id) = &rule.id {
            validate_non_empty_trimmed_field(
                id,
                "bucket lifecycle rule id must not be empty",
                "bucket lifecycle rule id must not include leading or trailing whitespace",
            )?;
            if id.len() > 255 {
                return Err(Error::invalid_config(
                    "bucket lifecycle rule id must be at most 255 bytes",
                ));
            }
            if !rule_ids.insert(id.as_str()) {
                return Err(Error::invalid_config(
                    "bucket lifecycle rule ids must be unique",
                ));
            }
        }
        match (rule.expiration_days, rule.expiration_date.as_deref()) {
            (Some(0), _) => {
                return Err(Error::invalid_config(
                    "bucket lifecycle expiration_days must be greater than 0",
                ));
            }
            (Some(_), Some(_)) => {
                return Err(Error::invalid_config(
                    "bucket lifecycle expiration must use either days or date, not both",
                ));
            }
            (None, None) => {
                return Err(Error::invalid_config(
                    "bucket lifecycle rule must include an expiration",
                ));
            }
            (Some(_), None) => {}
            (None, Some(date)) => {
                validate_lifecycle_expiration_date(date)?;
            }
        }
    }

    Ok(())
}

fn validate_lifecycle_expiration_date(value: &str) -> Result<(), Error> {
    validate_non_empty_trimmed_field(
        value,
        "bucket lifecycle expiration_date must not be empty",
        "bucket lifecycle expiration_date must not include leading or trailing whitespace",
    )?;
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| {
        Error::invalid_config("bucket lifecycle expiration_date must be an RFC3339 timestamp")
    })?;
    Ok(())
}

pub(crate) fn validate_bucket_cors(
    configuration: &types::BucketCorsConfiguration,
) -> Result<(), Error> {
    if configuration.rules.is_empty() {
        return Err(Error::invalid_config(
            "bucket cors configuration must include at least one rule",
        ));
    }

    for rule in &configuration.rules {
        if rule.allowed_origins.is_empty() {
            return Err(Error::invalid_config(
                "bucket cors rule must include at least one allowed origin",
            ));
        }
        if rule.allowed_methods.is_empty() {
            return Err(Error::invalid_config(
                "bucket cors rule must include at least one allowed method",
            ));
        }
        let mut origins = std::collections::BTreeSet::new();
        for origin in &rule.allowed_origins {
            validate_cors_allowed_origin(origin)?;
            if !origins.insert(origin.as_str()) {
                return Err(Error::invalid_config(
                    "bucket cors allowed origins must be unique",
                ));
            }
        }

        let mut methods = std::collections::BTreeSet::new();
        for method in &rule.allowed_methods {
            validate_cors_method(method.as_str())?;
            if !methods.insert(method.as_str()) {
                return Err(Error::invalid_config(
                    "bucket cors allowed methods must be unique",
                ));
            }
        }

        let mut allowed_headers = std::collections::BTreeSet::new();
        for header in &rule.allowed_headers {
            validate_cors_allowed_header(header)?;
            if !allowed_headers.insert(header.to_ascii_lowercase()) {
                return Err(Error::invalid_config(
                    "bucket cors allowed headers must be unique",
                ));
            }
        }

        let mut expose_headers = std::collections::BTreeSet::new();
        for header in &rule.expose_headers {
            validate_cors_expose_header(header)?;
            if !expose_headers.insert(header.to_ascii_lowercase()) {
                return Err(Error::invalid_config(
                    "bucket cors expose headers must be unique",
                ));
            }
        }
    }

    Ok(())
}

fn validate_cors_allowed_origin(value: &str) -> Result<(), Error> {
    validate_non_empty_trimmed_field(
        value,
        "bucket cors allowed origins must not be empty",
        "bucket cors allowed origins must not include leading or trailing whitespace",
    )?;
    if value
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(Error::invalid_config(
            "bucket cors allowed origins must not contain ASCII control or whitespace characters",
        ));
    }
    if value.bytes().filter(|b| *b == b'*').count() > 1 {
        return Err(Error::invalid_config(
            "bucket cors allowed origins must contain at most one wildcard",
        ));
    }
    Ok(())
}

fn validate_cors_method(value: &str) -> Result<(), Error> {
    validate_non_empty_trimmed_field(
        value,
        "bucket cors allowed methods must not be empty",
        "bucket cors allowed methods must not include leading or trailing whitespace",
    )?;
    if http::Method::from_bytes(value.as_bytes()).is_err() {
        return Err(Error::invalid_config(
            "bucket cors allowed methods must be valid HTTP method tokens",
        ));
    }
    Ok(())
}

fn validate_cors_allowed_header(value: &str) -> Result<(), Error> {
    validate_non_empty_trimmed_field(
        value,
        "bucket cors header names must not be empty",
        "bucket cors header names must not include leading or trailing whitespace",
    )?;
    if value.bytes().filter(|b| *b == b'*').count() > 1 {
        return Err(Error::invalid_config(
            "bucket cors allowed headers must contain at most one wildcard",
        ));
    }
    let header_name = value.replace('*', "x");
    if http::HeaderName::from_bytes(header_name.as_bytes()).is_err() {
        return Err(Error::invalid_config(
            "bucket cors allowed headers must be valid HTTP header patterns",
        ));
    }
    Ok(())
}

fn validate_cors_expose_header(value: &str) -> Result<(), Error> {
    validate_non_empty_trimmed_field(
        value,
        "bucket cors header names must not be empty",
        "bucket cors header names must not include leading or trailing whitespace",
    )?;
    if value.contains('*') {
        return Err(Error::invalid_config(
            "bucket cors expose headers must not contain wildcards",
        ));
    }
    if http::HeaderName::from_bytes(value.as_bytes()).is_err() {
        return Err(Error::invalid_config(
            "bucket cors expose headers must be valid HTTP header tokens",
        ));
    }
    Ok(())
}

pub(crate) fn validate_bucket_tagging(tagging: &types::BucketTagging) -> Result<(), Error> {
    if tagging.tags.len() > 50 {
        return Err(Error::invalid_config(
            "bucket tagging supports at most 50 tags",
        ));
    }

    let mut keys = std::collections::BTreeSet::new();
    for tag in &tagging.tags {
        validate_bucket_tag_key(&tag.key)?;
        validate_bucket_tag_value(&tag.value)?;

        if !keys.insert(tag.key.as_str()) {
            return Err(Error::invalid_config("bucket tag keys must be unique"));
        }
        if tag.key.len() > 128 {
            return Err(Error::invalid_config(
                "bucket tag key must be at most 128 bytes",
            ));
        }
        if tag.value.len() > 256 {
            return Err(Error::invalid_config(
                "bucket tag value must be at most 256 bytes",
            ));
        }
    }

    Ok(())
}

fn validate_bucket_tag_key(value: &str) -> Result<(), Error> {
    validate_non_empty_trimmed_field(
        value,
        "bucket tag key must not be empty",
        "bucket tag key must not include leading or trailing whitespace",
    )?;
    if value.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::invalid_config(
            "bucket tag key must not contain ASCII control characters",
        ));
    }
    Ok(())
}

fn validate_bucket_tag_value(value: &str) -> Result<(), Error> {
    if value.trim() != value {
        return Err(Error::invalid_config(
            "bucket tag value must not include leading or trailing whitespace",
        ));
    }
    if value.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::invalid_config(
            "bucket tag value must not contain ASCII control characters",
        ));
    }
    Ok(())
}

pub(crate) fn validate_bucket_encryption(
    configuration: &types::BucketEncryptionConfiguration,
) -> Result<(), Error> {
    if configuration.rules.is_empty() {
        return Err(Error::invalid_config(
            "bucket encryption configuration must include at least one rule",
        ));
    }

    for rule in &configuration.rules {
        if let types::SseAlgorithm::Other(value) = &rule.apply.sse_algorithm {
            validate_non_empty_trimmed_field(
                value,
                "bucket encryption SSE algorithm must not be empty",
                "bucket encryption SSE algorithm must not include leading or trailing whitespace",
            )?;
        }
        if let Some(kms_master_key_id) = &rule.apply.kms_master_key_id {
            validate_non_empty_trimmed_field(
                kms_master_key_id,
                "bucket encryption KMS master key id must not be empty",
                "bucket encryption KMS master key id must not include leading or trailing whitespace",
            )?;
        }
        if matches!(rule.apply.sse_algorithm, types::SseAlgorithm::Aes256)
            && rule.apply.kms_master_key_id.is_some()
        {
            return Err(Error::invalid_config(
                "bucket encryption KMS master key id requires an AWS KMS algorithm",
            ));
        }
    }

    Ok(())
}

fn validate_non_empty_trimmed_field(
    value: &str,
    empty_message: &'static str,
    whitespace_message: &'static str,
) -> Result<(), Error> {
    if value.is_empty() {
        return Err(Error::invalid_config(empty_message));
    }
    if value.trim() != value {
        return Err(Error::invalid_config(whitespace_message));
    }
    Ok(())
}

fn parse_versioning_status(value: &str) -> Result<types::BucketVersioningStatus, Error> {
    match value {
        "Enabled" => Ok(types::BucketVersioningStatus::Enabled),
        "Suspended" => Ok(types::BucketVersioningStatus::Suspended),
        _ => Err(Error::decode("unknown bucket versioning status", None)),
    }
}

fn versioning_status_str(value: types::BucketVersioningStatus) -> &'static str {
    match value {
        types::BucketVersioningStatus::Enabled => "Enabled",
        types::BucketVersioningStatus::Suspended => "Suspended",
    }
}

fn parse_mfa_delete(value: &str) -> Result<types::BucketMfaDeleteStatus, Error> {
    match value {
        "Enabled" => Ok(types::BucketMfaDeleteStatus::Enabled),
        "Disabled" => Ok(types::BucketMfaDeleteStatus::Disabled),
        _ => Err(Error::decode("unknown bucket MFA delete status", None)),
    }
}

fn mfa_delete_str(value: types::BucketMfaDeleteStatus) -> &'static str {
    match value {
        types::BucketMfaDeleteStatus::Enabled => "Enabled",
        types::BucketMfaDeleteStatus::Disabled => "Disabled",
    }
}

fn lifecycle_status_str(value: types::BucketLifecycleStatus) -> &'static str {
    match value {
        types::BucketLifecycleStatus::Enabled => "Enabled",
        types::BucketLifecycleStatus::Disabled => "Disabled",
    }
}

fn parse_lifecycle_status(value: &str) -> Result<types::BucketLifecycleStatus, Error> {
    match value {
        "Enabled" => Ok(types::BucketLifecycleStatus::Enabled),
        "Disabled" => Ok(types::BucketLifecycleStatus::Disabled),
        _ => Err(Error::decode("unknown bucket lifecycle rule status", None)),
    }
}

fn parse_cors_method(value: String) -> types::CorsMethod {
    match value.as_str() {
        "GET" => types::CorsMethod::Get,
        "PUT" => types::CorsMethod::Put,
        "POST" => types::CorsMethod::Post,
        "DELETE" => types::CorsMethod::Delete,
        "HEAD" => types::CorsMethod::Head,
        other => types::CorsMethod::Other(other.to_string()),
    }
}

fn parse_sse_algorithm(value: &str) -> types::SseAlgorithm {
    match value {
        "AES256" => types::SseAlgorithm::Aes256,
        "aws:kms" => types::SseAlgorithm::AwsKms,
        "aws:kms:dsse" => types::SseAlgorithm::AwsKmsDsse,
        other => types::SseAlgorithm::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_parsers_reject_unrelated_xml_roots() {
        let body = "<html><body>upstream error</body></html>";
        assert!(parse_bucket_versioning(body).is_err());
        assert!(parse_bucket_lifecycle(body).is_err());
        assert!(parse_bucket_cors(body).is_err());
        assert!(parse_bucket_tagging(body).is_err());
        assert!(parse_bucket_encryption(body).is_err());
        assert!(parse_list_buckets(body).is_err());
        assert!(parse_delete_objects(body).is_err());
        assert!(parse_copy_object(body).is_err());
    }

    #[test]
    fn listings_require_explicit_truncation_state() {
        assert!(
            parse_list_objects_v2("<ListBucketResult><Name>bucket</Name></ListBucketResult>")
                .is_err()
        );
        #[cfg(all(feature = "multipart", any(feature = "async", feature = "blocking")))]
        assert!(parse_list_parts("<ListPartsResult/>").is_err());
    }

    #[test]
    fn empty_collections_require_their_xml_containers() {
        assert!(parse_bucket_tagging("<Tagging/>").is_err());
        assert!(parse_list_buckets("<ListAllMyBucketsResult/>").is_err());
        assert!(
            parse_bucket_tagging("<Tagging><TagSet/></Tagging>")
                .unwrap()
                .tags
                .is_empty()
        );
        assert!(
            parse_list_buckets("<ListAllMyBucketsResult><Buckets/></ListAllMyBucketsResult>")
                .unwrap()
                .buckets
                .is_empty()
        );
        assert!(parse_bucket_versioning("<?xml version=\"1.0\"?><!-- empty --><s3:VersioningConfiguration xmlns:s3=\"http://s3.amazonaws.com/doc/2006-03-01/\"/>").is_ok());
    }
    use crate::types::DeleteObjectIdentifier;

    fn encoded_xml_to_string(xml: Bytes) -> String {
        String::from_utf8(xml.to_vec()).expect("XML encoder should produce valid UTF-8")
    }

    #[test]
    fn parses_error_xml() {
        let xml = r#"
<Error>
  <Code>NoSuchKey</Code>
  <Message>The specified key does not exist.</Message>
  <RequestId>req-123</RequestId>
  <HostId>host-456</HostId>
</Error>
"#;

        let err = parse_error_xml(xml).unwrap();
        assert_eq!(err.code.as_deref(), Some("NoSuchKey"));
        assert_eq!(
            err.message.as_deref(),
            Some("The specified key does not exist.")
        );
        assert_eq!(err.request_id.as_deref(), Some("req-123"));
        assert_eq!(err.host_id.as_deref(), Some("host-456"));
    }

    #[test]
    fn parses_wrapped_error_response_xml() {
        let xml = r#"
<ErrorResponse>
  <Error>
    <Code>AccessDenied</Code>
    <Message>Access denied</Message>
  </Error>
  <RequestId>req-outer</RequestId>
</ErrorResponse>
"#;

        let err = parse_error_xml(xml).expect("wrapped error response should parse");
        assert_eq!(err.code.as_deref(), Some("AccessDenied"));
        assert_eq!(err.message.as_deref(), Some("Access denied"));
        assert_eq!(err.request_id.as_deref(), Some("req-outer"));
    }

    #[test]
    fn parses_error_xml_after_declaration_and_comments() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- generated by compatible storage -->
<Error xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Code>NoSuchBucket</Code>
  <RequestId>req-xml</RequestId>
</Error>
"#;

        let err = parse_error_xml(xml).expect("root error should parse");
        assert_eq!(err.code.as_deref(), Some("NoSuchBucket"));
        assert_eq!(err.request_id.as_deref(), Some("req-xml"));
    }

    #[test]
    fn parses_error_xml_with_request_id_only() {
        let xml = r#"
<Error>
  <RequestId>req-only</RequestId>
</Error>
"#;
        let err = parse_error_xml(xml).expect("request-id-only error should parse");
        assert_eq!(err.code, None);
        assert_eq!(err.message, None);
        assert_eq!(err.request_id.as_deref(), Some("req-only"));
    }

    #[test]
    fn parses_namespaced_wrapped_errors_and_unescapes_outer_ids() {
        let body = r#"<s:ErrorResponse xmlns:s="urn:test">
            <s:Error><s:Code> AccessDenied </s:Code><s:RequestId> </s:RequestId></s:Error>
            <s:RequestId>req&amp;outer</s:RequestId><s:HostId>host&amp;outer</s:HostId>
        </s:ErrorResponse>"#;
        let error = parse_error_xml(body).unwrap();
        assert_eq!(error.code.as_deref(), Some("AccessDenied"));
        assert_eq!(error.request_id.as_deref(), Some("req&outer"));
        assert_eq!(error.host_id.as_deref(), Some("host&outer"));
    }

    #[test]
    fn error_xml_respects_comments_and_cdata() {
        let body = r#"<ErrorResponse>
            <!-- <Error><Code>FakeError</Code></Error> -->
            <Error><Code>AccessDenied</Code><Message><![CDATA[text </Error>]]></Message></Error>
        </ErrorResponse>"#;
        let error = parse_error_xml(body).unwrap();
        assert_eq!(error.code.as_deref(), Some("AccessDenied"));
        assert_eq!(error.message.as_deref(), Some("text </Error>"));
    }

    #[test]
    fn ignores_malformed_error_envelopes() {
        assert!(parse_error_xml("<ErrorResponse><Error><Code>Denied</Code></Error>").is_none());
        assert!(parse_error_xml("<Error><Code>Denied</Code></Wrong>").is_none());
    }

    #[test]
    fn ignores_non_error_xml_root() {
        let xml = r#"
<ListBucketResult>
  <Name>bucket-a</Name>
</ListBucketResult>
"#;
        assert!(parse_error_xml(xml).is_none());
    }

    #[test]
    fn ignores_nested_delete_result_errors() {
        let xml = r#"
<DeleteResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Deleted>
    <Key>a</Key>
  </Deleted>
  <Error>
    <Key>b</Key>
    <Code>AccessDenied</Code>
    <Message>Access Denied</Message>
  </Error>
</DeleteResult>
"#;
        assert!(parse_error_xml(xml).is_none());
    }

    #[test]
    fn parses_list_buckets() {
        let xml = r#"
<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Owner>
    <ID>owner-id</ID>
    <DisplayName>owner</DisplayName>
  </Owner>
  <Buckets>
    <Bucket>
      <Name>bucket-a</Name>
      <CreationDate>2020-01-01T00:00:00.000Z</CreationDate>
    </Bucket>
  </Buckets>
</ListAllMyBucketsResult>
"#;

        let out = parse_list_buckets(xml).unwrap();
        assert_eq!(out.owner.unwrap().id.as_deref(), Some("owner-id"));
        assert_eq!(out.buckets.len(), 1);
        assert_eq!(out.buckets[0].name, "bucket-a");
    }

    #[test]
    fn parses_list_objects_v2() {
        let xml = r#"
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>bucket-a</Name>
  <KeyCount>1</KeyCount>
  <MaxKeys>1000</MaxKeys>
  <IsTruncated>false</IsTruncated>
  <Contents>
    <Key>logs/app.txt</Key>
    <ETag>"etag-1"</ETag>
    <Size>7</Size>
  </Contents>
</ListBucketResult>
"#;

        let out = parse_list_objects_v2(xml).unwrap();
        assert_eq!(out.name, "bucket-a");
        assert_eq!(out.key_count, Some(1));
        assert_eq!(out.contents.len(), 1);
        assert_eq!(out.contents[0].key, "logs/app.txt");
        assert_eq!(out.contents[0].etag.as_deref(), Some("\"etag-1\""));
    }

    #[test]
    fn parses_bucket_versioning() {
        let xml = r#"
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Status>Enabled</Status>
  <MfaDelete>Disabled</MfaDelete>
</VersioningConfiguration>
"#;

        let cfg = parse_bucket_versioning(xml).unwrap();
        assert_eq!(cfg.status, Some(types::BucketVersioningStatus::Enabled));
        assert_eq!(cfg.mfa_delete, Some(types::BucketMfaDeleteStatus::Disabled));
    }

    #[test]
    fn parse_bucket_versioning_rejects_unknown_status() {
        let xml = r#"
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Status>Unknown</Status>
</VersioningConfiguration>
"#;

        assert_decode_error(parse_bucket_versioning(xml), "versioning status");
    }

    #[test]
    fn parses_bucket_lifecycle() {
        let xml = r#"
<LifecycleConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule>
    <ID>rule-1</ID>
    <Status>Enabled</Status>
    <Filter>
      <Prefix>logs/</Prefix>
    </Filter>
    <Expiration>
      <Days>30</Days>
    </Expiration>
  </Rule>
</LifecycleConfiguration>
"#;

        let cfg = parse_bucket_lifecycle(xml).unwrap();
        assert_eq!(cfg.rules.len(), 1);
        let r = &cfg.rules[0];
        assert_eq!(r.id.as_deref(), Some("rule-1"));
        assert_eq!(r.status, types::BucketLifecycleStatus::Enabled);
        assert_eq!(r.prefix.as_deref(), Some("logs/"));
        assert_eq!(r.expiration_days, Some(30));
    }

    #[test]
    fn parse_bucket_lifecycle_rejects_unknown_status() {
        let xml = r#"
<LifecycleConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule>
    <Status>Paused</Status>
  </Rule>
</LifecycleConfiguration>
"#;

        assert_decode_error(parse_bucket_lifecycle(xml), "lifecycle rule status");
    }

    #[test]
    fn parses_bucket_cors() {
        let xml = r#"
<CORSConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <CORSRule>
    <ID>rule-1</ID>
    <AllowedOrigin>*</AllowedOrigin>
    <AllowedMethod>GET</AllowedMethod>
    <AllowedMethod>PATCH</AllowedMethod>
    <AllowedHeader>*</AllowedHeader>
    <ExposeHeader>ETag</ExposeHeader>
    <MaxAgeSeconds>3000</MaxAgeSeconds>
  </CORSRule>
</CORSConfiguration>
"#;

        let cfg = parse_bucket_cors(xml).unwrap();
        assert_eq!(cfg.rules.len(), 1);
        let r = &cfg.rules[0];
        assert_eq!(r.id.as_deref(), Some("rule-1"));
        assert_eq!(r.allowed_origins, vec!["*".to_string()]);
        assert_eq!(
            r.allowed_methods,
            vec![
                types::CorsMethod::Get,
                types::CorsMethod::Other("PATCH".to_string())
            ]
        );
        assert_eq!(r.allowed_headers, vec!["*".to_string()]);
        assert_eq!(r.expose_headers, vec!["ETag".to_string()]);
        assert_eq!(r.max_age_seconds, Some(3000));
    }

    #[test]
    fn parses_bucket_tagging() {
        let xml = r#"
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet>
    <Tag>
      <Key>k</Key>
      <Value>v</Value>
    </Tag>
  </TagSet>
</Tagging>
"#;

        let cfg = parse_bucket_tagging(xml).unwrap();
        assert_eq!(cfg.tags.len(), 1);
        assert_eq!(cfg.tags[0].key, "k");
        assert_eq!(cfg.tags[0].value, "v");
    }

    #[test]
    fn parses_bucket_encryption() {
        let xml = r#"
<ServerSideEncryptionConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule>
    <ApplyServerSideEncryptionByDefault>
      <SSEAlgorithm>aws:kms</SSEAlgorithm>
      <KMSMasterKeyID>key-id</KMSMasterKeyID>
    </ApplyServerSideEncryptionByDefault>
    <BucketKeyEnabled>true</BucketKeyEnabled>
  </Rule>
</ServerSideEncryptionConfiguration>
"#;

        let cfg = parse_bucket_encryption(xml).unwrap();
        assert_eq!(cfg.rules.len(), 1);
        assert_eq!(
            cfg.rules[0].apply.sse_algorithm,
            types::SseAlgorithm::AwsKms
        );
        assert_eq!(
            cfg.rules[0].apply.kms_master_key_id.as_deref(),
            Some("key-id")
        );
        assert_eq!(cfg.rules[0].bucket_key_enabled, Some(true));
    }

    #[test]
    fn parse_bucket_encryption_rejects_missing_apply() {
        let xml = r#"
<ServerSideEncryptionConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule>
    <BucketKeyEnabled>true</BucketKeyEnabled>
  </Rule>
</ServerSideEncryptionConfiguration>
"#;

        assert_decode_error(
            parse_bucket_encryption(xml),
            "ApplyServerSideEncryptionByDefault",
        );
    }

    #[test]
    fn parses_bucket_public_access_block() {
        let xml = r#"
<PublicAccessBlockConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <BlockPublicAcls>true</BlockPublicAcls>
  <IgnorePublicAcls>false</IgnorePublicAcls>
  <BlockPublicPolicy>true</BlockPublicPolicy>
  <RestrictPublicBuckets>false</RestrictPublicBuckets>
</PublicAccessBlockConfiguration>
"#;

        let cfg = parse_bucket_public_access_block(xml).unwrap();
        assert!(cfg.block_public_acls);
        assert!(!cfg.ignore_public_acls);
        assert!(cfg.block_public_policy);
        assert!(!cfg.restrict_public_buckets);
    }

    #[test]
    fn parse_bucket_public_access_block_rejects_missing_fields() {
        let xml = r#"
<PublicAccessBlockConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <BlockPublicAcls>true</BlockPublicAcls>
  <BlockPublicPolicy>true</BlockPublicPolicy>
  <RestrictPublicBuckets>false</RestrictPublicBuckets>
</PublicAccessBlockConfiguration>
"#;

        assert_decode_error(parse_bucket_public_access_block(xml), "IgnorePublicAcls");
    }

    #[test]
    fn parses_delete_objects() {
        let xml = r#"
<DeleteResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Deleted>
    <Key>a</Key>
  </Deleted>
  <Error>
    <Key>b</Key>
    <Code>AccessDenied</Code>
    <Message>Access Denied</Message>
  </Error>
</DeleteResult>
"#;

        let out = parse_delete_objects(xml).unwrap();
        assert_eq!(out.deleted.len(), 1);
        assert_eq!(out.deleted[0].key.as_deref(), Some("a"));
        assert_eq!(out.errors.len(), 1);
        assert_eq!(out.errors[0].key.as_deref(), Some("b"));
        assert_eq!(out.errors[0].code.as_deref(), Some("AccessDenied"));
    }

    #[test]
    fn parses_copy_object() {
        let xml = r#"
<CopyObjectResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <LastModified>2020-01-01T00:00:00.000Z</LastModified>
  <ETag>"etag"</ETag>
</CopyObjectResult>
"#;

        let out = parse_copy_object(xml).unwrap();
        assert_eq!(out.etag.as_deref(), Some("\"etag\""));
        assert_eq!(
            out.last_modified.as_deref(),
            Some("2020-01-01T00:00:00.000Z")
        );
    }

    #[test]
    fn encodes_delete_objects_request() {
        let objects = vec![
            DeleteObjectIdentifier::new("a.txt").unwrap(),
            DeleteObjectIdentifier::new("b.txt")
                .unwrap()
                .with_version_id("v1")
                .unwrap(),
        ];
        let xml = encode_delete_objects(&objects, true).unwrap();
        let xml = encoded_xml_to_string(xml);

        assert!(xml.contains("<Delete"));
        assert!(xml.contains("<Quiet>true</Quiet>"));
        assert!(xml.contains("<Key>a.txt</Key>"));
        assert!(xml.contains("<Key>b.txt</Key>"));
        assert!(xml.contains("<VersionId>v1</VersionId>"));
    }

    #[test]
    fn encode_delete_objects_rejects_oversized_batches() {
        let objects = (0..=types::MAX_DELETE_OBJECTS_PER_REQUEST)
            .map(|idx| DeleteObjectIdentifier::new(format!("key-{idx}")))
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        let err = encode_delete_objects(&objects, false)
            .expect_err("delete_objects must reject batches over 1000 objects");

        match err {
            Error::InvalidConfig { message } => assert!(message.contains("at most 1000")),
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[test]
    fn delete_object_identifier_rejects_invalid_values() {
        assert_invalid_config(DeleteObjectIdentifier::new(""), "object key");
        assert_invalid_config(DeleteObjectIdentifier::new("a/../b"), "path segments");
        assert_invalid_config(
            DeleteObjectIdentifier::new("key")
                .unwrap()
                .with_version_id(" version"),
            "version_id",
        );
    }

    #[test]
    fn encodes_bucket_versioning() {
        let cfg = types::BucketVersioningConfiguration {
            status: Some(types::BucketVersioningStatus::Enabled),
            mfa_delete: Some(types::BucketMfaDeleteStatus::Disabled),
        };
        let xml = encode_bucket_versioning(&cfg).unwrap();
        let xml = encoded_xml_to_string(xml);
        assert!(xml.contains("<VersioningConfiguration"));
        assert!(xml.contains("xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\""));
        assert!(xml.contains("<Status>Enabled</Status>"));
        assert!(xml.contains("<MfaDelete>Disabled</MfaDelete>"));
    }

    #[test]
    fn encodes_create_bucket_configuration() {
        let xml = encode_create_bucket_configuration("eu-central-1").unwrap();
        let xml = encoded_xml_to_string(xml);
        assert!(xml.contains("<CreateBucketConfiguration"));
        assert!(xml.contains("xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\""));
        assert!(xml.contains("<LocationConstraint>eu-central-1</LocationConstraint>"));

        assert_invalid_config(
            encode_create_bucket_configuration(" eu-central-1"),
            "whitespace",
        );
        assert_invalid_config(encode_create_bucket_configuration("eu central 1"), "region");
    }

    #[test]
    fn encodes_bucket_lifecycle() {
        let cfg = types::BucketLifecycleConfiguration {
            rules: vec![types::BucketLifecycleRule {
                id: Some("rule-1".to_string()),
                status: types::BucketLifecycleStatus::Enabled,
                prefix: Some("logs/".to_string()),
                expiration_days: Some(30),
                expiration_date: None,
            }],
        };

        let xml = encode_bucket_lifecycle(&cfg).unwrap();
        let xml = encoded_xml_to_string(xml);
        assert!(xml.contains("<LifecycleConfiguration"));
        assert!(xml.contains("<Rule>"));
        assert!(xml.contains("<ID>rule-1</ID>"));
        assert!(xml.contains("<Status>Enabled</Status>"));
        assert!(xml.contains("<Prefix>logs/</Prefix>"));
        assert!(xml.contains("<Days>30</Days>"));
    }

    #[test]
    fn encode_bucket_lifecycle_rejects_invalid_expiration() {
        let mut rule = types::BucketLifecycleRule {
            id: None,
            status: types::BucketLifecycleStatus::Enabled,
            prefix: Some("logs/".to_string()),
            expiration_days: None,
            expiration_date: None,
        };

        let cfg = types::BucketLifecycleConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_lifecycle(&cfg), "must include an expiration");

        rule.expiration_days = Some(0);
        let cfg = types::BucketLifecycleConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_lifecycle(&cfg), "greater than 0");

        rule.expiration_days = Some(30);
        rule.expiration_date = Some("2026-01-01T00:00:00Z".to_string());
        let cfg = types::BucketLifecycleConfiguration { rules: vec![rule] };
        assert_invalid_config(encode_bucket_lifecycle(&cfg), "either days or date");
    }

    #[test]
    fn encode_bucket_lifecycle_rejects_invalid_rule_ids_and_dates() {
        let mut rule = types::BucketLifecycleRule {
            id: Some(" ".to_string()),
            status: types::BucketLifecycleStatus::Enabled,
            prefix: Some("logs/".to_string()),
            expiration_days: Some(30),
            expiration_date: None,
        };
        let cfg = types::BucketLifecycleConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_lifecycle(&cfg), "rule id");

        rule.id = Some("rule-a".to_string());
        rule.expiration_days = None;
        rule.expiration_date = Some(" ".to_string());
        let cfg = types::BucketLifecycleConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_lifecycle(&cfg), "expiration_date");

        rule.expiration_date = Some("2026-01-01".to_string());
        let cfg = types::BucketLifecycleConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_lifecycle(&cfg), "RFC3339");

        rule.expiration_date = Some("2026-01-01T00:00:00Z".to_string());
        let cfg = types::BucketLifecycleConfiguration {
            rules: vec![rule.clone(), rule],
        };
        assert_invalid_config(encode_bucket_lifecycle(&cfg), "unique");
    }

    #[test]
    fn encodes_bucket_cors() {
        let cfg = types::BucketCorsConfiguration {
            rules: vec![types::BucketCorsRule {
                id: Some("rule-1".to_string()),
                allowed_origins: vec!["*".to_string()],
                allowed_methods: vec![types::CorsMethod::Get, types::CorsMethod::Put],
                allowed_headers: vec!["*".to_string(), "x-amz-*".to_string()],
                expose_headers: vec!["ETag".to_string()],
                max_age_seconds: Some(3000),
            }],
        };

        let xml = encode_bucket_cors(&cfg).unwrap();
        let xml = encoded_xml_to_string(xml);
        assert!(xml.contains("<CORSConfiguration"));
        assert!(xml.contains("<AllowedOrigin>*</AllowedOrigin>"));
        assert!(xml.contains("<AllowedMethod>GET</AllowedMethod>"));
        assert!(xml.contains("<AllowedMethod>PUT</AllowedMethod>"));
        assert!(xml.contains("<AllowedHeader>x-amz-*</AllowedHeader>"));
    }

    #[test]
    fn encode_bucket_cors_rejects_empty_required_lists() {
        let cfg = types::BucketCorsConfiguration {
            rules: vec![types::BucketCorsRule {
                id: None,
                allowed_origins: Vec::new(),
                allowed_methods: vec![types::CorsMethod::Get],
                allowed_headers: Vec::new(),
                expose_headers: Vec::new(),
                max_age_seconds: None,
            }],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "allowed origin");

        let cfg = types::BucketCorsConfiguration {
            rules: vec![types::BucketCorsRule {
                id: None,
                allowed_origins: vec!["*".to_string()],
                allowed_methods: Vec::new(),
                allowed_headers: Vec::new(),
                expose_headers: Vec::new(),
                max_age_seconds: None,
            }],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "allowed method");
    }

    #[test]
    fn encode_bucket_cors_rejects_malformed_values() {
        let mut rule = types::BucketCorsRule {
            id: None,
            allowed_origins: vec![" https://example.com".to_string()],
            allowed_methods: vec![types::CorsMethod::Get],
            allowed_headers: Vec::new(),
            expose_headers: Vec::new(),
            max_age_seconds: None,
        };
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "allowed origins");

        rule.allowed_origins = vec!["https://bad host.example".to_string()];
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "allowed origins");

        rule.allowed_origins = vec!["https://*.bad.*.example".to_string()];
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "wildcard");

        rule.allowed_origins = vec!["https://example.com".to_string()];
        rule.allowed_headers = vec![" X-Test".to_string()];
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "header names");

        rule.allowed_headers = vec!["Bad Header".to_string()];
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "valid HTTP header");

        rule.allowed_headers = vec!["x-*-*".to_string()];
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "wildcard");

        rule.allowed_headers = vec!["X-Test".to_string(), "x-test".to_string()];
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "unique");

        rule.allowed_headers.clear();
        rule.expose_headers = vec!["*".to_string()];
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "expose headers");

        rule.expose_headers.clear();
        rule.allowed_methods = vec![
            types::CorsMethod::Get,
            types::CorsMethod::Other("GET".to_string()),
        ];
        let cfg = types::BucketCorsConfiguration {
            rules: vec![rule.clone()],
        };
        assert_invalid_config(encode_bucket_cors(&cfg), "unique");

        rule.allowed_headers.clear();
        rule.allowed_methods = vec![types::CorsMethod::Other(" GET".to_string())];
        let cfg = types::BucketCorsConfiguration { rules: vec![rule] };
        assert_invalid_config(encode_bucket_cors(&cfg), "allowed methods");
    }

    #[test]
    fn encodes_bucket_tagging() {
        let cfg = types::BucketTagging {
            tags: vec![types::Tag {
                key: "k".to_string(),
                value: "v".to_string(),
            }],
        };
        let xml = encode_bucket_tagging(&cfg).unwrap();
        let xml = encoded_xml_to_string(xml);
        assert!(xml.contains("<Tagging"));
        assert!(xml.contains("<Key>k</Key>"));
        assert!(xml.contains("<Value>v</Value>"));
    }

    #[test]
    fn encode_bucket_tagging_rejects_invalid_tags() {
        let cfg = types::BucketTagging {
            tags: vec![types::Tag {
                key: " ".to_string(),
                value: "v".to_string(),
            }],
        };
        assert_invalid_config(encode_bucket_tagging(&cfg), "tag key");

        let cfg = types::BucketTagging {
            tags: vec![types::Tag {
                key: "key\u{7f}".to_string(),
                value: "v".to_string(),
            }],
        };
        assert_invalid_config(encode_bucket_tagging(&cfg), "control");

        let cfg = types::BucketTagging {
            tags: vec![types::Tag {
                key: "k".to_string(),
                value: " v".to_string(),
            }],
        };
        assert_invalid_config(encode_bucket_tagging(&cfg), "tag value");

        let cfg = types::BucketTagging {
            tags: (0..51)
                .map(|idx| types::Tag {
                    key: format!("k-{idx}"),
                    value: "v".to_string(),
                })
                .collect(),
        };
        assert_invalid_config(encode_bucket_tagging(&cfg), "at most 50");

        let cfg = types::BucketTagging {
            tags: vec![
                types::Tag {
                    key: "k".to_string(),
                    value: "v1".to_string(),
                },
                types::Tag {
                    key: "k".to_string(),
                    value: "v2".to_string(),
                },
            ],
        };
        assert_invalid_config(encode_bucket_tagging(&cfg), "unique");
    }

    #[test]
    fn encodes_bucket_encryption() {
        let cfg = types::BucketEncryptionConfiguration {
            rules: vec![types::BucketEncryptionRule {
                apply: types::ApplyServerSideEncryptionByDefault {
                    sse_algorithm: types::SseAlgorithm::AwsKms,
                    kms_master_key_id: Some("key-id".to_string()),
                },
                bucket_key_enabled: Some(true),
            }],
        };
        let xml = encode_bucket_encryption(&cfg).unwrap();
        let xml = encoded_xml_to_string(xml);
        assert!(xml.contains("<ServerSideEncryptionConfiguration"));
        assert!(xml.contains("<SSEAlgorithm>aws:kms</SSEAlgorithm>"));
        assert!(xml.contains("<KMSMasterKeyID>key-id</KMSMasterKeyID>"));
        assert!(xml.contains("<BucketKeyEnabled>true</BucketKeyEnabled>"));
    }

    #[test]
    fn encode_bucket_encryption_rejects_kms_key_with_aes256() {
        let cfg = types::BucketEncryptionConfiguration {
            rules: vec![types::BucketEncryptionRule {
                apply: types::ApplyServerSideEncryptionByDefault {
                    sse_algorithm: types::SseAlgorithm::Aes256,
                    kms_master_key_id: Some("key-id".to_string()),
                },
                bucket_key_enabled: None,
            }],
        };

        assert_invalid_config(encode_bucket_encryption(&cfg), "AWS KMS algorithm");
    }

    #[test]
    fn encode_bucket_encryption_rejects_malformed_values() {
        let cfg = types::BucketEncryptionConfiguration {
            rules: vec![types::BucketEncryptionRule {
                apply: types::ApplyServerSideEncryptionByDefault {
                    sse_algorithm: types::SseAlgorithm::Other(" ".to_string()),
                    kms_master_key_id: None,
                },
                bucket_key_enabled: None,
            }],
        };
        assert_invalid_config(encode_bucket_encryption(&cfg), "SSE algorithm");

        let cfg = types::BucketEncryptionConfiguration {
            rules: vec![types::BucketEncryptionRule {
                apply: types::ApplyServerSideEncryptionByDefault {
                    sse_algorithm: types::SseAlgorithm::AwsKms,
                    kms_master_key_id: Some(" ".to_string()),
                },
                bucket_key_enabled: Some(true),
            }],
        };
        assert_invalid_config(encode_bucket_encryption(&cfg), "KMS master key id");
    }

    #[test]
    fn encodes_bucket_public_access_block() {
        let cfg = types::BucketPublicAccessBlockConfiguration {
            block_public_acls: true,
            ignore_public_acls: false,
            block_public_policy: true,
            restrict_public_buckets: false,
        };
        let xml = encode_bucket_public_access_block(&cfg).unwrap();
        let xml = encoded_xml_to_string(xml);
        assert!(xml.contains("<PublicAccessBlockConfiguration"));
        assert!(xml.contains("<BlockPublicAcls>true</BlockPublicAcls>"));
        assert!(xml.contains("<IgnorePublicAcls>false</IgnorePublicAcls>"));
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn encodes_complete_multipart_upload() {
        let parts = vec![
            types::CompletedPart::new(1, "\"etag1\"").unwrap(),
            types::CompletedPart::new(2, "\"etag2\"").unwrap(),
        ];
        let xml = encode_complete_multipart_upload(&parts).unwrap();
        let xml = encoded_xml_to_string(xml);
        assert!(xml.contains("<CompleteMultipartUpload"));
        assert!(xml.contains("<PartNumber>1</PartNumber>"));
        assert!(xml.contains("<ETag>\"etag1\"</ETag>"));
    }

    fn assert_decode_error<T>(result: Result<T, Error>, expected: &str) {
        match result {
            Err(Error::Decode { message, .. }) => assert!(
                message.contains(expected),
                "expected error message to contain {expected:?}, got {message:?}",
            ),
            Err(other) => panic!("expected Decode error, got {other:?}"),
            Ok(_) => panic!("expected Decode error"),
        }
    }

    fn assert_invalid_config<T>(result: Result<T, Error>, expected: &str) {
        match result {
            Err(Error::InvalidConfig { message }) => assert!(
                message.contains(expected),
                "expected error message to contain {expected:?}, got {message:?}",
            ),
            Err(other) => panic!("expected InvalidConfig, got {other:?}"),
            Ok(_) => panic!("expected InvalidConfig"),
        }
    }
}
