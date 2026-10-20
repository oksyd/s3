use super::parse_expiration;
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
#[cfg(any(feature = "async", feature = "blocking"))]
use reqx::{
    advanced::TlsRootStore,
    prelude::{RedirectPolicy, RetryPolicy, StatusPolicy},
};

use crate::{
    auth::{AddressingStyle, Credentials, CredentialsSnapshot, Region},
    error::Error,
};

const SERVICE: &str = "sts";

#[cfg(feature = "async")]
pub(crate) async fn assume_role_async(
    region: &Region,
    role_arn: &str,
    role_session_name: &str,
    source_credentials: &Credentials,
    tls_root_store: TlsRootStore,
) -> Result<CredentialsSnapshot, Error> {
    use std::time::Duration;

    let (url, headers, body) =
        assume_role_request(region, role_arn, role_session_name, source_credentials)?;

    let client = sts_async_client(Duration::from_secs(10), tls_root_store)?;
    let resp = send_form_async(&client, url.into(), headers, body).await?;
    let text = sts_response_text(resp.status(), resp.headers(), resp.body())?;

    parse_assume_role_response(text)
}

#[cfg(feature = "blocking")]
pub(crate) fn assume_role_blocking(
    region: &Region,
    role_arn: &str,
    role_session_name: &str,
    source_credentials: &Credentials,
    tls_root_store: TlsRootStore,
) -> Result<CredentialsSnapshot, Error> {
    use std::time::Duration;

    let (url, headers, body) =
        assume_role_request(region, role_arn, role_session_name, source_credentials)?;

    let client = sts_blocking_client(Duration::from_secs(10), tls_root_store)?;
    let resp = send_form_blocking(&client, url.into(), headers, body)?;
    let text = sts_response_text(resp.status(), resp.headers(), resp.body())?;

    parse_assume_role_response(text)
}

fn assume_role_request(
    region: &Region,
    role_arn: &str,
    role_session_name: &str,
    source_credentials: &Credentials,
) -> Result<(url::Url, HeaderMap, Bytes), Error> {
    validate_assume_role_inputs(role_arn, role_session_name)?;

    let endpoint = sts_regional_endpoint(region.as_str(), None)?;
    let body = form_body(&[
        ("Action", "AssumeRole"),
        ("Version", "2011-06-15"),
        ("RoleArn", role_arn),
        ("RoleSessionName", role_session_name),
    ]);

    let body_bytes = Bytes::from(body);
    let payload_hash = crate::util::signing::sha256_hex(&body_bytes);

    let resolved =
        crate::util::url::resolve_url(&endpoint, None, None, &[], AddressingStyle::Path)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );

    let now = time::OffsetDateTime::now_utc();
    crate::util::signing::sign_headers(
        &Method::POST,
        &resolved,
        &mut headers,
        &payload_hash,
        crate::util::signing::SigV4Params::new(region, SERVICE, source_credentials, now),
    )?;

    Ok((resolved.url, headers, body_bytes))
}

#[cfg(feature = "async")]
pub(crate) async fn assume_role_with_web_identity_env_async(
    tls_root_store: TlsRootStore,
) -> Result<CredentialsSnapshot, Error> {
    use std::time::Duration;

    let (url, headers, body) = web_identity_request()?;

    let client = sts_async_client(Duration::from_secs(10), tls_root_store)?;
    let resp = send_form_async(&client, url.into(), headers, body).await?;
    let text = sts_response_text(resp.status(), resp.headers(), resp.body())?;

    parse_assume_role_with_web_identity_response(text)
}

#[cfg(feature = "blocking")]
pub(crate) fn assume_role_with_web_identity_env_blocking(
    tls_root_store: TlsRootStore,
) -> Result<CredentialsSnapshot, Error> {
    use std::time::Duration;

    let (url, headers, body) = web_identity_request()?;

    let client = sts_blocking_client(Duration::from_secs(10), tls_root_store)?;
    let resp = send_form_blocking(&client, url.into(), headers, body)?;
    let text = sts_response_text(resp.status(), resp.headers(), resp.body())?;

    parse_assume_role_with_web_identity_response(text)
}

