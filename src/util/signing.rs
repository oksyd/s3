use graviola::hashing::{Hash as _, HashOutput, Sha256, hmac::Hmac};
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use time::OffsetDateTime;

use crate::{
    auth::{Credentials, Region},
    error::Error,
    types::PresignedRequest,
    util::url::ResolvedUrl,
};

type HmacSha256 = Hmac<Sha256>;

pub(crate) const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
pub(crate) const EMPTY_PAYLOAD_HASH: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const DEFAULT_SERVICE: &str = "s3";

#[derive(Clone, Copy)]
pub(crate) struct SigV4Params<'a> {
    region: &'a Region,
    service: &'a str,
    credentials: &'a Credentials,
    now: OffsetDateTime,
}

impl<'a> SigV4Params<'a> {
    pub(crate) fn new(
        region: &'a Region,
        service: &'a str,
        credentials: &'a Credentials,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            region,
            service,
            credentials,
            now,
        }
    }

    pub(crate) fn for_s3(
        region: &'a Region,
        credentials: &'a Credentials,
        now: OffsetDateTime,
    ) -> Self {
        Self::new(region, DEFAULT_SERVICE, credentials, now)
    }
}

pub(crate) fn sign_headers(
    method: &Method,
    resolved: &ResolvedUrl,
    headers: &mut HeaderMap,
    payload_hash: &str,
    params: SigV4Params<'_>,
) -> Result<(), Error> {
    set_amz_headers(headers, payload_hash, params.credentials, params.now)?;

    let host_header_value = host_header_value(&resolved.url)?;
    headers.insert(http::header::HOST, host_header_value);

    let (canonical_headers, signed_headers) = canonicalize_headers(headers)?;

    let canonical_request = canonical_request(
        method,
        &resolved.canonical_uri,
        &resolved.canonical_query_string,
        &canonical_headers,
        &signed_headers,
        payload_hash,
    );

    let string_to_sign = string_to_sign(
        params.region,
        params.service,
        params.now,
        &canonical_request,
    );
    let signature = signature(
        params.credentials,
        params.region,
        params.service,
        params.now,
        &string_to_sign,
    );

    let credential_scope = credential_scope(params.region, params.service, params.now);
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        params.credentials.access_key_id(),
        credential_scope,
        signed_headers,
        signature
    );

    let value = HeaderValue::from_str(&authorization)
        .map_err(|_| Error::signing("invalid authorization header value"))?;
    headers.insert(http::header::AUTHORIZATION, value);

    Ok(())
}

pub(crate) fn presign(
    method: Method,
    resolved: ResolvedUrl,
    params: SigV4Params<'_>,
    expires_in: std::time::Duration,
    mut query_params: Vec<(String, String)>,
    mut headers: HeaderMap,
) -> Result<PresignedRequest, Error> {
    let mut resolved = resolved;
    let expires = validate_presign_expires(expires_in)?;
    validate_existing_presign_query_params(&query_params)?;
    validate_presign_headers(&headers)?;

    headers.insert(http::header::HOST, host_header_value(&resolved.url)?);
    let (canonical_headers, signed_headers) = canonicalize_headers(&headers)?;
    headers.remove(http::header::HOST);

    let amz_date = amz_datetime(params.now);
    let credential_scope = credential_scope(params.region, params.service, params.now);
    let credential = format!(
        "{}/{}",
        params.credentials.access_key_id(),
        credential_scope
    );

    query_params.push((
        "X-Amz-Algorithm".to_string(),
        "AWS4-HMAC-SHA256".to_string(),
    ));
    query_params.push(("X-Amz-Credential".to_string(), credential));
    query_params.push(("X-Amz-Date".to_string(), amz_date));
    query_params.push(("X-Amz-Expires".to_string(), expires.to_string()));

    query_params.push(("X-Amz-SignedHeaders".to_string(), signed_headers.clone()));

    if let Some(token) = params.credentials.session_token() {
        query_params.push(("X-Amz-Security-Token".to_string(), token.to_string()));
    }

    resolved.canonical_query_string = crate::util::encode::canonical_query_string(&query_params);
    let canonical_request = canonical_request(
        &method,
        &resolved.canonical_uri,
        &resolved.canonical_query_string,
        &canonical_headers,
        &signed_headers,
        UNSIGNED_PAYLOAD,
    );

    let string_to_sign = string_to_sign(
        params.region,
        params.service,
        params.now,
        &canonical_request,
    );
    let sig = signature(
        params.credentials,
        params.region,
        params.service,
        params.now,
        &string_to_sign,
    );

    query_params.push(("X-Amz-Signature".to_string(), sig));
    let final_query = crate::util::encode::canonical_query_string(&query_params);
    resolved.url.set_query(Some(&final_query));

    Ok(PresignedRequest {
        method,
        url: resolved.url,
        headers,
    })
}

