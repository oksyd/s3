use crate::util::validation::validate_object_key;
use url::{Host, Url};

use crate::{auth::AddressingStyle, error::Error};

#[derive(Debug)]
pub(crate) struct ResolvedUrl {
    pub(crate) url: Url,
    pub(crate) canonical_uri: String,
    pub(crate) canonical_query_string: String,
}

pub(crate) fn resolve_url(
    base_url: &Url,
    bucket: Option<&str>,
    key: Option<&str>,
    query_params: &[(String, String)],
    addressing: AddressingStyle,
) -> Result<ResolvedUrl, Error> {
    if !base_url.username().is_empty() || base_url.password().is_some() {
        return Err(Error::invalid_config("endpoint must not include user info"));
    }

    let mut url = base_url.clone();

    let canonical_query_string = crate::util::encode::canonical_query_string(query_params);

    if canonical_query_string.is_empty() {
        url.set_query(None);
    } else {
        url.set_query(Some(&canonical_query_string));
    }

    let Some(bucket) = bucket else {
        url.set_path("/");
        return Ok(ResolvedUrl {
            url,
            canonical_uri: "/".to_string(),
            canonical_query_string,
        });
    };
    validate_bucket_name(bucket)?;
    if let Some(key) = key {
        validate_object_key(key)?;
    }

    let host = base_url
        .host_str()
        .ok_or_else(|| Error::invalid_config("endpoint must include host"))?;

    let virtual_hosted = use_virtual_hosted_style(base_url, bucket, addressing);

    let (final_host, raw_path) = match virtual_hosted {
        false => {
            let raw_path = match key {
                Some(key) => format!("/{bucket}/{key}"),
                None => format!("/{bucket}"),
            };
            (None, raw_path)
        }
        true => {
            if endpoint_has_ip_host(base_url) {
                return Err(Error::invalid_config(
                    "virtual-hosted-style requires a domain endpoint host",
                ));
            }
            if !is_dns_compatible_bucket(bucket) {
                return Err(Error::invalid_config(
                    "bucket is not DNS compatible for virtual-hosted-style",
                ));
            }
            let raw_path = match key {
                Some(key) => format!("/{key}"),
                _ => "/".to_string(),
            };
            (Some(format!("{bucket}.{host}")), raw_path)
        }
    };

    let canonical_uri = crate::util::encode::aws_percent_encode_path(&raw_path);

    url.set_path(&canonical_uri);
    if let Some(final_host) = final_host {
        url.set_host(Some(&final_host))
            .map_err(|_| Error::invalid_config("invalid endpoint host"))?;
    }

    Ok(ResolvedUrl {
        url,
        canonical_uri,
        canonical_query_string,
    })
}

fn use_virtual_hosted_style(base_url: &Url, bucket: &str, addressing: AddressingStyle) -> bool {
    match addressing {
        AddressingStyle::Path => false,
        AddressingStyle::VirtualHosted => true,
        AddressingStyle::Auto => {
            !endpoint_requires_path_style(base_url)
                && !(base_url.scheme() == "https" && bucket.contains('.'))
                && is_dns_compatible_bucket(bucket)
        }
    }
}

fn endpoint_requires_path_style(base_url: &Url) -> bool {
    match base_url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(_) | Host::Ipv6(_)) => true,
        None => false,
    }
}

fn endpoint_has_ip_host(base_url: &Url) -> bool {
    matches!(base_url.host(), Some(Host::Ipv4(_) | Host::Ipv6(_)))
}

fn validate_bucket_name(bucket: &str) -> Result<(), Error> {
    if bucket.is_empty() {
        return Err(Error::invalid_config("bucket must not be empty"));
    }
    if bucket.trim() != bucket {
        return Err(Error::invalid_config(
            "bucket must not include leading or trailing whitespace",
        ));
    }
    if bucket.contains('/') {
        return Err(Error::invalid_config("bucket must not contain '/'"));
    }
    if bucket
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(Error::invalid_config(
            "bucket must not contain ASCII control or whitespace characters",
        ));
    }
    Ok(())
}

