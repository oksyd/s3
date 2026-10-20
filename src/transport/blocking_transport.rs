use std::time::Duration;

#[cfg(test)]
use bytes::Bytes;
#[cfg(test)]
use http::StatusCode;
use http::header::USER_AGENT;
use http::{HeaderMap, HeaderValue, Method};
use reqx::{
    advanced::TlsRootStore,
    prelude::{RedirectPolicy, RetryPolicy, StatusPolicy},
};
use url::Url;

use crate::{
    error::Result,
    transport::{
        MAX_BUFFERED_RESPONSE_BODY_BYTES, MAX_ERROR_RESPONSE_BODY_BYTES, RequestBody, RequestTimer,
        RetryConfig, default_tls_backend, ensure_method_accepts_body, map_reqx_error,
        prepare_user_agent, record_retry, record_service_error, response_error,
        response_error_if_any, service_retry_delay, transport_retry_delay,
        validate_request_timeout,
    },
};

pub(crate) type BlockingBody = RequestBody<Box<dyn std::io::Read + Send + 'static>>;

pub(crate) struct BlockingTransport {
    client: reqx::blocking::Client,
    retry: RetryConfig,
    user_agent: HeaderValue,
}

impl BlockingTransport {
    pub(crate) fn new(
        retry: RetryConfig,
        user_agent: Option<String>,
        timeout: Option<Duration>,
        tls_root_store: TlsRootStore,
    ) -> Result<Self> {
        let retry = retry.validate()?;
        validate_request_timeout(timeout)?;
        let (user_agent_text, user_agent) = prepare_user_agent(user_agent)?;

        let mut builder = reqx::blocking::Client::builder("http://localhost")
            .client_name(user_agent_text)
            .retry_policy(RetryPolicy::disabled())
            .redirect_policy(RedirectPolicy::none())
            .default_status_policy(StatusPolicy::Response)
            .max_response_body_bytes(MAX_BUFFERED_RESPONSE_BODY_BYTES)
            .tls_backend(default_tls_backend())
            .tls_root_store(tls_root_store);

        #[cfg(feature = "metrics")]
        {
            builder = builder
                .observer(std::sync::Arc::new(
                    crate::transport::TransportMetricsObserver,
                ))
                .interceptor(std::sync::Arc::new(
                    crate::transport::TransportMetricsInterceptor,
                ));
        }

        if let Some(timeout) = timeout {
            builder = builder.request_timeout(timeout);
        }

        let client = builder
            .build()
            .map_err(|e| map_reqx_error("failed to build HTTP client", e))?;

        Ok(Self {
            client,
            retry,
            user_agent,
        })
    }

    pub(crate) fn send(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        mut body: BlockingBody,
    ) -> Result<reqx::Response> {
        let max_attempts = if body.is_replayable() {
            self.retry.max_attempts
        } else {
            1
        };
        let mut attempt = 1;
        let _timer = RequestTimer::start(&method);

        loop {
            let current_body = body.take_for_attempt();
            let req = self.build_request(&method, &url, &headers, current_body)?;
            let resp = match req.send() {
                Ok(resp) => resp,
                Err(err) => {
                    if let Some(delay) =
                        transport_retry_delay(self.retry, attempt, max_attempts, &method, &err)
                    {
                        record_retry(&method, "transport");
                        std::thread::sleep(delay);
                        attempt += 1;
                        continue;
                    }
                    return Err(map_reqx_error("request failed", err));
                }
            };
            if let Some(error) = response_error_if_any(&resp) {
                if let Some(delay) =
                    service_retry_delay(self.retry, attempt, max_attempts, &method, &error)
                {
                    drop(resp);
                    record_retry(&method, "service");
                    std::thread::sleep(delay);
                    attempt += 1;
                    continue;
                }
                record_service_error(&method);
                return Err(error);
            }

            return Ok(resp);
        }
    }