pub(crate) fn validate_presign_expires(expires_in: std::time::Duration) -> Result<u64, Error> {
    const MAX_PRESIGN_EXPIRES: std::time::Duration = std::time::Duration::from_secs(604_800);

    if expires_in.is_zero() {
        return Err(Error::invalid_config("presign expires_in must be > 0"));
    }
    if expires_in.subsec_nanos() != 0 {
        return Err(Error::invalid_config(
            "presign expires_in must use whole seconds",
        ));
    }
    if expires_in > MAX_PRESIGN_EXPIRES {
        return Err(Error::invalid_config(
            "presign expires_in must be <= 7 days",
        ));
    }

    Ok(expires_in.as_secs())
}

fn validate_existing_presign_query_params(
    existing_query_params: &[(String, String)],
) -> Result<(), Error> {
    for (name, value) in existing_query_params {
        validate_presign_query_param(name, value)?;
    }
    Ok(())
}

pub(crate) fn validate_presign_query_param(name: &str, value: &str) -> Result<(), Error> {
    validate_presign_query_name(name)?;
    validate_presign_query_value(value)
}

fn validate_presign_query_name(name: &str) -> Result<(), Error> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::invalid_config(
            "presign query parameter name must not be empty",
        ));
    }
    if trimmed != name {
        return Err(Error::invalid_config(
            "presign query parameter name must not include leading or trailing whitespace",
        ));
    }
    if name
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(Error::invalid_config(
            "presign query parameter name must not contain ASCII control or whitespace characters",
        ));
    }
    if trimmed
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("x-amz-"))
    {
        return Err(Error::invalid_config(
            "presign query parameters must not use reserved x-amz-* names",
        ));
    }
    Ok(())
}

fn validate_presign_query_value(value: &str) -> Result<(), Error> {
    if value.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::invalid_config(
            "presign query parameter value must not contain ASCII control characters",
        ));
    }
    Ok(())
}

fn validate_presign_headers(headers: &HeaderMap) -> Result<(), Error> {
    for (name, value) in headers {
        validate_presign_header(name, value)?;
    }
    Ok(())
}

pub(crate) fn validate_presign_header(name: &HeaderName, value: &HeaderValue) -> Result<(), Error> {
    validate_presign_header_name(name)?;
    value.to_str().map_err(|_| {
        Error::invalid_config(format!(
            "presign header {name} value must be representable as a UTF-8 string"
        ))
    })?;
    Ok(())
}

pub(crate) fn validate_presign_header_name(name: &HeaderName) -> Result<(), Error> {
    if is_sigv4_managed_presign_header(name.as_str()) {
        return Err(Error::invalid_config(format!(
            "presign headers must not include SigV4-managed header {name}"
        )));
    }
    Ok(())
}

fn is_sigv4_managed_presign_header(name: &str) -> bool {
    matches!(
        name,
        "host" | "authorization" | "x-amz-content-sha256" | "x-amz-date" | "x-amz-security-token"
    )
}

fn set_amz_headers(
    headers: &mut HeaderMap,
    payload_hash: &str,
    credentials: &Credentials,
    now: OffsetDateTime,
) -> Result<(), Error> {
    let amz_date = amz_datetime(now);
    let amz_date = HeaderValue::from_str(&amz_date)
        .map_err(|_| Error::signing("invalid x-amz-date header value"))?;
    headers.insert("x-amz-date", amz_date);

    let payload_hash = HeaderValue::from_str(payload_hash)
        .map_err(|_| Error::signing("invalid x-amz-content-sha256 header value"))?;
    headers.insert("x-amz-content-sha256", payload_hash);

    if let Some(token) = credentials.session_token() {
        let token = HeaderValue::from_str(token)
            .map_err(|_| Error::signing("invalid x-amz-security-token header value"))?;
        headers.insert("x-amz-security-token", token);
    }

    Ok(())
}

fn host_header_value(url: &url::Url) -> Result<HeaderValue, Error> {
    let host = url
        .host_str()
        .ok_or_else(|| Error::invalid_config("endpoint must include host"))?;
    let default_port = match url.scheme() {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    };
    let host = match (url.port(), default_port) {
        (Some(port), Some(default)) if port != default => format!("{host}:{port}"),
        (Some(port), None) => format!("{host}:{port}"),
        _ => host.to_string(),
    };

    HeaderValue::from_str(&host).map_err(|_| Error::signing("invalid host header value"))
}

