use http::{
    HeaderMap,
    header::{AsHeaderName, HeaderName},
};

use crate::{
    Error, Result,
    util::validation::{validate_object_key, validate_version_id},
};

pub(crate) fn header_string<N>(headers: &HeaderMap, name: N) -> Option<String>
where
    N: AsHeaderName,
{
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
}

pub(crate) fn header_u64<N>(headers: &HeaderMap, name: N) -> Option<u64>
where
    N: AsHeaderName,
{
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
}

pub(crate) fn metadata_header_name(value: &str) -> Result<HeaderName> {
    if value.is_empty() {
        return Err(Error::invalid_config("metadata key must not be empty"));
    }
    if value.trim() != value {
        return Err(Error::invalid_config(
            "metadata key must not include leading or trailing whitespace",
        ));
    }

    let mut name = String::with_capacity("x-amz-meta-".len() + value.len());
    name.push_str("x-amz-meta-");
    name.push_str(value);

    HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| Error::invalid_config("invalid metadata key for x-amz-meta-* header"))
}

pub(crate) fn copy_source_header_value(
    bucket: &str,
    key: &str,
    version_id: Option<&str>,
) -> Result<String> {
    validate_copy_source_bucket(bucket)?;
    validate_object_key(key)?;
    let bucket_enc = crate::util::encode::aws_percent_encode(bucket);
    let key_enc = crate::util::encode::aws_percent_encode_path(key);

    let value = match version_id {
        Some(v) => {
            validate_version_id(v)?;
            let version_enc = crate::util::encode::aws_percent_encode(v);
            format!("/{bucket_enc}/{key_enc}?versionId={version_enc}")
        }
        None => format!("/{bucket_enc}/{key_enc}"),
    };

    Ok(value)
}

fn validate_copy_source_bucket(bucket: &str) -> Result<()> {
    if bucket.is_empty() {
        return Err(Error::invalid_config(
            "copy source bucket must not be empty",
        ));
    }
    if bucket.trim() != bucket {
        return Err(Error::invalid_config(
            "copy source bucket must not include leading or trailing whitespace",
        ));
    }
    if bucket.contains('/') {
        return Err(Error::invalid_config(
            "copy source bucket must not contain '/'",
        ));
    }
    if bucket
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(Error::invalid_config(
            "copy source bucket must not contain ASCII control or whitespace characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_metadata_header_name() {
        let name = metadata_header_name("Foo").unwrap();
        assert_eq!(name.as_str(), "x-amz-meta-foo");

        assert!(metadata_header_name("").is_err());
        assert!(metadata_header_name(" Foo ").is_err());
        assert!(metadata_header_name("a b").is_err());
    }

    #[test]
    fn extracts_string_and_u64_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("etag", "\"abc\"".parse().unwrap());
        headers.insert("content-length", "42".parse().unwrap());

        assert_eq!(header_string(&headers, "etag").as_deref(), Some("\"abc\""));
        assert_eq!(header_u64(&headers, "content-length"), Some(42));
    }

    #[test]
    fn copy_source_header_value_encodes_bucket_key_and_version() {
        assert_eq!(
            copy_source_header_value("bucket-name", "dir/file name.txt", Some("v 1")).unwrap(),
            "/bucket-name/dir/file%20name.txt?versionId=v%201"
        );
    }

    #[test]
    fn copy_source_header_value_rejects_invalid_version_id() {
        assert!(copy_source_header_value("bucket", "key", Some("")).is_err());
        assert!(copy_source_header_value("bucket", "key", Some(" version")).is_err());
        assert!(copy_source_header_value("bucket", "key", Some("version ")).is_err());
        assert!(copy_source_header_value("bucket", "key", Some("ver\nsion")).is_err());
    }

    #[test]
    fn copy_source_header_value_rejects_invalid_bucket_segment() {
        assert!(copy_source_header_value("", "key", None).is_err());
        assert!(copy_source_header_value(" bucket", "key", None).is_err());
        assert!(copy_source_header_value("bucket ", "key", None).is_err());
        assert!(copy_source_header_value("buck et", "key", None).is_err());
        assert!(copy_source_header_value("buck\tet", "key", None).is_err());
        assert!(copy_source_header_value("bucket/child", "key", None).is_err());
    }

    #[test]
    fn copy_source_header_value_rejects_empty_key() {
        assert!(copy_source_header_value("bucket", "", None).is_err());
        assert!(copy_source_header_value("bucket", "a/../b", None).is_err());
    }
}