fn web_identity_request() -> Result<(url::Url, HeaderMap, Bytes), Error> {
    let (role_arn, session_name, token) = web_identity_env()?;
    let endpoint = web_identity_sts_endpoint(&role_arn)?;

    let body = form_body(&[
        ("Action", "AssumeRoleWithWebIdentity"),
        ("Version", "2011-06-15"),
        ("RoleArn", &role_arn),
        ("RoleSessionName", &session_name),
        ("WebIdentityToken", &token),
    ]);
    let body_bytes = Bytes::from(body);

    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );

    Ok((endpoint, headers, body_bytes))
}

#[cfg(feature = "async")]
async fn send_form_async(
    client: &reqx::Client,
    url: String,
    headers: HeaderMap,
    body: Bytes,
) -> Result<reqx::Response, Error> {
    let mut req = client.request(Method::POST, url).body(body);
    for (name, value) in headers {
        if let Some(name) = name {
            req = req.header(name, value);
        }
    }

    req.send()
        .await
        .map_err(|e| crate::transport::map_reqx_error("request failed", e))
}

#[cfg(feature = "blocking")]
fn send_form_blocking(
    client: &reqx::blocking::Client,
    url: String,
    headers: HeaderMap,
    body: Bytes,
) -> Result<reqx::Response, Error> {
    let mut req = client.request(Method::POST, url).body(body);
    for (name, value) in headers {
        if let Some(name) = name {
            req = req.header(name, value);
        }
    }

    req.send()
        .map_err(|e| crate::transport::map_reqx_error("request failed", e))
}

fn sts_response_text<'a>(
    status: StatusCode,
    headers: &HeaderMap,
    body: &'a Bytes,
) -> Result<&'a str, Error> {
    let text = crate::util::text::decode_utf8_response_body(body)?;
    if status.is_success() {
        return Ok(text);
    }

    Err(crate::transport::response_error_from_status(
        status, headers, text,
    ))
}

fn sts_regional_endpoint(region: &str, partition: Option<&str>) -> Result<url::Url, Error> {
    let suffix = if matches!(partition, Some("aws-cn")) || region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    let url = format!("https://sts.{region}.{suffix}");
    url::Url::parse(&url).map_err(|_| Error::invalid_config("invalid STS endpoint URL"))
}

fn web_identity_region_from_env() -> Result<Option<String>, Error> {
    let value =
        crate::util::env::optional_first_non_empty_var(&["AWS_REGION", "AWS_DEFAULT_REGION"])?
            .map(|(_, value)| value);
    let Some(value) = value else {
        return Ok(None);
    };
    if value.trim() != value {
        return Err(Error::invalid_config(
            "AWS_REGION or AWS_DEFAULT_REGION must not include leading or trailing whitespace",
        ));
    }
    Ok(Some(value))
}

fn partition_from_role_arn(role_arn: &str) -> Option<&str> {
    let mut parts = role_arn.splitn(6, ':');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("arn"), Some(partition), Some(_service), Some(_region), Some(_account))
            if !partition.is_empty() =>
        {
            Some(partition)
        }
        _ => None,
    }
}

fn web_identity_sts_endpoint(role_arn: &str) -> Result<url::Url, Error> {
    let partition = partition_from_role_arn(role_arn);
    let region = web_identity_region_from_env()?;
    resolve_web_identity_sts_endpoint(partition, region.as_deref())
}

fn resolve_web_identity_sts_endpoint(
    partition: Option<&str>,
    region: Option<&str>,
) -> Result<url::Url, Error> {
    let region = region.ok_or_else(|| {
        Error::invalid_config(
            "AWS_REGION or AWS_DEFAULT_REGION is required for regional STS endpoint",
        )
    })?;
    crate::util::validation::validate_region("region", region)?;
    sts_regional_endpoint(region, partition)
}