    pub(crate) fn send_stream(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        mut body: BlockingBody,
    ) -> Result<reqx::blocking::ResponseStream> {
        let max_attempts = if body.is_replayable() {
            self.retry.max_attempts
        } else {
            1
        };
        let mut attempt = 1;
        let _timer = RequestTimer::start(&method);

        loop {
            let current_body = body.take_for_attempt();
            let req = self.build_request(&method, &url, &headers, current_body)?;
            let resp = match req.send_stream() {
                Ok(resp) => resp,
                Err(err) => {
                    if let Some(delay) =
                        transport_retry_delay(self.retry, attempt, max_attempts, &method, &err)
                    {
                        record_retry(&method, "transport");
                        std::thread::sleep(delay);
                        attempt += 1;
                        continue;
                    }
                    return Err(map_reqx_error("request failed", err));
                }
            };

            if resp.status().is_success() {
                return Ok(resp);
            }

            let response = match resp.into_response_limited(MAX_ERROR_RESPONSE_BODY_BYTES) {
                Ok(response) => response,
                Err(error) => {
                    if let Some(delay) =
                        transport_retry_delay(self.retry, attempt, max_attempts, &method, &error)
                    {
                        record_retry(&method, "transport");
                        std::thread::sleep(delay);
                        attempt += 1;
                        continue;
                    }
                    return Err(map_reqx_error("failed to read error response", error));
                }
            };
            let error = response_error(response);
            if let Some(delay) =
                service_retry_delay(self.retry, attempt, max_attempts, &method, &error)
            {
                record_retry(&method, "service");
                std::thread::sleep(delay);
                attempt += 1;
                continue;
            }
            record_service_error(&method);
            return Err(error);
        }
    }

    fn build_request(
        &self,
        method: &Method,
        url: &Url,
        headers: &HeaderMap,
        body: BlockingBody,
    ) -> Result<reqx::blocking::RequestBuilder> {
        ensure_method_accepts_body(method, &body)?;

        let mut req = self
            .client
            .request(method.clone(), url.as_str().to_string())
            .header(USER_AGENT, self.user_agent.clone());

        for name in headers.keys() {
            req = req.header(name.clone(), headers[name].clone());
        }

        req = match body {
            BlockingBody::Empty => req,
            BlockingBody::Bytes(b) => req.body(b),
            BlockingBody::Stream {
                stream: reader,
                content_length: Some(len),
            } => req
                .header(http::header::CONTENT_LENGTH, HeaderValue::from(len))
                .body_reader(reader),
            BlockingBody::Stream {
                stream: reader,
                content_length: None,
            } => req.body_reader(reader),
        };

        Ok(req)
    }
}