fn canonicalize_headers(headers: &HeaderMap) -> Result<(String, String), Error> {
    let mut names: Vec<_> = headers
        .keys()
        .filter(|name| should_sign_header(name.as_str()))
        .collect();
    names.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));

    let mut canonical_headers = String::new();
    let mut signed_headers = String::new();
    for (idx, name) in names.into_iter().enumerate() {
        canonical_headers.push_str(name.as_str());
        canonical_headers.push(':');
        for (value_idx, value) in headers.get_all(name).iter().enumerate() {
            let value = value
                .to_str()
                .map_err(|_| Error::signing(format!("invalid {name} header value for signing")))?;
            if value_idx > 0 {
                canonical_headers.push(',');
            }
            append_normalized_header_value(&mut canonical_headers, value);
        }
        canonical_headers.push('\n');

        if idx > 0 {
            signed_headers.push(';');
        }
        signed_headers.push_str(name.as_str());
    }

    Ok((canonical_headers, signed_headers))
}

fn should_sign_header(name: &str) -> bool {
    match name {
        "host"
        | "content-type"
        | "content-md5"
        | "range"
        | "if-match"
        | "if-none-match"
        | "if-modified-since"
        | "if-unmodified-since" => true,
        _ => name.starts_with("x-amz-"),
    }
}

fn append_normalized_header_value(out: &mut String, value: &str) {
    let mut in_ws = false;
    for c in value.trim().chars() {
        if c.is_whitespace() {
            in_ws = true;
            continue;
        }
        if in_ws {
            out.push(' ');
        }
        in_ws = false;
        out.push(c);
    }
}

