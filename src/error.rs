use std::{error::Error as StdError, fmt, time::Duration};

use http::StatusCode;

/// Library result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Details reported by S3 or a credential service.
#[derive(Debug)]
pub struct ApiError {
    /// HTTP status returned by the service.
    pub status: StatusCode,
    /// Suggested delay before retrying, derived from `Retry-After` when present.
    pub retry_after: Option<Duration>,
    /// Service-specific error code, when present.
    pub code: Option<String>,
    /// Service-provided error message, when present.
    pub message: Option<String>,
    /// Service request id, when present in headers or the error payload.
    pub request_id: Option<String>,
    /// Service host id, when present.
    pub host_id: Option<String>,
    /// Truncated UTF-8 response body, absent when the body is not valid UTF-8.
    pub body_snippet: Option<String>,
}

impl ApiError {
    /// Returns true for HTTP 429 or a service throttling error code.
    pub fn is_rate_limited(&self) -> bool {
        self.status == StatusCode::TOO_MANY_REQUESTS
            || self.code.as_deref().is_some_and(|code| {
                matches!(
                    code,
                    "Throttling"
                        | "ThrottlingException"
                        | "ThrottledException"
                        | "TooManyRequestsException"
                        | "RequestLimitExceeded"
                        | "SlowDown"
                )
            })
    }

    /// Returns true for throttling, server errors, or transient service error codes.
    ///
    /// The client also checks the operation and body replayability before retrying.
    pub fn is_retryable(&self) -> bool {
        self.status == StatusCode::TOO_MANY_REQUESTS
            || self.status.is_server_error()
            || self.has_retryable_code()
    }

    pub(crate) fn has_retryable_code(&self) -> bool {
        self.code.as_deref().is_some_and(|code| {
            matches!(
                code,
                "Throttling"
                    | "ThrottlingException"
                    | "ThrottledException"
                    | "TooManyRequestsException"
                    | "RequestLimitExceeded"
                    | "SlowDown"
                    | "RequestTimeout"
                    | "RequestTimeoutException"
                    | "InternalError"
                    | "InternalFailure"
                    | "ServiceUnavailable"
            )
        })
    }
}

/// Error type for request building, transport, and API responses.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// Invalid configuration or parameters.
    InvalidConfig {
        /// Human-readable validation failure message.
        message: String,
    },
    /// Request signing failed.
    Signing {
        /// Human-readable signing failure message.
        message: String,
    },
    /// Service returned an error response, including throttling.
    Api(Box<ApiError>),
    /// Transport-level failure (HTTP client, IO, TLS).
    Transport {
        /// Human-readable transport failure message.
        message: String,
        /// Underlying transport error, if preserved.
        source: Option<Box<dyn StdError + Send + Sync + 'static>>,
    },
    /// Response decode or parse failure.
    Decode {
        /// Human-readable decode failure message.
        message: String,
        /// Underlying decode error, if preserved.
        source: Option<Box<dyn StdError + Send + Sync + 'static>>,
    },
}

impl Error {
    /// Creates an invalid configuration error.
    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self::InvalidConfig {
            message: message.into(),
        }
    }

    /// Creates a signing error.
    pub fn signing(message: impl Into<String>) -> Self {
        Self::Signing {
            message: message.into(),
        }
    }

    /// Creates a transport error with optional source.
    pub fn transport(
        message: impl Into<String>,
        source: Option<Box<dyn StdError + Send + Sync + 'static>>,
    ) -> Self {
        Self::Transport {
            message: message.into(),
            source,
        }
    }

    /// Creates a decode error with optional source.
    pub fn decode(
        message: impl Into<String>,
        source: Option<Box<dyn StdError + Send + Sync + 'static>>,
    ) -> Self {
        Self::Decode {
            message: message.into(),
            source,
        }
    }

    /// Returns the service error details when available.
    pub fn api_error(&self) -> Option<&ApiError> {
        match self {
            Self::Api(error) => Some(error),
            _ => None,
        }
    }

    /// Returns an HTTP status when available.
    pub fn status(&self) -> Option<StatusCode> {
        self.api_error().map(|error| error.status)
    }

    /// Returns the request id if reported by the service.
    pub fn request_id(&self) -> Option<&str> {
        self.api_error()?.request_id.as_deref()
    }

    /// Returns the service error code when available.
    pub fn code(&self) -> Option<&str> {
        self.api_error()?.code.as_deref()
    }

    /// Returns the service error message when available.
    pub fn message(&self) -> Option<&str> {
        self.api_error()?.message.as_deref()
    }

    /// Returns the service host id when available.
    pub fn host_id(&self) -> Option<&str> {
        self.api_error()?.host_id.as_deref()
    }

    /// Returns a truncated response body snippet when available.
    pub fn body_snippet(&self) -> Option<&str> {
        self.api_error()?.body_snippet.as_deref()
    }

    /// Returns the service's suggested retry delay when available.
    pub fn retry_after(&self) -> Option<Duration> {
        self.api_error()?.retry_after
    }

    /// Returns true for HTTP 429 or a service throttling error code.
    pub fn is_rate_limited(&self) -> bool {
        self.api_error().is_some_and(ApiError::is_rate_limited)
    }
}