#[cfg(test)]
mod tests {
    use crate::Error;
    use std::io::{ErrorKind, Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use super::*;
    use crate::transport::response_error_from_status;

    mod reqx {
        pub use ::reqx::advanced::TlsRootStore;
    }

    type RequestCaptureServer = (SocketAddr, std::thread::JoinHandle<()>, Arc<Mutex<Vec<u8>>>);

    fn spawn_test_server(
        responses: Vec<Vec<u8>>,
    ) -> Result<(SocketAddr, std::thread::JoinHandle<()>, Arc<AtomicUsize>)> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| Error::transport("failed to bind test server", Some(Box::new(e))))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| Error::transport("failed to configure test server", Some(Box::new(e))))?;
        let addr = listener.local_addr().map_err(|e| {
            Error::transport("failed to read test server address", Some(Box::new(e)))
        })?;

        let hits = Arc::new(AtomicUsize::new(0));
        let hits_thread = hits.clone();

        let handle = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            for response in responses {
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
                            hits_thread.fetch_add(1, Ordering::SeqCst);
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
            }
        });

        Ok((addr, handle, hits))
    }

    fn spawn_request_capture_server(response: Vec<u8>) -> Result<RequestCaptureServer> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| Error::transport("failed to bind test server", Some(Box::new(e))))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| Error::transport("failed to configure test server", Some(Box::new(e))))?;
        let addr = listener.local_addr().map_err(|e| {
            Error::transport("failed to read test server address", Some(Box::new(e)))
        })?;
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_thread = captured.clone();

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
                                Ok(n) => request.extend_from_slice(&buf[..n]),
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
                        *captured_thread.lock().expect("capture lock") = request;
                        let _ = stream.write_all(&response);
                        let _ = stream.flush();
                        return;
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

        Ok((addr, handle, captured))
    }

    #[test]
    fn send_bytes_sets_content_length() -> Result<()> {
        let (addr, handle, captured) = spawn_request_capture_server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        )?;
        let transport = BlockingTransport::new(
            RetryConfig {
                max_attempts: 1,
                ..RetryConfig::default()
            },
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        transport.send(
            Method::PUT,
            url,
            HeaderMap::new(),
            BlockingBody::Bytes(Bytes::from_static(b"abc")),
        )?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        let request = String::from_utf8(captured.lock().expect("capture lock").clone())
            .map_err(|e| Error::decode("captured request is not valid UTF-8", Some(Box::new(e))))?;
        let request = request.to_ascii_lowercase();
        assert_eq!(request.matches("\r\ncontent-length: ").count(), 1);
        assert!(request.contains("\r\ncontent-length: 3\r\n"));
        Ok(())
    }

    #[test]
    fn response_error_extracts_rate_limit() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-amz-request-id", http::HeaderValue::from_static("req-1"));
        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("3"),
        );

        let err = response_error_from_status(StatusCode::TOO_MANY_REQUESTS, &headers, "slow down");
        match err {
            Error::Api(error) => {
                assert!(error.is_rate_limited());
                let crate::ApiError {
                    retry_after,
                    request_id,
                    ..
                } = *error;
                assert_eq!(retry_after, Some(Duration::from_secs(3)));
                assert_eq!(request_id.as_deref(), Some("req-1"));
            }
            other => panic!("expected rate limited error, got {other:?}"),
        }
    }

    #[test]
    fn response_error_parses_xml_error_fields() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "x-amz-request-id",
            http::HeaderValue::from_static("req-outer"),
        );

        let body = r#"
<Error>
  <Code>AccessDenied</Code>
  <Message>Access Denied</Message>
  <RequestId>req-inner</RequestId>
  <HostId>host-1</HostId>