fn web_identity_env() -> Result<(String, String, String), Error> {
    let role_arn = crate::util::env::required_var("AWS_ROLE_ARN")?;
    let token_file = crate::util::env::required_non_empty_var("AWS_WEB_IDENTITY_TOKEN_FILE")?;
    let session_name = crate::util::env::optional_var("AWS_ROLE_SESSION_NAME")?
        .unwrap_or_else(|| "s3-session".to_string());

    validate_assume_role_inputs(&role_arn, &session_name)?;

    let mut token = std::fs::read_to_string(token_file)
        .map_err(|e| Error::invalid_config(format!("failed to read web identity token: {e}")))?;
    let token_len = web_identity_token_from_file_contents(&token)?.len();
    token.truncate(token_len);

    Ok((role_arn, session_name, token))
}

fn web_identity_token_from_file_contents(contents: &str) -> Result<&str, Error> {
    let token = crate::util::text::strip_trailing_line_ending(contents);
    if token.is_empty() {
        return Err(Error::invalid_config("web identity token is empty"));
    }
    if token
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(Error::invalid_config(
            "web identity token must not contain ASCII control or whitespace characters",
        ));
    }
    Ok(token)
}

fn validate_assume_role_inputs(role_arn: &str, role_session_name: &str) -> Result<(), Error> {
    validate_non_empty_no_outer_whitespace("role_arn", role_arn)?;
    validate_non_empty_no_outer_whitespace("role_session_name", role_session_name)?;
    validate_role_arn(role_arn)?;

    let len = role_session_name.len();
    if !(2..=64).contains(&len) {
        return Err(Error::invalid_config(
            "role_session_name must be 2..=64 bytes",
        ));
    }
    if !role_session_name.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'=' | b',' | b'.' | b'@' | b'-')
    }) {
        return Err(Error::invalid_config(
            "role_session_name contains characters not allowed by STS",
        ));
    }

    Ok(())
}

fn validate_role_arn(role_arn: &str) -> Result<(), Error> {
    let mut parts = role_arn.splitn(6, ':');
    let (Some("arn"), Some(partition), Some("iam"), Some(region), Some(account), Some(resource)) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(Error::invalid_config("role_arn must be an IAM role ARN"));
    };

    if partition.is_empty()
        || !partition
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(Error::invalid_config(
            "role_arn partition must contain only ASCII letters, digits, or '-'",
        ));
    }
    if !region.is_empty() {
        return Err(Error::invalid_config(
            "role_arn for IAM roles must not include a region",
        ));
    }
    if account.is_empty() || !account.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::invalid_config(
            "role_arn account id must contain only digits",
        ));
    }
    if !resource.starts_with("role/") || resource.len() == "role/".len() {
        return Err(Error::invalid_config(
            "role_arn resource must start with role/",
        ));
    }

    Ok(())
}

fn validate_non_empty_no_outer_whitespace(name: &str, value: &str) -> Result<(), Error> {
    if value.is_empty() {
        return Err(Error::invalid_config(format!("{name} must not be empty")));
    }
    if value.trim() != value {
        return Err(Error::invalid_config(format!(
            "{name} must not include leading or trailing whitespace"
        )));
    }
    if value
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(Error::invalid_config(format!(
            "{name} must not contain ASCII control or whitespace characters"
        )));
    }
    Ok(())
}

fn form_body(params: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (idx, (k, v)) in params.iter().enumerate() {
        if idx > 0 {
            out.push('&');
        }
        out.push_str(&crate::util::encode::aws_percent_encode(k));
        out.push('=');
        out.push_str(&crate::util::encode::aws_percent_encode(v));
    }
    out
}

#[derive(serde::Deserialize)]
struct XmlStsResult {
    #[serde(rename = "Credentials")]
    credentials: XmlStsCredentials,
}