impl StdError for ApiError {}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_rate_limited() {
            write!(f, "rate limited: {}", self.status)?;
            if let Some(delay) = self.retry_after {
                write!(f, " (retry after {}s)", delay.as_secs())?;
            }
        } else {
            write!(f, "api error: {}", self.status)?;
        }
        for (label, value) in [
            ("code", self.code.as_deref()),
            ("request_id", self.request_id.as_deref()),
            ("host_id", self.host_id.as_deref()),
        ] {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                write!(f, " {label}={value}")?;
            }
        }
        if let Some(message) = self.message.as_deref().filter(|value| !value.is_empty()) {
            write!(f, " ({message})")?;
        }
        Ok(())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { message } => write!(f, "invalid config: {message}"),
            Self::Signing { message } => write!(f, "signing error: {message}"),
            Self::Api(error) => fmt::Display::fmt(error, f),
            Self::Transport { message, .. } => write!(f, "transport error: {message}"),
            Self::Decode { message, .. } => write!(f, "decode error: {message}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Transport { source, .. } | Self::Decode { source, .. } => {
                source.as_deref().map(|e| e as &(dyn StdError + 'static))
            }
            Self::Api(error) => Some(error.as_ref()),
            Self::InvalidConfig { .. } | Self::Signing { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service_error(status: StatusCode, code: &str) -> Error {
        Error::Api(Box::new(ApiError {
            status,
            retry_after: Some(Duration::from_secs(2)),
            code: Some(code.to_string()),
            message: Some("service failure".to_string()),
            request_id: Some("req-1".to_string()),
            host_id: Some("host-1".to_string()),
            body_snippet: None,
        }))
    }

    #[test]
    fn api_error_retryability_can_be_driven_by_service_code() {
        let err = service_error(StatusCode::OK, "InternalError");
        assert!(err.api_error().unwrap().is_retryable());
        assert!(!err.is_rate_limited());
    }

    #[test]
    fn api_error_with_non_retryable_code_and_2xx_status_is_not_retryable() {
        let err = service_error(StatusCode::OK, "AccessDenied");
        assert!(!err.api_error().unwrap().is_retryable());
    }

    #[test]
    fn api_error_display_includes_host_id_when_available() {
        let err = service_error(StatusCode::FORBIDDEN, "AccessDenied");
        assert_eq!(
            err.to_string(),
            "api error: 403 Forbidden code=AccessDenied request_id=req-1 host_id=host-1 (service failure)"
        );
    }

    #[test]
    fn throttling_classification_uses_status_or_service_code() {
        for (status, code) in [
            (StatusCode::TOO_MANY_REQUESTS, "Unknown"),
            (StatusCode::BAD_REQUEST, "SlowDown"),
            (StatusCode::OK, "ThrottlingException"),
        ] {
            let err = service_error(status, code);
            assert!(err.is_rate_limited());
            assert!(err.api_error().unwrap().is_retryable());
            assert_eq!(err.retry_after(), Some(Duration::from_secs(2)));
            assert!(err.to_string().starts_with("rate limited:"));
        }
    }
}