</Error>
"#;

        let err = response_error_from_status(StatusCode::FORBIDDEN, &headers, body);
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
                assert_eq!(request_id.as_deref(), Some("req-inner"));
                assert_eq!(host_id.as_deref(), Some("host-1"));
                assert!(body_snippet.unwrap_or_default().contains("AccessDenied"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }

    #[test]
    fn send_returns_api_error_for_http_error_status() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 1,
            ..RetryConfig::default()
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp_result = transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty);
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;
        let resp = resp_result.expect_err("expected service error");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::NOT_FOUND);
        Ok(())
    }

    #[test]
    fn send_transport_error_redacts_request_path() -> Result<()> {
        let retry = RetryConfig {
            max_attempts: 1,
            ..RetryConfig::default()
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_millis(100)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse("http://127.0.0.1:1/private/object/key?token=secret")
            .map_err(|_| Error::invalid_config("invalid test URL"))?;

        let err = match transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty) {
            Ok(_) => panic!("request should fail with transport error"),
            Err(err) => err,
        };
        let debug = format!("{err:?}");

        match err {
            Error::Transport { message, source } => {
                assert_eq!(message, "request failed");
                let source = source.expect("transport error should preserve a sanitized source");
                let source_text = source.to_string();
                assert!(source_text.contains("http://<redacted-host>:1/<redacted>"));
                assert!(debug.contains("http://<redacted-host>:1/<redacted>"));
                assert!(!message.contains("127.0.0.1"));
                assert!(!source_text.contains("127.0.0.1"));
                assert!(!debug.contains("127.0.0.1"));
                assert!(!message.contains("private/object/key"));
                assert!(!source_text.contains("private/object/key"));
                assert!(!debug.contains("private/object/key"));
                assert!(!message.contains("token=secret"));
                assert!(!source_text.contains("token=secret"));
                assert!(!debug.contains("token=secret"));
            }
            other => panic!("expected transport error, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn send_head_with_content_encoding_and_empty_body_returns_ok() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 200 OK\r\nContent-Encoding: zstd\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 1,
            ..RetryConfig::default()
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport.send(Method::HEAD, url, HeaderMap::new(), BlockingBody::Empty)?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(http::header::CONTENT_ENCODING)
                .and_then(|v| v.to_str().ok()),
            Some("zstd")
        );
        assert!(resp.body().is_empty());
        Ok(())
    }

    #[test]
    fn send_does_not_retry_on_non_retryable_4xx_status() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::FORBIDDEN);
        Ok(())
    }

    #[test]
    fn retry_limit_includes_transport_and_service_failures() -> Result<()> {
        for streaming in [false, true] {
            let (addr, handle, hits) = spawn_test_server(vec![
                Vec::new(),
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            ])?;
            let transport = BlockingTransport::new(
                RetryConfig {
                    max_attempts: 2,
                    base_delay: Duration::ZERO,
                    max_delay: Duration::ZERO,
                    ..RetryConfig::default()
                },
                None,
                Some(Duration::from_secs(5)),
                reqx::TlsRootStore::BackendDefault,
            )?;
            let url = Url::parse(&format!("http://{addr}/"))
                .map_err(|_| Error::invalid_config("invalid test server URL"))?;
            let status = if streaming {
                transport
                    .send_stream(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)
                    .expect_err("expected service error")
                    .status()
                    .unwrap()
            } else {
                transport
                    .send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)
                    .expect_err("expected service error")
                    .status()
                    .unwrap()
            };
            handle
                .join()
                .map_err(|_| Error::transport("test server thread panicked", None))?;
            assert_eq!(hits.load(Ordering::SeqCst), 2, "streaming={streaming}");
            assert_eq!(
                status,
                StatusCode::SERVICE_UNAVAILABLE,
                "streaming={streaming}"
            );
        }
        Ok(())
    }

    #[test]
    fn send_retries_on_retryable_5xx_status() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[test]
    fn send_exhausts_retryable_status_without_nested_status_retries() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status().unwrap(), StatusCode::SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[test]
    fn send_does_not_retry_on_retryable_status_for_non_idempotent_post() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(
                Method::POST,
                url,
                HeaderMap::new(),
                BlockingBody::Bytes(Bytes::from_static(b"hello")),
            )
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[test]
    fn send_retries_on_transport_error_for_replayable_body() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            Vec::new(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[test]
    fn send_retries_when_buffered_body_read_fails() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nabc".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[test]
    fn send_retries_on_embedded_retryable_service_error_xml() -> Result<()> {
        let error_xml =
            "<Error><Code>InternalError</Code><Message>backend failure</Message></Error>";
        let first = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            error_xml.len(),
            error_xml
        )
        .into_bytes();
        let (addr, handle, hits) = spawn_test_server(vec![
            first,
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[test]
    fn send_retries_on_retryable_service_error_code_from_4xx_body() -> Result<()> {
        let error_xml = "<Error><Code>SlowDown</Code><Message>slow down</Message></Error>";
        let first = format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            error_xml.len(),
            error_xml
        )
        .into_bytes();
        let (addr, handle, hits) = spawn_test_server(vec![
            first,
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[test]
    fn send_stream_retries_retryable_status_for_replayable_body() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 503 Service Unavailable\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let mut resp =
            transport.send_stream(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)?;
        let mut body = Vec::new();
        resp.read_to_end(&mut body)
            .map_err(|e| Error::transport("failed to read response body", Some(Box::new(e))))?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body, b"ok");
        Ok(())
    }

    #[test]
    fn send_stream_preserves_content_encoding_and_raw_bytes() -> Result<()> {
        let gzipped = vec![
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xcb, 0x48, 0xcd, 0xc9,
            0xc9, 0x07, 0x00, 0x86, 0xa6, 0x10, 0x36, 0x05, 0x00, 0x00, 0x00,
        ];
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            gzipped.len()
        );
        let mut wire = response.into_bytes();
        wire.extend_from_slice(&gzipped);
        let (addr, handle, _) = spawn_test_server(vec![wire])?;

        let retry = RetryConfig::default();
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp =
            transport.send_stream(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)?;
        assert_eq!(
            resp.headers()
                .get(http::header::CONTENT_ENCODING)
                .and_then(|v| v.to_str().ok()),
            Some("gzip")
        );
        let mut resp = resp;
        let mut out = Vec::new();
        resp.read_to_end(&mut out)
            .map_err(|e| Error::transport("failed to read response body", Some(Box::new(e))))?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;
        assert_eq!(out, gzipped);
        Ok(())
    }

    #[test]
    fn send_stream_body_read_error_is_observable() -> Result<()> {
        let (addr, handle, _) = spawn_test_server(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc".to_vec(),
        ])?;

        let retry = RetryConfig::default();
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp =
            transport.send_stream(Method::GET, url, HeaderMap::new(), BlockingBody::Empty)?;
        let mut resp = resp;
        let mut out = Vec::new();
        let read = resp.read_to_end(&mut out);
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;
        assert!(read.is_err(), "expected read error for truncated response");
        Ok(())
    }

    #[test]
    fn send_stream_into_response_limited_enforces_limit() -> Result<()> {
        let payload = "x".repeat(MAX_ERROR_RESPONSE_BODY_BYTES + 1);
        let response = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            payload.len(),
            payload
        );
        let (addr, handle, _) = spawn_test_server(vec![response.into_bytes()])?;

        let retry = RetryConfig::default();
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let limited =
            transport.send_stream(Method::GET, url, HeaderMap::new(), BlockingBody::Empty);
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;
        let error = limited.expect_err("oversized error response must fail");
        let Error::Transport {
            source: Some(source),
            ..
        } = error
        else {
            panic!("expected body limit error");
        };
        assert!(
            source.to_string().contains("response_body_too_large"),
            "{source}"
        );
        Ok(())
    }

    #[test]
    fn send_does_not_succeed_after_redirect_on_blocking_backend() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 301 Moved Permanently\r\nLocation: /next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig::default();
        let transport = BlockingTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let outcome = transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty);
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        let resp = outcome.expect_err("redirect must fail");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::MOVED_PERMANENTLY);
        Ok(())
    }

    #[test]
    fn send_does_not_succeed_after_query_only_redirect() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 302 Found\r\nLocation: /?next=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let transport = BlockingTransport::new(
            RetryConfig::default(),
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::System,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let outcome = transport.send(Method::GET, url, HeaderMap::new(), BlockingBody::Empty);
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        let resp = outcome.expect_err("redirect must fail");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::FOUND);
        Ok(())
    }

    #[test]
    fn object_download_retries_service_codes_and_preserves_xml_objects() -> Result<()> {
        let error_body = "<Error><Code>SlowDown</Code></Error>";
        let object_body = b"<Error><Code>AccessDenied</Code></Error>";
        let failures = [
            format!("HTTP/1.1 400 Bad Request\r\nRetry-After: 0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", error_body.len(), error_body).into_bytes(),
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 100\r\nConnection: close\r\n\r\ntruncated".to_vec(),
        ];
        for failure in failures {
            let (addr, handle, hits) = spawn_test_server(vec![
                failure,
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    object_body.len(),
                    std::str::from_utf8(object_body).unwrap()
                )
                .into_bytes(),
            ])?;
            let client = crate::BlockingClient::builder(format!("http://{addr}"))?
                .region("us-east-1")
                .max_attempts(2)
                .build()?;
            let object = client.objects().get("bucket", "key").send()?;
            let mut body = Vec::new();
            object.write_to(&mut body)?;
            handle.join().unwrap();
            assert_eq!(hits.load(Ordering::SeqCst), 2);
            assert_eq!(body, object_body);
        }
        Ok(())
    }
}