#[derive(serde::Deserialize)]
struct XmlStsCredentials {
    #[serde(rename = "AccessKeyId")]
    access_key_id: String,
    #[serde(rename = "Expiration")]
    expiration: String,
    #[serde(rename = "SecretAccessKey")]
    secret_access_key: String,
    #[serde(rename = "SessionToken")]
    session_token: String,
}

impl XmlStsCredentials {
    fn into_snapshot(self) -> Result<CredentialsSnapshot, Error> {
        let mut creds = Credentials::new(self.access_key_id, self.secret_access_key)?;
        creds = creds.with_session_token(self.session_token)?;
        let expires_at = parse_expiration(&self.expiration)?;
        Ok(CredentialsSnapshot::new(creds).with_expires_at(expires_at))
    }
}

fn parse_assume_role_response(body: &str) -> Result<CredentialsSnapshot, Error> {
    #[derive(serde::Deserialize)]
    struct XmlAssumeRoleResponse {
        #[serde(rename = "AssumeRoleResult")]
        result: XmlStsResult,
    }

    let parsed = quick_xml::de::from_str::<XmlAssumeRoleResponse>(body)
        .map_err(|e| Error::decode("failed to parse AssumeRole XML response", Some(Box::new(e))))?;

    parsed.result.credentials.into_snapshot()
}

fn parse_assume_role_with_web_identity_response(body: &str) -> Result<CredentialsSnapshot, Error> {
    #[derive(serde::Deserialize)]
    struct XmlResponse {
        #[serde(rename = "AssumeRoleWithWebIdentityResult")]
        result: XmlStsResult,
    }

    let parsed = quick_xml::de::from_str::<XmlResponse>(body).map_err(|e| {
        Error::decode(
            "failed to parse AssumeRoleWithWebIdentity XML response",
            Some(Box::new(e)),
        )
    })?;

    parsed.result.credentials.into_snapshot()
}

#[cfg(feature = "async")]
fn sts_async_client(
    timeout: std::time::Duration,
    tls_root_store: TlsRootStore,
) -> Result<reqx::Client, Error> {
    reqx::Client::builder("http://localhost")
        .request_timeout(timeout)
        .retry_policy(RetryPolicy::disabled())
        .redirect_policy(RedirectPolicy::none())
        .default_status_policy(StatusPolicy::Response)
        .max_response_body_bytes(4 * 1024 * 1024)
        .tls_backend(crate::transport::default_tls_backend())
        .tls_root_store(tls_root_store)
        .client_name("s3-sts")
        .build()
        .map_err(|e| Error::transport("failed to build HTTP client", Some(Box::new(e))))
}

#[cfg(feature = "blocking")]
fn sts_blocking_client(
    timeout: std::time::Duration,
    tls_root_store: TlsRootStore,
) -> Result<reqx::blocking::Client, Error> {
    reqx::blocking::Client::builder("http://localhost")
        .request_timeout(timeout)
        .retry_policy(RetryPolicy::disabled())
        .redirect_policy(RedirectPolicy::none())
        .default_status_policy(StatusPolicy::Response)
        .max_response_body_bytes(4 * 1024 * 1024)
        .tls_backend(crate::transport::default_tls_backend())
        .tls_root_store(tls_root_store)
        .client_name("s3-sts")
        .build()
        .map_err(|e| Error::transport("failed to build HTTP client", Some(Box::new(e))))
}