fn canonical_request(
    method: &Method,
    canonical_uri: &str,
    canonical_query_string: &str,
    canonical_headers: &str,
    signed_headers: &str,
    payload_hash: &str,
) -> String {
    format!(
        "{method}\n{canonical_uri}\n{canonical_query_string}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    )
}

fn string_to_sign(
    region: &Region,
    service: &str,
    now: OffsetDateTime,
    canonical_request: &str,
) -> String {
    let amz_date = amz_datetime(now);
    let scope = credential_scope(region, service, now);
    let hashed = sha256_hex(canonical_request.as_bytes());
    format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{hashed}")
}

fn signature(
    credentials: &Credentials,
    region: &Region,
    service: &str,
    now: OffsetDateTime,
    string_to_sign: &str,
) -> String {
    let k_date = hmac_sha256(
        format!("AWS4{}", credentials.secret_access_key()).as_bytes(),
        date_stamp(now).as_bytes(),
    );
    let k_region = hmac_sha256(k_date.as_ref(), region.as_str().as_bytes());
    let k_service = hmac_sha256(k_region.as_ref(), service.as_bytes());
    let k_signing = hmac_sha256(k_service.as_ref(), b"aws4_request");
    let sig = hmac_sha256(k_signing.as_ref(), string_to_sign.as_bytes());
    hex::encode(sig.as_ref())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> HashOutput {
    let mut mac = HmacSha256::new(key);
    mac.update(data);
    mac.finish()
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::hash(data).as_ref())
}

fn date_stamp(now: OffsetDateTime) -> String {
    let year = now.year();
    let month = now.month() as u8;
    let day = now.day();
    format!("{year:04}{month:02}{day:02}")
}

fn amz_datetime(now: OffsetDateTime) -> String {
    let year = now.year();
    let month = now.month() as u8;
    let day = now.day();
    let hour = now.hour();
    let minute = now.minute();
    let second = now.second();
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

fn credential_scope(region: &Region, service: &str, now: OffsetDateTime) -> String {
    format!(
        "{}/{}/{service}/aws4_request",
        date_stamp(now),
        region.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::AddressingStyle, util::url as s3_url};
    use std::time::Duration;

    #[test]
    fn signatures_match_aws_documented_examples() {
        // https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sig-v4-header-based-auth.html
        // https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sigv4-query-string-auth.html
        let endpoint = url::Url::parse("https://s3.amazonaws.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds = Credentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        )
        .unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();
        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("examplebucket"),
            Some("test.txt"),
            &[],
            AddressingStyle::VirtualHosted,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(http::header::RANGE, HeaderValue::from_static("bytes=0-9"));
        sign_headers(
            &Method::GET,
            &resolved,
            &mut headers,
            EMPTY_PAYLOAD_HASH,
            SigV4Params::for_s3(&region, &creds, now),
        )
        .unwrap();
        let authorization = headers
            .get(http::header::AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            authorization.rsplit_once("Signature=").unwrap().1,
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
        );

        let presigned = presign(
            Method::GET,
            resolved,
            SigV4Params::for_s3(&region, &creds, now),
            Duration::from_secs(86_400),
            Vec::new(),
            HeaderMap::new(),
        )
        .unwrap();
        let (_, signature) = presigned
            .url
            .query_pairs()
            .find(|(name, _)| name == "X-Amz-Signature")
            .unwrap();
        assert_eq!(
            signature,
            "aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
        );
    }

    #[test]
    fn signs_headers_and_sets_expected_fields() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();

        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        sign_headers(
            &Method::GET,
            &resolved,
            &mut headers,
            EMPTY_PAYLOAD_HASH,
            SigV4Params::for_s3(&region, &creds, now),
        )
        .unwrap();

        assert_eq!(
            headers.get("x-amz-date").unwrap().to_str().unwrap(),
            "20130524T000000Z"
        );

        let auth = headers
            .get(http::header::AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(auth.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20130524/us-east-1/s3/aws4_request,"
        ));
        assert!(auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date,"));
        assert!(auth.contains("Signature="));
        let sig = auth.split("Signature=").nth(1).unwrap();
        assert_eq!(sig.len(), 64);
        assert!(
            sig.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );
    }

    #[test]
    fn signing_rejects_non_utf8_signed_header_values() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();
        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_bytes(&[0xff]).unwrap(),
        );

        let err = sign_headers(
            &Method::GET,
            &resolved,
            &mut headers,
            EMPTY_PAYLOAD_HASH,
            SigV4Params::for_s3(&region, &creds, now),
        )
        .expect_err("non-UTF-8 signed header values must be rejected");

        match err {
            Error::Signing { message } => assert!(message.contains("content-type")),
            other => panic!("expected Signing error, got {other:?}"),
        }
    }

    #[test]
    fn canonicalize_headers_combines_duplicate_signed_headers() {
        let mut headers = HeaderMap::new();
        headers.append("x-amz-meta-color", HeaderValue::from_static(" blue "));
        headers.append("x-amz-meta-color", HeaderValue::from_static("green"));

        let (canonical_headers, signed_headers) = canonicalize_headers(&headers).unwrap();

        assert_eq!(canonical_headers, "x-amz-meta-color:blue,green\n");
        assert_eq!(signed_headers, "x-amz-meta-color");
    }

    #[test]
    fn signing_preserves_ipv6_host_header_brackets() {
        let endpoint = url::Url::parse("http://[::1]:9000").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds = Credentials::new("AKIDEXAMPLE", "SECRET").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();
        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Auto,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        sign_headers(
            &Method::GET,
            &resolved,
            &mut headers,
            EMPTY_PAYLOAD_HASH,
            SigV4Params::for_s3(&region, &creds, now),
        )
        .unwrap();

        assert_eq!(
            headers
                .get(http::header::HOST)
                .and_then(|v| v.to_str().ok()),
            Some("[::1]:9000")
        );
    }

    #[test]
    fn presign_includes_expected_query_params() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();

        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        let presigned = presign(
            Method::GET,
            resolved,
            SigV4Params::for_s3(&region, &creds, now),
            Duration::from_secs(60),
            Vec::new(),
            HeaderMap::new(),
        )
        .unwrap();

        let s = presigned.url.as_str();
        assert!(s.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(
            s.contains("X-Amz-Credential=AKIDEXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request")
        );
        assert!(s.contains("X-Amz-Date=20130524T000000Z"));
        assert!(s.contains("X-Amz-Expires=60"));
        assert!(s.contains("X-Amz-SignedHeaders=host"));
        assert!(s.contains("X-Amz-Signature="));
    }

    #[test]
    fn presign_can_sign_additional_headers() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();

        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        );

        headers.insert(
            http::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        );
        let expected_headers = headers.clone();
        let presigned = presign(
            Method::PUT,
            resolved,
            SigV4Params::for_s3(&region, &creds, now),
            Duration::from_secs(60),
            Vec::new(),
            headers,
        )
        .unwrap();

        let s = presigned.url.as_str();
        assert!(s.contains("X-Amz-SignedHeaders=content-type%3Bhost"));
        assert_eq!(presigned.headers, expected_headers);
        assert_eq!(
            presigned
                .headers
                .get(http::header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "text/plain"
        );
    }

    #[test]
    fn presign_rejects_ambiguous_expiration_durations() {
        assert!(validate_presign_expires(Duration::ZERO).is_err());
        assert!(validate_presign_expires(Duration::from_millis(1500)).is_err());
        assert!(
            validate_presign_expires(Duration::from_secs(604_800) + Duration::from_nanos(1))
                .is_err()
        );
        assert_eq!(
            validate_presign_expires(Duration::from_secs(604_800)).unwrap(),
            604_800
        );
    }

    #[test]
    fn presign_rejects_reserved_amz_query_params() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();
        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        let err = presign(
            Method::GET,
            resolved,
            SigV4Params::for_s3(&region, &creds, now),
            Duration::from_secs(60),
            vec![("X-Amz-Signature".to_string(), "fake".to_string())],
            HeaderMap::new(),
        )
        .expect_err("reserved x-amz-* query names must be rejected");

        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("reserved x-amz-*"));
            }
            other => panic!("expected invalid config error, got {other:?}"),
        }
    }

    #[test]
    fn presign_rejects_query_param_names_with_outer_whitespace() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();
        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        let err = presign(
            Method::GET,
            resolved,
            SigV4Params::for_s3(&region, &creds, now),
            Duration::from_secs(60),
            vec![(
                " response-content-type".to_string(),
                "text/plain".to_string(),
            )],
            HeaderMap::new(),
        )
        .expect_err("query names with outer whitespace must be rejected");

        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("whitespace"));
            }
            other => panic!("expected invalid config error, got {other:?}"),
        }
    }

    #[test]
    fn presign_rejects_query_param_names_with_internal_whitespace() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();
        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        let err = presign(
            Method::GET,
            resolved,
            SigV4Params::for_s3(&region, &creds, now),
            Duration::from_secs(60),
            vec![(
                "response content type".to_string(),
                "text/plain".to_string(),
            )],
            HeaderMap::new(),
        )
        .expect_err("query names with internal whitespace must be rejected");

        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("whitespace"));
            }
            other => panic!("expected invalid config error, got {other:?}"),
        }
    }

    #[test]
    fn presign_rejects_query_param_values_with_control_characters() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();
        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();

        let err = presign(
            Method::GET,
            resolved,
            SigV4Params::for_s3(&region, &creds, now),
            Duration::from_secs(60),
            vec![(
                "response-content-disposition".to_string(),
                "attachment\nfilename=report.csv".to_string(),
            )],
            HeaderMap::new(),
        )
        .expect_err("query values with control characters must be rejected");

        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("control"));
            }
            other => panic!("expected invalid config error, got {other:?}"),
        }
    }

    #[test]
    fn presign_rejects_sigv4_managed_headers() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();

        for header in [
            http::header::HOST,
            http::header::AUTHORIZATION,
            http::header::HeaderName::from_static("x-amz-date"),
            http::header::HeaderName::from_static("x-amz-content-sha256"),
            http::header::HeaderName::from_static("x-amz-security-token"),
        ] {
            let resolved = s3_url::resolve_url(
                &endpoint,
                Some("my-bucket"),
                Some("a+b"),
                &[],
                AddressingStyle::Path,
            )
            .unwrap();
            let mut headers = HeaderMap::new();
            headers.insert(header, HeaderValue::from_static("managed"));

            let err = presign(
                Method::GET,
                resolved,
                SigV4Params::for_s3(&region, &creds, now),
                Duration::from_secs(60),
                Vec::new(),
                headers,
            )
            .expect_err("SigV4-managed headers must be rejected");

            match err {
                Error::InvalidConfig { message } => {
                    assert!(message.contains("SigV4-managed"));
                }
                other => panic!("expected invalid config error, got {other:?}"),
            }
        }
    }

    #[test]
    fn presign_rejects_non_utf8_header_values() {
        let endpoint = url::Url::parse("https://example.com").unwrap();
        let region = Region::new("us-east-1").unwrap();
        let creds =
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1_369_353_600).unwrap();
        let resolved = s3_url::resolve_url(
            &endpoint,
            Some("my-bucket"),
            Some("a+b"),
            &[],
            AddressingStyle::Path,
        )
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-meta-bin", HeaderValue::from_bytes(&[0xff]).unwrap());

        let err = presign(
            Method::GET,
            resolved,
            SigV4Params::for_s3(&region, &creds, now),
            Duration::from_secs(60),
            Vec::new(),
            headers,
        )
        .expect_err("non-UTF-8 signed header values must be rejected");

        match err {
            Error::InvalidConfig { message } => assert!(message.contains("header")),
            other => panic!("expected invalid config error, got {other:?}"),
        }
    }
}