fn is_dns_compatible_bucket(bucket: &str) -> bool {
    let bytes = bucket.as_bytes();
    if bytes.len() < 3 || bytes.len() > 63 {
        return false;
    }

    if bytes.iter().any(|b| b.is_ascii_uppercase()) {
        return false;
    }

    let is_allowed = |b: u8| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.');
    if !bytes.iter().all(|&b| is_allowed(b)) {
        return false;
    }

    let starts_ok = matches!(bytes[0], b'a'..=b'z' | b'0'..=b'9');
    let ends_ok = matches!(bytes[bytes.len() - 1], b'a'..=b'z' | b'0'..=b'9');
    if !starts_ok || !ends_ok {
        return false;
    }

    if bucket.contains("..") {
        return false;
    }

    if bucket
        .split('.')
        .any(|label| label.starts_with('-') || label.ends_with('-'))
    {
        return false;
    }

    if bucket.parse::<std::net::IpAddr>().is_ok() {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AddressingStyle;

    #[test]
    fn resolves_path_style_url_and_does_not_double_encode() {
        let base = Url::parse("https://example.com").unwrap();
        let resolved = resolve_url(
            &base,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        assert_eq!(resolved.canonical_uri, "/my-bucket/a%2Bb");
        assert_eq!(resolved.url.as_str(), "https://example.com/my-bucket/a%2Bb");
    }

    #[test]
    fn resolves_virtual_hosted_style_url() {
        let base = Url::parse("https://s3.example.com").unwrap();
        let resolved = resolve_url(
            &base,
            Some("mybucket"),
            Some("a+b"),
            &[],
            AddressingStyle::VirtualHosted,
        )
        .unwrap();

        assert_eq!(resolved.url.host_str().unwrap(), "mybucket.s3.example.com");
        assert_eq!(resolved.canonical_uri, "/a%2Bb");
    }

    #[test]
    fn auto_falls_back_to_path_style_for_dot_bucket_on_https() {
        let base = Url::parse("https://s3.example.com").unwrap();
        let resolved = resolve_url(
            &base,
            Some("bucket.with.dots"),
            Some("key"),
            &[],
            AddressingStyle::Auto,
        )
        .unwrap();

        assert_eq!(resolved.url.host_str().unwrap(), "s3.example.com");
        assert_eq!(resolved.canonical_uri, "/bucket.with.dots/key");
    }

    #[test]
    fn auto_falls_back_to_path_style_for_ip_endpoints() {
        for endpoint in ["http://127.0.0.1:9000", "http://[::1]:9000"] {
            let base = Url::parse(endpoint).unwrap();
            let resolved = resolve_url(
                &base,
                Some("mybucket"),
                Some("key"),
                &[],
                AddressingStyle::Auto,
            )
            .unwrap();

            assert_eq!(resolved.canonical_uri, "/mybucket/key");
            assert_eq!(resolved.url.path(), "/mybucket/key");
            assert_eq!(resolved.url.host(), base.host());
        }
    }

    #[test]
    fn virtual_hosted_style_allows_localhost_domain_endpoints() {
        let base = Url::parse("http://localhost:9000").unwrap();
        let resolved = resolve_url(
            &base,
            Some("mybucket"),
            Some("key"),
            &[],
            AddressingStyle::VirtualHosted,
        )
        .unwrap();

        assert_eq!(resolved.url.host_str().unwrap(), "mybucket.localhost");
        assert_eq!(resolved.canonical_uri, "/key");
    }

    #[test]
    fn virtual_hosted_style_rejects_ip_endpoints() {
        for endpoint in ["http://127.0.0.1:9000", "http://[::1]:9000"] {
            let base = Url::parse(endpoint).unwrap();
            let err = resolve_url(
                &base,
                Some("mybucket"),
                Some("key"),
                &[],
                AddressingStyle::VirtualHosted,
            )
            .expect_err("virtual-hosted-style must require a domain endpoint");

            assert_invalid_config_contains(err, "domain endpoint host");
        }
    }

    #[test]
    fn virtual_hosted_style_rejects_invalid_dns_labels() {
        let base = Url::parse("https://s3.example.com").unwrap();
        for bucket in ["bad-.bucket", "bad.-bucket"] {
            let err = resolve_url(
                &base,
                Some(bucket),
                Some("key"),
                &[],
                AddressingStyle::VirtualHosted,
            )
            .expect_err("invalid DNS labels must be rejected");

            assert_invalid_config_contains(err, "DNS compatible");
        }
    }

    #[test]
    fn path_encoding_preserves_slash_in_key() {
        let base = Url::parse("https://example.com").unwrap();
        let resolved = resolve_url(
            &base,
            Some("my-bucket"),
            Some("a/b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        assert_eq!(resolved.canonical_uri, "/my-bucket/a/b");
    }

    #[test]
    fn query_params_are_canonicalized_and_applied_to_url() {
        let base = Url::parse("https://example.com").unwrap();
        let resolved = resolve_url(
            &base,
            Some("my-bucket"),
            Some("key"),
            &[
                ("b".to_string(), "2".to_string()),
                ("a".to_string(), "".to_string()),
            ],
            AddressingStyle::Path,
        )
        .unwrap();

        assert_eq!(resolved.canonical_query_string, "a=&b=2");
        assert_eq!(resolved.url.query().unwrap_or(""), "a=&b=2");
    }

    #[test]
    fn empty_bucket_is_rejected() {
        let base = Url::parse("https://example.com").unwrap();
        let err = match resolve_url(&base, Some(""), Some("key"), &[], AddressingStyle::Path) {
            Ok(_) => panic!("empty bucket should be rejected"),
            Err(err) => err,
        };
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("bucket must not be empty"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[test]
    fn empty_object_key_is_rejected() {
        let base = Url::parse("https://example.com").unwrap();
        let err = match resolve_url(
            &base,
            Some("my-bucket"),
            Some(""),
            &[],
            AddressingStyle::Path,
        ) {
            Ok(_) => panic!("empty object key should be rejected"),
            Err(err) => err,
        };

        assert_invalid_config_contains(err, "object key");
    }

    #[test]
    fn object_key_dot_segments_are_rejected() {
        let base = Url::parse("https://example.com").unwrap();
        for key in [".", "..", "a/./b", "a/../b"] {
            let err = resolve_url(
                &base,
                Some("my-bucket"),
                Some(key),
                &[],
                AddressingStyle::Path,
            )
            .expect_err("period-only key segments must be rejected");

            assert_invalid_config_contains(err, "path segments");
        }
    }

    #[test]
    fn object_key_control_characters_are_rejected() {
        let base = Url::parse("https://example.com").unwrap();
        for key in ["line\nbreak", "bad\u{7f}"] {
            let err = resolve_url(
                &base,
                Some("my-bucket"),
                Some(key),
                &[],
                AddressingStyle::Path,
            )
            .expect_err("control-character key should be rejected");

            assert_invalid_config_contains(err, "control");
        }
    }

    #[test]
    fn malformed_bucket_names_are_rejected() {
        let base = Url::parse("https://example.com").unwrap();
        let cases = [
            (" bucket", "whitespace"),
            ("bucket ", "whitespace"),
            ("buck et", "whitespace"),
            ("buck\tet", "whitespace"),
            ("a/b", "'/'"),
        ];

        for (bucket, expected) in cases {
            let err =
                match resolve_url(&base, Some(bucket), Some("key"), &[], AddressingStyle::Path) {
                    Ok(_) => panic!("malformed bucket should be rejected"),
                    Err(err) => err,
                };

            assert_invalid_config_contains(err, expected);
        }
    }

    #[test]
    fn endpoint_with_user_info_is_rejected() {
        let base = Url::parse("https://user:pass@example.com").unwrap();
        let err = match resolve_url(
            &base,
            Some("my-bucket"),
            Some("key"),
            &[],
            AddressingStyle::Path,
        ) {
            Ok(_) => panic!("endpoint with user info should be rejected"),
            Err(err) => err,
        };
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("must not include user info"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    fn assert_invalid_config_contains(err: Error, expected: &str) {
        match err {
            Error::InvalidConfig { message } => assert!(
                message.contains(expected),
                "expected error message to contain {expected:?}, got {message:?}",
            ),
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }
}