#[cfg(test)]
mod tests {
    use std::io::{ErrorKind, Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::thread::JoinHandle;
    use std::time::Duration;
    use std::time::Instant;

    use super::*;

    fn spawn_test_server(
        response: Vec<u8>,
    ) -> std::result::Result<(SocketAddr, JoinHandle<()>), Error> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| Error::transport("failed to bind test server", Some(Box::new(e))))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| Error::transport("failed to configure test server", Some(Box::new(e))))?;
        let addr = listener.local_addr().map_err(|e| {
            Error::transport("failed to read test server address", Some(Box::new(e)))
        })?;

        let handle = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                        let mut request = Vec::new();
                        let mut buf = [0u8; 1024];
                        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                            match stream.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    request.extend_from_slice(&buf[..n]);
                                    if request.len() > 64 * 1024 {
                                        break;
                                    }
                                }
                                Err(err)
                                    if matches!(
                                        err.kind(),
                                        ErrorKind::WouldBlock | ErrorKind::TimedOut
                                    ) =>
                                {
                                    break;
                                }
                                Err(_) => break,
                            }
                        }
                        let _ = stream.write_all(&response);
                        let _ = stream.flush();
                        break;
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => return,
                }
            }
        });

        Ok((addr, handle))
    }

    #[cfg(all(
        any(feature = "async", feature = "blocking"),
        feature = "native-tls",
        not(feature = "rustls")
    ))]
    fn assert_native_tls_webpki_error(err: Error) {
        match err {
            Error::Transport {
                source: Some(source),
                ..
            } => {
                assert!(
                    source.to_string().contains("TlsRootStore::WebPki"),
                    "unexpected source error: {source}"
                );
            }
            other => panic!("expected transport error, got {other:?}"),
        }
    }

    #[test]
    fn builds_regional_endpoint() {
        let url = sts_regional_endpoint("us-east-1", None).unwrap();
        assert_eq!(url.as_str(), "https://sts.us-east-1.amazonaws.com/");
    }

    #[test]
    fn builds_regional_endpoint_for_cn_region() {
        let url = sts_regional_endpoint("cn-north-1", None).unwrap();
        assert_eq!(url.as_str(), "https://sts.cn-north-1.amazonaws.com.cn/");
    }

    #[test]
    fn web_identity_sts_requires_a_region_in_every_partition() {
        for partition in [None, Some("aws"), Some("aws-cn"), Some("aws-us-gov")] {
            let err = resolve_web_identity_sts_endpoint(partition, None)
                .expect_err("web identity requires an explicit region");
            assert!(matches!(err, Error::InvalidConfig { message }
                if message.contains("AWS_REGION or AWS_DEFAULT_REGION")));
        }
    }

    #[test]
    fn web_identity_sts_uses_regional_endpoints() {
        for (partition, region, expected) in [
            (
                Some("aws"),
                "eu-west-1",
                "https://sts.eu-west-1.amazonaws.com/",
            ),
            (
                Some("aws-cn"),
                "cn-northwest-1",
                "https://sts.cn-northwest-1.amazonaws.com.cn/",
            ),
            (
                Some("aws-us-gov"),
                "us-gov-west-1",
                "https://sts.us-gov-west-1.amazonaws.com/",
            ),
        ] {
            assert_eq!(
                resolve_web_identity_sts_endpoint(partition, Some(region))
                    .unwrap()
                    .as_str(),
                expected
            );
        }
        assert!(resolve_web_identity_sts_endpoint(Some("aws"), Some(" eu-west-1")).is_err());
    }

    #[test]
    fn form_body_percent_encodes() {
        let body = form_body(&[("a+b", "c d"), ("x", "~")]);
        assert_eq!(body, "a%2Bb=c%20d&x=~");
    }

    #[test]
    fn assume_role_inputs_reject_ambiguous_values() {
        assert!(validate_assume_role_inputs("arn:aws:iam::123:role/demo", "s3-session").is_ok());
        assert!(validate_assume_role_inputs(" arn:aws:iam::123:role/demo", "s3-session").is_err());
        assert!(
            validate_assume_role_inputs("arn:aws:iam::123:role/demo prod", "s3-session").is_err()
        );
        assert!(validate_assume_role_inputs("arn:aws:iam::123:role/demo", "x").is_err());
        assert!(validate_assume_role_inputs("arn:aws:iam::123:role/demo", "bad space").is_err());
    }

    #[test]
    fn role_arn_validation_rejects_non_iam_role_shapes() {
        assert!(validate_role_arn("arn:aws:iam::123456789012:role/demo/path").is_ok());

        for role_arn in [
            "not-an-arn",
            "arn:aws:s3:::bucket",
            "arn:aws:iam:us-east-1:123456789012:role/demo",
            "arn:aws:iam::abc:role/demo",
            "arn:aws:iam::123456789012:user/demo",
            "arn:aws:iam::123456789012:role/",
        ] {
            let err = validate_role_arn(role_arn).expect_err("invalid role ARN must be rejected");
            match err {
                Error::InvalidConfig { message } => assert!(message.contains("role_arn")),
                other => panic!("expected invalid config, got {other:?}"),
            }
        }
    }

    #[test]
    fn web_identity_token_file_contents_allow_only_one_clean_line() {
        assert_eq!(
            web_identity_token_from_file_contents("header.payload.signature\n").unwrap(),
            "header.payload.signature"
        );
        assert!(web_identity_token_from_file_contents("").is_err());
        assert!(web_identity_token_from_file_contents(" token").is_err());
        assert!(web_identity_token_from_file_contents("token\n\n").is_err());
        assert!(web_identity_token_from_file_contents("token\tvalue").is_err());
    }

    #[test]
    fn parses_assume_role_response() {
        let xml = r#"
 <AssumeRoleResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">
   <AssumeRoleResult>
     <Credentials>
       <AccessKeyId>AKIA_TEST</AccessKeyId>
       <Expiration>2020-01-01T00:00:00Z</Expiration>
       <SecretAccessKey>SECRET_TEST</SecretAccessKey>
       <SessionToken>TOKEN_TEST</SessionToken>
     </Credentials>
   </AssumeRoleResult>
 </AssumeRoleResponse>
 "#;

        let snapshot = parse_assume_role_response(xml).unwrap();
        let creds = snapshot.credentials();
        assert_eq!(creds.access_key_id(), "AKIA_TEST");
        assert_eq!(creds.secret_access_key(), "SECRET_TEST");
        assert_eq!(creds.session_token(), Some("TOKEN_TEST"));
        assert_eq!(
            snapshot.expires_at(),
            Some(parse_expiration("2020-01-01T00:00:00Z").unwrap())
        );
    }

    #[test]
    fn parse_assume_role_response_rejects_ambiguous_expiration() {
        let xml = r#"
 <AssumeRoleResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">
   <AssumeRoleResult>
     <Credentials>
       <AccessKeyId>AKIA_TEST</AccessKeyId>
       <Expiration>2020-01-01T00:00:00Z </Expiration>
       <SecretAccessKey>SECRET_TEST</SecretAccessKey>
       <SessionToken>TOKEN_TEST</SessionToken>
     </Credentials>
   </AssumeRoleResult>
 </AssumeRoleResponse>
 "#;

        let err =
            parse_assume_role_response(xml).expect_err("ambiguous expiration must be rejected");
        assert!(matches!(err, Error::Decode { .. }));
    }

    #[test]
    fn parses_assume_role_with_web_identity_response() {
        let xml = r#"
 <AssumeRoleWithWebIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">
   <AssumeRoleWithWebIdentityResult>
     <Credentials>
       <AccessKeyId>AKIA_TEST</AccessKeyId>
       <Expiration>2020-01-01T00:00:00Z</Expiration>
       <SecretAccessKey>SECRET_TEST</SecretAccessKey>
       <SessionToken>TOKEN_TEST</SessionToken>
     </Credentials>
   </AssumeRoleWithWebIdentityResult>
 </AssumeRoleWithWebIdentityResponse>
 "#;

        let snapshot = parse_assume_role_with_web_identity_response(xml).unwrap();
        let creds = snapshot.credentials();
        assert_eq!(creds.access_key_id(), "AKIA_TEST");
        assert_eq!(creds.secret_access_key(), "SECRET_TEST");
        assert_eq!(creds.session_token(), Some("TOKEN_TEST"));
        assert_eq!(
            snapshot.expires_at(),
            Some(parse_expiration("2020-01-01T00:00:00Z").unwrap())
        );
    }

    #[test]
    fn sts_api_error_parses_xml_error() {
        let err_xml = r#"
<Error>
  <Code>AccessDenied</Code>
  <Message>Access Denied</Message>
  <RequestId>req-123</RequestId>
  <HostId>host-456</HostId>
</Error>
"#;
        let headers = HeaderMap::new();
        let err =
            crate::transport::response_error_from_status(StatusCode::FORBIDDEN, &headers, err_xml);
        match err {
            Error::Api(error) => {
                let crate::ApiError {
                    status,
                    code,
                    message,
                    request_id,
                    host_id,
                    body_snippet,
                    ..
                } = *error;
                assert_eq!(status, StatusCode::FORBIDDEN);
                assert_eq!(code.as_deref(), Some("AccessDenied"));
                assert_eq!(message.as_deref(), Some("Access Denied"));
                assert_eq!(request_id.as_deref(), Some("req-123"));
                assert_eq!(host_id.as_deref(), Some("host-456"));
                assert!(body_snippet.unwrap_or_default().contains("AccessDenied"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn sts_api_error_maps_rate_limited() {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, HeaderValue::from_static("2"));
        headers.insert("x-amz-request-id", HeaderValue::from_static("req-1"));

        let err = crate::transport::response_error_from_status(
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            "slow down",
        );
        match err {
            Error::Api(error) => {
                assert!(error.is_rate_limited());
                let crate::ApiError {
                    retry_after,
                    request_id,
                    ..
                } = *error;
                assert_eq!(retry_after, Some(Duration::from_secs(2)));
                assert_eq!(request_id.as_deref(), Some("req-1"));
            }
            other => panic!("expected rate-limited error, got {other:?}"),
        }
    }

    #[test]
    fn sts_response_text_rejects_invalid_utf8_success_body() {
        let err = sts_response_text(
            StatusCode::OK,
            &HeaderMap::new(),
            &Bytes::from_static(&[0xff]),
        )
        .expect_err("successful STS response body must be valid UTF-8");

        match err {
            Error::Decode { message, .. } => assert!(message.contains("UTF-8")),
            other => panic!("expected decode error, got {other:?}"),
        }
    }

    #[test]
    fn sts_response_text_rejects_invalid_utf8_non_success_body() {
        let err = sts_response_text(
            StatusCode::FORBIDDEN,
            &HeaderMap::new(),
            &Bytes::from_static(&[0xff]),
        )
        .expect_err("STS error response body must be valid UTF-8");

        match err {
            Error::Decode { message, .. } => assert!(message.contains("UTF-8")),
            other => panic!("expected decode error, got {other:?}"),
        }
    }

    #[cfg(feature = "async")]
    #[test]
    fn sts_async_client_accepts_backend_default() {
        let client = sts_async_client(Duration::from_secs(1), TlsRootStore::BackendDefault);
        let client = client.expect("async STS client should build");
        assert_eq!(client.default_status_policy(), StatusPolicy::Response);
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn send_form_async_non_success_returns_status_response() -> std::result::Result<(), Error>
    {
        let (addr, handle) = spawn_test_server(
            b"HTTP/1.1 403 Forbidden\r\nx-amz-request-id: req-1\r\nContent-Length: 13\r\nConnection: close\r\n\r\nAccess Denied!".to_vec(),
        )?;
        let client = sts_async_client(Duration::from_secs(2), TlsRootStore::BackendDefault)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );

        let url = format!("http://{addr}/");
        let resp = send_form_async(&client, url, headers, Bytes::from("Action=AssumeRole")).await?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = crate::util::text::decode_utf8_response_body(resp.body()).unwrap();
        assert!(body.contains("Access Denied"));
        Ok(())
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn send_form_async_does_not_follow_redirects() -> std::result::Result<(), Error> {
        let (addr, handle) = spawn_test_server(
            b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        )?;
        let client = sts_async_client(Duration::from_secs(2), TlsRootStore::BackendDefault)?;
        let resp = send_form_async(
            &client,
            format!("http://{addr}/"),
            HeaderMap::new(),
            Bytes::from_static(b"Action=AssumeRole"),
        )
        .await?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
        Ok(())
    }

    #[cfg(all(feature = "async", feature = "rustls"))]
    #[test]
    fn sts_async_client_accepts_webpki_on_rustls() {
        let client = sts_async_client(Duration::from_secs(1), TlsRootStore::WebPki);
        assert!(client.is_ok(), "rustls should accept WebPki root store");
    }

    #[cfg(all(feature = "async", feature = "native-tls", not(feature = "rustls")))]
    #[test]
    fn sts_async_client_rejects_webpki_on_native_tls() {
        let err = match sts_async_client(Duration::from_secs(1), TlsRootStore::WebPki) {
            Ok(_) => panic!("native-tls should reject WebPki root store"),
            Err(err) => err,
        };
        assert_native_tls_webpki_error(err);
    }

    #[cfg(feature = "blocking")]
    #[test]
    fn sts_blocking_client_accepts_backend_default() {
        let client = sts_blocking_client(Duration::from_secs(1), TlsRootStore::BackendDefault);
        let client = client.expect("blocking STS client should build");
        assert_eq!(client.default_status_policy(), StatusPolicy::Response);
    }

    #[cfg(feature = "blocking")]
    #[test]
    fn send_form_blocking_non_success_returns_status_response() -> std::result::Result<(), Error> {
        let (addr, handle) = spawn_test_server(
            b"HTTP/1.1 403 Forbidden\r\nx-amz-request-id: req-1\r\nContent-Length: 13\r\nConnection: close\r\n\r\nAccess Denied!".to_vec(),
        )?;
        let client = sts_blocking_client(Duration::from_secs(2), TlsRootStore::BackendDefault)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );

        let url = format!("http://{addr}/");
        let resp = send_form_blocking(&client, url, headers, Bytes::from("Action=AssumeRole"))?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = crate::util::text::decode_utf8_response_body(resp.body()).unwrap();
        assert!(body.contains("Access Denied"));
        Ok(())
    }

    #[cfg(feature = "blocking")]
    #[test]
    fn send_form_blocking_does_not_follow_redirects() -> std::result::Result<(), Error> {
        let (addr, handle) = spawn_test_server(
            b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        )?;
        let client = sts_blocking_client(Duration::from_secs(2), TlsRootStore::BackendDefault)?;
        let resp = send_form_blocking(
            &client,
            format!("http://{addr}/"),
            HeaderMap::new(),
            Bytes::from_static(b"Action=AssumeRole"),
        )?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
        Ok(())
    }

    #[cfg(all(feature = "blocking", feature = "rustls"))]
    #[test]
    fn sts_blocking_client_accepts_webpki_on_rustls() {
        let client = sts_blocking_client(Duration::from_secs(1), TlsRootStore::WebPki);
        assert!(client.is_ok(), "rustls should accept WebPki root store");
    }

    #[cfg(all(feature = "blocking", feature = "native-tls", not(feature = "rustls")))]
    #[test]
    fn sts_blocking_client_rejects_webpki_on_native_tls() {
        let err = match sts_blocking_client(Duration::from_secs(1), TlsRootStore::WebPki) {
            Ok(_) => panic!("native-tls should reject WebPki root store"),
            Err(err) => err,
        };
        assert_native_tls_webpki_error(err);
    }
}
