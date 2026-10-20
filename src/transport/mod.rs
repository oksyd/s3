#[cfg(any(feature = "async", feature = "blocking"))]
use std::cfg_select;
use std::{
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[cfg(any(feature = "tracing", feature = "async", feature = "blocking"))]
const REDACTED_HOST: &str = "<redacted-host>";

#[cfg(feature = "async")]
pub(crate) mod async_transport;
#[cfg(feature = "blocking")]
pub(crate) mod blocking_transport;

#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
use http::HeaderMap;
#[cfg(any(feature = "async", feature = "blocking"))]
use http::Method;
#[cfg(all(
    any(test, feature = "metrics"),
    any(feature = "async", feature = "blocking")
))]
use http::StatusCode;
#[cfg(any(feature = "async", feature = "blocking"))]
use reqx::advanced::TlsBackend;
#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
use reqx::{
    Error as ReqxError, ErrorCode as ReqxErrorCode,
    advanced::{Interceptor, Observer, RequestContext},
};
#[cfg(any(feature = "tracing", feature = "async", feature = "blocking"))]
use url::Url;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RetryConfig {
    pub(crate) max_attempts: u32,
    pub(crate) base_delay: Duration,
    pub(crate) max_delay: Duration,
    pub(crate) max_retry_after: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(2),
            max_retry_after: Duration::from_secs(30),
        }
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl RetryConfig {
    pub(crate) fn validate(self) -> crate::error::Result<Self> {
        if self.max_attempts == 0 {
            return Err(crate::error::Error::invalid_config(
                "max_attempts must be >= 1",
            ));
        }
        if self.base_delay > self.max_delay {
            return Err(crate::error::Error::invalid_config(
                "base_retry_delay must be <= max_retry_delay",
            ));
        }
        Ok(self)
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) const MAX_BUFFERED_RESPONSE_BODY_BYTES: usize = 32 * 1024 * 1024;
#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) const MAX_ERROR_RESPONSE_BODY_BYTES: usize = 256 * 1024;

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) enum RequestBody<S> {
    Empty,
    Bytes(bytes::Bytes),
    Stream {
        stream: S,
        content_length: Option<u64>,
    },
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl<S> RequestBody<S> {
    fn is_replayable(&self) -> bool {
        !matches!(self, Self::Stream { .. })
    }

    fn take_for_attempt(&mut self) -> Self {
        match self {
            Self::Bytes(bytes) => Self::Bytes(bytes.clone()),
            _ => std::mem::replace(self, Self::Empty),
        }
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn ensure_method_accepts_body<S>(
    method: &Method,
    body: &RequestBody<S>,
) -> crate::error::Result<()> {
    if matches!(*method, Method::GET | Method::HEAD | Method::DELETE)
        && !matches!(body, RequestBody::Empty)
    {
        return Err(crate::error::Error::invalid_config(
            "this operation does not accept a request body",
        ));
    }
    Ok(())
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn prepare_user_agent(
    user_agent: Option<String>,
) -> crate::error::Result<(String, http::HeaderValue)> {
    let user_agent_text = user_agent.unwrap_or_else(default_user_agent);
    if user_agent_text.is_empty() {
        return Err(crate::error::Error::invalid_config(
            "User-Agent must not be empty",
        ));
    }
    if user_agent_text.trim() != user_agent_text {
        return Err(crate::error::Error::invalid_config(
            "User-Agent must not include leading or trailing whitespace",
        ));
    }
    let user_agent = http::HeaderValue::from_str(&user_agent_text)
        .map_err(|_| crate::error::Error::invalid_config("invalid User-Agent header"))?;
    Ok((user_agent_text, user_agent))
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn validate_request_timeout(timeout: Option<Duration>) -> crate::error::Result<()> {
    if timeout.is_some_and(|timeout| timeout.is_zero()) {
        return Err(crate::error::Error::invalid_config("timeout must be > 0"));
    }
    Ok(())
}

pub(crate) fn backoff_delay(config: RetryConfig, attempt: u32) -> Duration {
    let attempt = attempt.saturating_sub(1);
    let factor = 1u32 << attempt.min(16);
    let millis = config
        .base_delay
        .as_millis()
        .saturating_mul(u128::from(factor));
    let capped = millis.min(config.max_delay.as_millis());

    let jitter = jitter_millis(capped);
    Duration::from_millis(jitter as u64)
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn default_tls_backend() -> TlsBackend {
    cfg_select! {
        feature = "rustls" => TlsBackend::RustlsRing,
        feature = "native-tls" => TlsBackend::NativeTls,
        _ => TlsBackend::RustlsRing,
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn default_user_agent() -> String {
    format!("s3/{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
#[derive(Debug, Default)]
pub(crate) struct TransportMetricsObserver;

#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
impl Observer for TransportMetricsObserver {
    fn on_request_start(&self, context: &RequestContext) {
        metrics::counter!("s3_http_attempts_total", "method" => method_label(context.method()))
            .increment(1);
    }
}

#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
#[derive(Debug, Default)]
pub(crate) struct TransportMetricsInterceptor;

#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
impl Interceptor for TransportMetricsInterceptor {
    fn on_response(&self, context: &RequestContext, status: StatusCode, _headers: &HeaderMap) {
        metrics::counter!(
            "s3_http_responses_total",
            "method" => method_label(context.method()),
            "class" => status_class(status),
        )
        .increment(1);
    }

    fn on_error(&self, context: &RequestContext, error: &ReqxError) {
        metrics::counter!(
            "s3_http_errors_total",
            "method" => method_label(context.method()),
            "kind" => reqx_error_kind_label(error),
        )
        .increment(1);
    }
}

#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
fn reqx_error_kind_label(error: &ReqxError) -> &'static str {
    match error.code() {
        ReqxErrorCode::MissingRedirectLocation
        | ReqxErrorCode::InvalidRedirectLocation
        | ReqxErrorCode::RedirectLimitExceeded
        | ReqxErrorCode::RedirectBodyNotReplayable => "redirect",
        _ => "transport",
    }
}

#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
pub(crate) fn status_class(status: StatusCode) -> &'static str {
    if status.is_informational() {
        "1xx"
    } else if status.is_success() {
        "2xx"
    } else if status.is_redirection() {
        "3xx"
    } else if status.is_client_error() {
        "4xx"
    } else if status.is_server_error() {
        "5xx"
    } else {
        "other"
    }
}

#[cfg(all(feature = "metrics", any(feature = "async", feature = "blocking")))]
pub(crate) fn method_label(method: &Method) -> &'static str {
    match method.as_str() {
        "GET" => "GET",
        "PUT" => "PUT",
        "HEAD" => "HEAD",
        "DELETE" => "DELETE",
        "POST" => "POST",
        _ => "OTHER",
    }
}

fn jitter_millis(max_millis: u128) -> u128 {
    if max_millis <= 1 {
        return max_millis;
    }

    u128::from(next_jitter_u64()) % max_millis
}

fn advance_jitter_state(state: u64) -> u64 {
    state.wrapping_mul(6364136223846793005).wrapping_add(1)
}

fn next_jitter_u64() -> u64 {
    static JITTER_STATE: OnceLock<AtomicU64> = OnceLock::new();
    let state = JITTER_STATE.get_or_init(|| {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
            ^ u64::from(std::process::id());
        AtomicU64::new(seed.max(1))
    });

    let previous = state.update(Ordering::Relaxed, Ordering::Relaxed, advance_jitter_state);
    advance_jitter_state(previous)
}

#[cfg(any(feature = "async", feature = "blocking"))]
fn request_id_from_headers(headers: &http::HeaderMap) -> Option<String> {
    header_text_from_headers(headers, &["x-amz-request-id", "x-request-id"])
}

#[cfg(any(feature = "async", feature = "blocking"))]
fn host_id_from_headers(headers: &http::HeaderMap) -> Option<String> {
    header_text_from_headers(headers, &["x-amz-id-2", "x-amz-host-id"])
}

#[cfg(any(feature = "async", feature = "blocking"))]
fn header_text_from_headers(headers: &http::HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

#[cfg(any(test, feature = "async", feature = "blocking"))]
pub(crate) fn retry_after_from_headers(headers: &http::HeaderMap) -> Option<Duration> {
    headers
        .get(http::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after_value)
}

#[cfg(any(test, feature = "async", feature = "blocking"))]
pub(crate) fn clamp_retry_after(config: RetryConfig, retry_after: Duration) -> Duration {
    retry_after.min(config.max_retry_after)
}

#[cfg(any(test, feature = "async", feature = "blocking"))]
fn parse_retry_after_value(value: &str) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let when = httpdate::parse_http_date(value).ok()?;
    let now = std::time::SystemTime::now();
    match when.duration_since(now) {
        Ok(delay) => Some(delay),
        Err(_) => Some(Duration::from_secs(0)),
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) struct RequestTimer {
    #[cfg(feature = "metrics")]
    start: std::time::Instant,
    #[cfg(feature = "metrics")]
    method: &'static str,
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl RequestTimer {
    pub(crate) fn start(method: &Method) -> Self {
        #[cfg(not(feature = "metrics"))]
        let _ = method;
        Self {
            #[cfg(feature = "metrics")]
            start: std::time::Instant::now(),
            #[cfg(feature = "metrics")]
            method: method_label(method),
        }
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl Drop for RequestTimer {
    fn drop(&mut self) {
        #[cfg(feature = "metrics")]
        metrics::histogram!(
            "s3_http_request_duration_seconds",
            "method" => self.method,
        )
        .record(self.start.elapsed().as_secs_f64());
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn record_service_error(method: &Method) {
    #[cfg(feature = "metrics")]
    metrics::counter!(
        "s3_http_errors_total",
        "method" => method_label(method),
        "kind" => "service"
    )
    .increment(1);

    #[cfg(not(feature = "metrics"))]
    let _ = method;
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn record_retry(method: &Method, reason: &'static str) {
    #[cfg(feature = "metrics")]
    metrics::counter!(
        "s3_http_retries_total",
        "method" => method_label(method),
        "reason" => reason
    )
    .increment(1);

    #[cfg(not(feature = "metrics"))]
    let _ = (method, reason);
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn response_error_from_status(
    status: http::StatusCode,
    headers: &http::HeaderMap,
    body: &str,
) -> crate::error::Error {
    response_error_from_parts(
        status,
        headers,
        crate::util::xml::parse_error_xml(body),
        Some(body),
    )
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn response_error(resp: reqx::Response) -> crate::error::Error {
    response_error_from_body(resp.status(), resp.headers(), resp.body())
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn response_error_from_body(
    status: http::StatusCode,
    headers: &http::HeaderMap,
    body: &[u8],
) -> crate::error::Error {
    match std::str::from_utf8(body) {
        Ok(body) => response_error_from_status(status, headers, body),
        Err(_) => response_error_from_parts(status, headers, None, None),
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
fn response_error_from_parts(
    status: http::StatusCode,
    headers: &http::HeaderMap,
    parsed: Option<crate::types::xml::XmlError>,
    body: Option<&str>,
) -> crate::error::Error {
    let request_id = request_id_from_headers(headers);
    let header_host_id = host_id_from_headers(headers);
    let snippet = body.map(|body| crate::util::text::truncate_snippet(body, 4096));
    let (code, message, parsed_request_id, parsed_host_id) = match parsed {
        Some(parsed) => (
            parsed.code,
            parsed.message,
            parsed.request_id,
            parsed.host_id,
        ),
        None => (None, None, None, None),
    };

    crate::error::Error::Api(Box::new(crate::error::ApiError {
        status,
        retry_after: retry_after_from_headers(headers),
        code,
        message,
        request_id: parsed_request_id.or(request_id),
        host_id: parsed_host_id.or(header_host_id),
        body_snippet: snippet,
    }))
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn response_service_error(
    status: http::StatusCode,
    headers: &http::HeaderMap,
    body: &str,
) -> Option<crate::error::Error> {
    crate::util::xml::parse_error_xml(body)
        .map(|parsed| response_error_from_parts(status, headers, Some(parsed), Some(body)))
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn response_error_if_any(resp: &reqx::Response) -> Option<crate::Error> {
    if resp.status().is_success() {
        response_service_error(
            resp.status(),
            resp.headers(),
            std::str::from_utf8(resp.body()).ok()?,
        )
    } else {
        Some(response_error_from_body(
            resp.status(),
            resp.headers(),
            resp.body(),
        ))
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn transport_retry_delay(
    retry: RetryConfig,
    attempt: u32,
    max_attempts: u32,
    method: &Method,
    error: &reqx::Error,
) -> Option<Duration> {
    use reqx::TransportErrorKind;

    if attempt >= max_attempts || !method_allows_status_retry(method) {
        return None;
    }

    // Retry transient I/O failures. TLS, request framing, and body write errors
    // require caller intervention and must not replay the request.
    matches!(
        error,
        reqx::Error::Transport {
            kind: TransportErrorKind::Dns | TransportErrorKind::Connect | TransportErrorKind::Read,
            ..
        } | reqx::Error::Timeout { .. }
            | reqx::Error::ReadBody { .. }
    )
    .then(|| backoff_delay(retry, attempt))
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn service_retry_delay(
    retry: RetryConfig,
    attempt: u32,
    max_attempts: u32,
    method: &Method,
    error: &crate::Error,
) -> Option<Duration> {
    let error = error.api_error()?;
    if attempt >= max_attempts
        || !error.is_retryable()
        || !(method_allows_status_retry(method) || error.has_retryable_code())
    {
        return None;
    }
    Some(match error.retry_after {
        Some(delay) => clamp_retry_after(retry, delay),
        None => backoff_delay(retry, attempt),
    })
}

#[cfg(any(feature = "async", feature = "blocking"))]
fn method_allows_status_retry(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::PUT | Method::DELETE
    )
}

#[cfg(any(feature = "async", feature = "blocking"))]
struct SanitizedReqxSource {
    code: reqx::ErrorCode,
    method: Option<http::Method>,
    uri: Option<String>,
    detail: Option<String>,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl SanitizedReqxSource {
    fn from_reqx(error: reqx::Error) -> Self {
        let mut sanitized = Self {
            code: error.code(),
            method: error.request_method().cloned(),
            uri: error.request_uri_redacted().map(redacted_uri_for_error),
            detail: None,
            source: None,
        };

        match error {
            reqx::Error::InvalidNoProxyRule { rule } => {
                sanitized.detail = Some(format!("rule={rule:?}"));
            }
            reqx::Error::InvalidProxyConfig { message, .. } => {
                sanitized.detail = Some(message);
            }
            reqx::Error::ProxyAuthorizationRequiresHttpProxy => {
                sanitized.detail = Some("proxy_authorization requires http_proxy".to_string());
            }
            reqx::Error::InvalidAdaptiveConcurrencyPolicy {
                min_limit,
                initial_limit,
                max_limit,
                message,
            } => {
                sanitized.detail = Some(format!(
                    "min_limit={min_limit} initial_limit={initial_limit} max_limit={max_limit}: {message}"
                ));
            }
            reqx::Error::SerializeJson { source } | reqx::Error::DeserializeJson { source, .. } => {
                sanitized.source = Some(Box::new(source));
            }
            reqx::Error::SerializeQuery { source } | reqx::Error::SerializeForm { source } => {
                sanitized.source = Some(Box::new(source));
            }
            reqx::Error::RequestBuild { source } => {
                sanitized.source = Some(Box::new(source));
            }
            reqx::Error::Transport {
                kind,
                message,
                source,
                ..
            } => {
                sanitized.detail = Some(format!("kind={kind}: {message}"));
                sanitized.source = Some(source);
            }
            reqx::Error::Timeout {
                phase, timeout_ms, ..
            } => {
                sanitized.detail = Some(format!("phase={phase} timeout_ms={timeout_ms}"));
            }
            reqx::Error::DeadlineExceeded { timeout_ms, .. } => {
                sanitized.detail = Some(format!("timeout_ms={timeout_ms}"));
            }
            reqx::Error::ReadBody { source } | reqx::Error::WriteBody { source, .. } => {
                sanitized.source = Some(source);
            }
            reqx::Error::ResponseBodyTooLarge {
                limit_bytes,
                actual_bytes,
                ..
            } => {
                sanitized.detail = Some(format!(
                    "actual_bytes={actual_bytes} limit_bytes={limit_bytes}"
                ));
            }
            reqx::Error::HttpStatus { status, .. }
            | reqx::Error::MissingRedirectLocation { status, .. } => {
                sanitized.detail = Some(format!("status={status}"));
            }
            reqx::Error::DecodeText { source, .. } => {
                sanitized.source = Some(Box::new(source));
            }
            reqx::Error::InvalidHeaderName { name, source } => {
                sanitized.detail = Some(format!("name={name}"));
                sanitized.source = Some(Box::new(source));
            }
            reqx::Error::InvalidHeaderValue { name, source } => {
                sanitized.detail = Some(format!("name={name}"));
                sanitized.source = Some(Box::new(source));
            }
            reqx::Error::DecodeContentEncoding {
                encoding, message, ..
            } => {
                sanitized.detail = Some(format!("encoding={encoding}: {message}"));
            }
            reqx::Error::TlsBackendUnavailable { backend } => {
                sanitized.detail = Some(format!("backend={backend}"));
            }
            reqx::Error::TlsBackendInit { backend, message }
            | reqx::Error::TlsConfig { backend, message } => {
                sanitized.detail = Some(format!("backend={backend}: {message}"));
            }
            reqx::Error::CircuitOpen { retry_after_ms, .. } => {
                sanitized.detail = Some(format!("retry_after_ms={retry_after_ms}"));
            }
            reqx::Error::RedirectLimitExceeded { max_redirects, .. } => {
                sanitized.detail = Some(format!("max_redirects={max_redirects}"));
            }
            _ => {}
        }
        sanitized
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl std::fmt::Display for SanitizedReqxSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "reqx {}", self.code.as_str())?;
        if let Some(method) = &self.method {
            write!(formatter, " for {method}")?;
        }
        if let Some(uri) = &self.uri {
            write!(formatter, " {uri}")?;
        }
        if let Some(detail) = &self.detail {
            write!(formatter, " ({detail})")?;
        }
        if let Some(source) = &self.source {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl std::fmt::Debug for SanitizedReqxSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SanitizedReqxSource")
            .field("message", &self.to_string())
            .finish()
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl std::error::Error for SanitizedReqxSource {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|error| error as _)
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
fn redacted_uri_for_error(uri: &str) -> String {
    match Url::parse(uri) {
        Ok(url) => redacted_url_for_error(&url),
        Err(_) => "<redacted-uri>".to_string(),
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn map_reqx_error(message: &str, err: reqx::Error) -> crate::error::Error {
    match err {
        reqx::Error::HttpStatus {
            status,
            headers,
            body,
            ..
        } => {
            let status = match http::StatusCode::from_u16(status) {
                Ok(status) => status,
                Err(source) => {
                    return crate::Error::decode(
                        "HTTP client returned an invalid status",
                        Some(Box::new(source)),
                    );
                }
            };
            response_error_from_status(status, &headers, &body)
        }
        reqx::Error::DeserializeJson { source, .. } => crate::error::Error::decode(
            format!("{message}: failed to decode response json"),
            Some(Box::new(source)),
        ),
        other => crate::error::Error::transport(
            message,
            Some(Box::new(SanitizedReqxSource::from_reqx(other))),
        ),
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
pub(crate) fn redacted_url_for_error(url: &Url) -> String {
    let mut out = String::new();
    out.push_str(url.scheme());
    out.push_str("://");
    out.push_str(REDACTED_HOST);

    if let Some(port) = url.port() {
        let default = match url.scheme() {
            "http" => Some(80),
            "https" => Some(443),
            _ => None,
        };
        if Some(port) != default {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }

    out.push_str(redacted_path_for_trace(url));
    if url.query().is_some() {
        out.push_str("?<redacted>");
    }
    out
}

#[cfg(any(feature = "tracing", feature = "async", feature = "blocking"))]
pub(crate) fn redacted_path_for_trace(url: &Url) -> &'static str {
    if url.path() == "/" {
        "/"
    } else {
        "/<redacted>"
    }
}

#[cfg(all(
    any(feature = "async", feature = "blocking"),
    any(feature = "tracing", test)
))]
pub(crate) fn redacted_host_for_trace(_url: &Url) -> &'static str {
    REDACTED_HOST
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    #[cfg(any(feature = "async", feature = "blocking"))]
    use http::Method;

    use super::*;

    #[test]
    fn backoff_delay_is_capped_and_non_negative() {
        let cfg = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(2),
            max_retry_after: Duration::from_secs(30),
        };

        let d1 = backoff_delay(cfg, 1);
        let d2 = backoff_delay(cfg, 2);
        let d3 = backoff_delay(cfg, 3);
        let d99 = backoff_delay(cfg, 99);

        assert!(d1 < Duration::from_millis(200));
        assert!(d2 < Duration::from_millis(400));
        assert!(d3 < Duration::from_millis(800));
        assert!(d99 < cfg.max_delay);
    }

    #[test]
    fn backoff_delay_zero_base_is_zero() {
        let cfg = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_secs(2),
            max_retry_after: Duration::from_secs(30),
        };

        assert_eq!(backoff_delay(cfg, 1), Duration::from_millis(0));
        assert_eq!(backoff_delay(cfg, 10), Duration::from_millis(0));
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn transport_retries_preserve_safety_and_attempt_limits() {
        let retry = RetryConfig {
            base_delay: Duration::ZERO,
            ..RetryConfig::default()
        };
        for (kind, retryable) in [
            (reqx::TransportErrorKind::Dns, true),
            (reqx::TransportErrorKind::Connect, true),
            (reqx::TransportErrorKind::Read, true),
            (reqx::TransportErrorKind::Tls, false),
            (reqx::TransportErrorKind::Other, false),
        ] {
            let error = reqx::Error::Transport {
                kind,
                method: Method::GET,
                uri: "http://localhost/".to_string(),
                message: "test failure".to_string(),
                source: Box::new(std::io::Error::other("test failure")),
            };
            assert_eq!(
                transport_retry_delay(retry, 1, 2, &Method::GET, &error).is_some(),
                retryable,
                "kind={kind}"
            );
            assert!(transport_retry_delay(retry, 2, 2, &Method::GET, &error).is_none());
            assert!(transport_retry_delay(retry, 1, 1, &Method::GET, &error).is_none());
            assert!(transport_retry_delay(retry, 1, 2, &Method::POST, &error).is_none());
        }
    }

    #[test]
    fn retry_config_default_includes_retry_budget() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_attempts, 3);
        assert_eq!(cfg.max_retry_after, Duration::from_secs(30));
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn retry_config_rejects_ambiguous_delay_window() {
        let err = RetryConfig {
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(1),
            ..RetryConfig::default()
        }
        .validate()
        .expect_err("base delay must not exceed max delay");

        match err {
            crate::error::Error::InvalidConfig { message } => {
                assert!(message.contains("base_retry_delay"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn validate_request_timeout_rejects_zero() {
        let err = validate_request_timeout(Some(Duration::ZERO))
            .expect_err("zero timeout must be rejected");

        match err {
            crate::error::Error::InvalidConfig { message } => {
                assert!(message.contains("timeout"));
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }

        assert!(validate_request_timeout(Some(Duration::from_millis(1))).is_ok());
        assert!(validate_request_timeout(None).is_ok());
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn prepare_user_agent_rejects_blank_or_ambiguous_values() {
        assert!(prepare_user_agent(Some(String::new())).is_err());
        assert!(prepare_user_agent(Some(" s3-client".to_string())).is_err());
        assert!(prepare_user_agent(Some("s3-client ".to_string())).is_err());
        assert!(prepare_user_agent(Some("s3-client".to_string())).is_ok());
    }

    #[test]
    fn jitter_millis_produces_varying_values() {
        let mut seen = BTreeSet::new();
        for _ in 0..16 {
            seen.insert(jitter_millis(997));
        }
        assert!(seen.len() > 1);
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn map_reqx_error_http_status_is_mapped_to_api_error() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "x-amz-request-id",
            http::HeaderValue::from_static("req-123"),
        );
        let body = "<Error><Code>AccessDenied</Code><Message>Access Denied</Message></Error>";

        let err = reqx::Error::HttpStatus {
            status: 403,
            method: http::Method::GET,
            uri: "https://example.com/path".to_string(),
            headers: Box::new(headers),
            body: body.to_string(),
        };

        match map_reqx_error("request failed", err) {
            crate::error::Error::Api(error) => {
                let crate::ApiError {
                    status,
                    code,
                    message,
                    request_id,
                    ..
                } = *error;
                assert_eq!(status, http::StatusCode::FORBIDDEN);
                assert_eq!(code.as_deref(), Some("AccessDenied"));
                assert_eq!(message.as_deref(), Some("Access Denied"));
                assert_eq!(request_id.as_deref(), Some("req-123"));
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn map_reqx_error_write_body_preserves_source_and_redacts_context() {
        let err = reqx::Error::WriteBody {
            method: http::Method::GET,
            uri: "https://private.example.com/object?token=secret".to_string(),
            source: Box::new(std::io::Error::other("writer failed")),
        };
        let mapped = map_reqx_error("request failed", err);
        let crate::Error::Transport { source, .. } = mapped else {
            panic!("expected Transport error");
        };
        let source = source.unwrap();
        let text = source.to_string();
        assert!(text.contains("GET"));
        assert!(text.contains("<redacted-host>"));
        assert!(!text.contains("private.example.com"));
        assert!(!text.contains("secret"));
        let cause = source
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(cause.to_string(), "writer failed");
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn map_reqx_error_transport_source_redacts_host_path_and_query() {
        let err = reqx::Error::Transport {
            kind: reqx::TransportErrorKind::Connect,
            method: http::Method::GET,
            uri: "https://bucket.s3.example.com/private/object/key?token=secret".to_string(),
            message: "connect failed".to_string(),
            source: Box::new(std::io::Error::other("connect failed")),
        };

        let mapped = map_reqx_error("request failed", err);
        let debug = format!("{mapped:?}");

        match mapped {
            crate::error::Error::Transport { message, source } => {
                assert_eq!(message, "request failed");
                let source = source.expect("expected sanitized reqx source");
                let source_text = source.to_string();

                assert!(source_text.contains("https://<redacted-host>/<redacted>"));
                assert!(!source_text.contains("bucket.s3.example.com"));
                assert!(!source_text.contains("private/object/key"));
                assert!(!source_text.contains("token=secret"));

                assert!(debug.contains("https://<redacted-host>/<redacted>"));
                assert!(!debug.contains("bucket.s3.example.com"));
                assert!(!debug.contains("private/object/key"));
                assert!(!debug.contains("token=secret"));
            }
            other => panic!("expected Transport error, got {other:?}"),
        }
    }

    #[test]
    fn retry_after_from_headers_supports_http_date() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("Sun, 06 Nov 1994 08:49:37 GMT"),
        );

        assert_eq!(
            retry_after_from_headers(&headers),
            Some(Duration::from_secs(0))
        );
    }

    #[test]
    fn retry_after_from_headers_supports_seconds() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("7"),
        );

        assert_eq!(
            retry_after_from_headers(&headers),
            Some(Duration::from_secs(7))
        );
    }

    #[test]
    fn clamp_retry_after_respects_retry_policy_cap() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(2),
            max_retry_after: Duration::from_secs(30),
        };

        assert_eq!(
            clamp_retry_after(config, Duration::from_millis(500)),
            Duration::from_millis(500)
        );
        assert_eq!(
            clamp_retry_after(config, Duration::from_secs(30)),
            Duration::from_secs(30)
        );
        assert_eq!(
            clamp_retry_after(config, Duration::from_secs(60)),
            Duration::from_secs(30)
        );
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn service_retry_delay_honors_retry_after_for_http_and_service_errors() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(2),
            max_retry_after: Duration::from_secs(30),
        };
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("5"),
        );

        for status in [StatusCode::SERVICE_UNAVAILABLE, StatusCode::BAD_REQUEST] {
            let error = response_error_from_status(
                status,
                &headers,
                "<Error><Code>SlowDown</Code></Error>",
            );
            assert_eq!(
                service_retry_delay(config, 1, 3, &Method::GET, &error),
                Some(Duration::from_secs(5))
            );
            assert_eq!(
                service_retry_delay(config, 3, 3, &Method::GET, &error),
                None
            );
        }
        let error = response_error_from_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            &headers,
            "<Error><Code>Unknown</Code></Error>",
        );
        assert_eq!(
            service_retry_delay(config, 1, 3, &Method::POST, &error),
            None
        );
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_error_from_status_maps_common_status_matrix() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "x-amz-request-id",
            http::HeaderValue::from_static("req-matrix"),
        );

        let api_statuses = [
            http::StatusCode::BAD_REQUEST,
            http::StatusCode::FORBIDDEN,
            http::StatusCode::NOT_FOUND,
            http::StatusCode::CONFLICT,
            http::StatusCode::PRECONDITION_FAILED,
            http::StatusCode::INTERNAL_SERVER_ERROR,
            http::StatusCode::SERVICE_UNAVAILABLE,
        ];

        for status in api_statuses {
            match response_error_from_status(status, &headers, "plain error body") {
                crate::error::Error::Api(error) => {
                    let crate::ApiError {
                        status: got_status,
                        request_id,
                        ..
                    } = *error;
                    assert_eq!(got_status, status);
                    assert_eq!(request_id.as_deref(), Some("req-matrix"));
                }
                other => panic!("expected Api for {status}, got {other:?}"),
            }
        }

        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("3"),
        );
        match response_error_from_status(http::StatusCode::TOO_MANY_REQUESTS, &headers, "throttled")
        {
            crate::error::Error::Api(error) => {
                assert!(error.is_rate_limited());
                let crate::ApiError {
                    retry_after,
                    request_id,
                    code,
                    message,
                    host_id,
                    ..
                } = *error;
                assert_eq!(retry_after, Some(Duration::from_secs(3)));
                assert_eq!(request_id.as_deref(), Some("req-matrix"));
                assert!(code.is_none());
                assert!(message.is_none());
                assert!(host_id.is_none());
            }
            other => panic!("expected RateLimited for 429, got {other:?}"),
        }
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_error_preserves_http_status_for_non_utf8_bodies() {
        let err = response_error_from_body(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            &http::HeaderMap::new(),
            &[0xff],
        );

        assert_eq!(err.status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
        assert_eq!(err.body_snippet(), None);
        assert!(service_retry_delay(RetryConfig::default(), 1, 3, &Method::GET, &err).is_some());
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_error_from_status_uses_header_request_and_host_ids() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "x-amz-request-id",
            http::HeaderValue::from_static(" req-header "),
        );
        headers.insert(
            "x-amz-id-2",
            http::HeaderValue::from_static(" host-header "),
        );

        match response_error_from_status(http::StatusCode::FORBIDDEN, &headers, "plain error body")
        {
            crate::error::Error::Api(error) => {
                let crate::ApiError {
                    request_id,
                    host_id,
                    ..
                } = *error;
                assert_eq!(request_id.as_deref(), Some("req-header"));
                assert_eq!(host_id.as_deref(), Some("host-header"));
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn redacted_path_for_trace_hides_object_paths() {
        let root = Url::parse("https://example.com/").expect("url");
        let object = Url::parse("https://example.com/a/b/c?token=secret").expect("url");
        assert_eq!(redacted_path_for_trace(&root), "/");
        assert_eq!(redacted_path_for_trace(&object), "/<redacted>");
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn redacted_host_for_trace_hides_virtual_host_bucket() {
        let url = Url::parse("https://bucket-with-sensitive-name.s3.example.com/key").expect("url");
        assert_eq!(redacted_host_for_trace(&url), "<redacted-host>");
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_error_from_status_429_preserves_service_error_fields() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "x-amz-request-id",
            http::HeaderValue::from_static("req-outer"),
        );

        let body = r#"
<Error>
  <Code>SlowDown</Code>
  <Message>slow down</Message>
  <RequestId>req-inner</RequestId>
  <HostId>host-1</HostId>
</Error>
"#;

        match response_error_from_status(http::StatusCode::TOO_MANY_REQUESTS, &headers, body) {
            crate::error::Error::Api(error) => {
                assert!(error.is_rate_limited());
                let crate::ApiError {
                    request_id,
                    code,
                    message,
                    host_id,
                    body_snippet,
                    ..
                } = *error;
                assert_eq!(request_id.as_deref(), Some("req-inner"));
                assert_eq!(code.as_deref(), Some("SlowDown"));
                assert_eq!(message.as_deref(), Some("slow down"));
                assert_eq!(host_id.as_deref(), Some("host-1"));
                assert!(
                    body_snippet
                        .as_deref()
                        .unwrap_or_default()
                        .contains("SlowDown")
                );
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_service_error_detects_embedded_service_error_xml() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "x-amz-request-id",
            http::HeaderValue::from_static("req-success-xml"),
        );
        let body = r#"<Error><Code>InternalError</Code><Message>backend failure</Message></Error>"#;

        let err = response_service_error(http::StatusCode::OK, &headers, body)
            .expect("expected embedded error to be detected");
        match err {
            crate::error::Error::Api(error) => {
                let crate::ApiError {
                    status,
                    code,
                    request_id,
                    ..
                } = *error;
                assert_eq!(status, http::StatusCode::OK);
                assert_eq!(code.as_deref(), Some("InternalError"));
                assert_eq!(request_id.as_deref(), Some("req-success-xml"));
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_service_error_ignores_non_error_success_body() {
        assert!(
            response_service_error(
                http::StatusCode::OK,
                &http::HeaderMap::new(),
                "<ListBucketResult/>"
            )
            .is_none()
        );
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_service_error_ignores_delete_result_entry_errors() {
        let body = r#"
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

        assert!(
            response_service_error(http::StatusCode::OK, &http::HeaderMap::new(), body).is_none()
        );
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_service_error_maps_request_id_only_error_payload() {
        let body = r#"<Error><RequestId>req-only</RequestId></Error>"#;
        let err =
            response_service_error(http::StatusCode::BAD_REQUEST, &http::HeaderMap::new(), body)
                .expect("request-id-only payload should be treated as service error");
        match err {
            crate::error::Error::Api(error) => {
                let crate::ApiError {
                    status, request_id, ..
                } = *error;
                assert_eq!(status, http::StatusCode::BAD_REQUEST);
                assert_eq!(request_id.as_deref(), Some("req-only"));
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[cfg(any(feature = "async", feature = "blocking"))]
    #[test]
    fn response_service_error_preserves_retryable_service_code_on_4xx() {
        let body = r#"<Error><Code>SlowDown</Code><Message>slow down</Message></Error>"#;
        let err =
            response_service_error(http::StatusCode::BAD_REQUEST, &http::HeaderMap::new(), body)
                .expect("expected embedded service error");
        match err {
            crate::error::Error::Api(error) => {
                assert!(error.is_rate_limited());
                let crate::ApiError { status, code, .. } = *error;
                assert_eq!(status, http::StatusCode::BAD_REQUEST);
                assert_eq!(code.as_deref(), Some("SlowDown"));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[cfg(all(
        any(feature = "async", feature = "blocking"),
        feature = "credentials-imds"
    ))]
    #[test]
    fn map_reqx_error_deserialize_json_is_sanitized() {
        let source = serde_json::from_slice::<serde_json::Value>(b"{not-json")
            .expect_err("invalid json should produce an error");
        let err = reqx::Error::DeserializeJson {
            source,
            body: "token=super-secret-value".to_string(),
        };

        let mapped = map_reqx_error("request failed", err);
        match mapped {
            crate::error::Error::Decode { source, .. } => {
                assert!(source.is_some());
            }
            other => panic!("expected Decode error, got {other:?}"),
        }
    }
}
