use std::{pin::Pin, time::Duration};

use bytes::Bytes;
use futures_core::Stream;
use futures_util::TryStreamExt as _;
#[cfg(test)]
use http::StatusCode;
use http::header::{CONTENT_LENGTH, USER_AGENT};
use http::{HeaderMap, HeaderValue, Method};
use reqx::{
    Client as HttpClient,
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

type AsyncByteStream = Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send + 'static>>;

pub(crate) fn boxed_byte_stream<S, E>(stream: S) -> AsyncByteStream
where
    S: Stream<Item = std::result::Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    Box::pin(stream.map_err(std::io::Error::other))
}

pub(crate) type AsyncBody = RequestBody<AsyncByteStream>;

pub(crate) struct AsyncTransport {
    client: HttpClient,
    retry: RetryConfig,
    user_agent: HeaderValue,
}

impl AsyncTransport {
    pub(crate) fn new(
        retry: RetryConfig,
        user_agent: Option<String>,
        timeout: Option<Duration>,
        tls_root_store: TlsRootStore,
    ) -> Result<Self> {
        let retry = retry.validate()?;
        validate_request_timeout(timeout)?;
        let (user_agent_text, user_agent) = prepare_user_agent(user_agent)?;

        let mut builder = HttpClient::builder("http://localhost")
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

    pub(crate) async fn send(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        mut body: AsyncBody,
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
            let resp = match req.send().await {
                Ok(resp) => resp,
                Err(err) => {
                    if let Some(delay) =
                        transport_retry_delay(self.retry, attempt, max_attempts, &method, &err)
                    {
                        record_retry(&method, "transport");
                        tokio::time::sleep(delay).await;
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
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                record_service_error(&method);
                return Err(error);
            }

            return Ok(resp);
        }
    }

    pub(crate) async fn send_stream(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        mut body: AsyncBody,
    ) -> Result<reqx::ResponseStream> {
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
            let resp = match req.send_stream().await {
                Ok(resp) => resp,
                Err(err) => {
                    if let Some(delay) =
                        transport_retry_delay(self.retry, attempt, max_attempts, &method, &err)
                    {
                        record_retry(&method, "transport");
                        tokio::time::sleep(delay).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(map_reqx_error("request failed", err));
                }
            };

            if resp.status().is_success() {
                return Ok(resp);
            }

            let response = match resp
                .into_response_limited(MAX_ERROR_RESPONSE_BODY_BYTES)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    if let Some(delay) =
                        transport_retry_delay(self.retry, attempt, max_attempts, &method, &error)
                    {
                        record_retry(&method, "transport");
                        tokio::time::sleep(delay).await;
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
                tokio::time::sleep(delay).await;
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
        body: AsyncBody,
    ) -> Result<reqx::RequestBuilder> {
        ensure_method_accepts_body(method, &body)?;

        let mut req = self
            .client
            .request(method.clone(), url.as_str().to_string())
            .header(USER_AGENT, self.user_agent.clone());

        for name in headers.keys() {
            req = req.header(name.clone(), headers[name].clone());
        }

        req = match body {
            AsyncBody::Empty => req,
            AsyncBody::Bytes(b) => req.body(b),
            AsyncBody::Stream {
                stream,
                content_length: Some(len),
            } => req
                .header(CONTENT_LENGTH, HeaderValue::from(len))
                .body_stream(stream),
            AsyncBody::Stream {
                stream,
                content_length: None,
            } => req.body_stream(stream),
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

    #[tokio::test]
    async fn send_bytes_sets_content_length() -> Result<()> {
        let (addr, handle, captured) = spawn_request_capture_server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        )?;
        let transport = AsyncTransport::new(
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

        transport
            .send(
                Method::PUT,
                url,
                HeaderMap::new(),
                AsyncBody::Bytes(Bytes::from_static(b"abc")),
            )
            .await?;
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

    #[tokio::test]
    async fn send_returns_api_error_for_http_error_status() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 1,
            ..RetryConfig::default()
        };
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::NOT_FOUND);
        Ok(())
    }

    #[tokio::test]
    async fn send_head_with_content_encoding_and_empty_body_returns_ok() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 200 OK\r\nContent-Encoding: zstd\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 1,
            ..RetryConfig::default()
        };
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::HEAD, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
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

    #[tokio::test]
    async fn send_stream_surfaces_embedded_service_error_on_2xx() -> Result<()> {
        let body = "<Error><Code>AccessDenied</Code><Message>denied</Message></Error>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (addr, handle, hits) = spawn_test_server(vec![response.into_bytes()])?;

        let retry = RetryConfig {
            max_attempts: 1,
            ..RetryConfig::default()
        };
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let stream = futures_util::stream::once(async {
            Ok::<_, std::io::Error>(Bytes::from_static(b"hello"))
        });
        let err = transport
            .send(
                Method::PUT,
                url,
                HeaderMap::new(),
                AsyncBody::Stream {
                    stream: boxed_byte_stream(stream),
                    content_length: Some(5),
                },
            )
            .await
            .expect_err("expected embedded error xml to fail request");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        match err {
            Error::Api(error) => {
                let crate::ApiError {
                    status,
                    code,
                    message,
                    ..
                } = *error;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(code.as_deref(), Some("AccessDenied"));
                assert_eq!(message.as_deref(), Some("denied"));
            }
            other => panic!("expected api error, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn response_error_extracts_rate_limit() -> Result<()> {
        let (addr, handle, _) = spawn_test_server(vec![
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 3\r\nx-amz-request-id: req-1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 1,
            ..RetryConfig::default()
        };
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        let err = resp;
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
        Ok(())
    }

    #[tokio::test]
    async fn response_error_parses_xml_error_fields() -> Result<()> {
        let body = r#"
<Error>
  <Code>AccessDenied</Code>
  <Message>Access Denied</Message>
  <RequestId>req-inner</RequestId>
  <HostId>host-1</HostId>
</Error>
"#;
        let response = format!(
            "HTTP/1.1 403 Forbidden\r\nx-amz-request-id: req-outer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (addr, handle, _) = spawn_test_server(vec![response.into_bytes()])?;

        let retry = RetryConfig {
            max_attempts: 1,
            ..RetryConfig::default()
        };
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        let err = resp;
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
        Ok(())
    }

    #[tokio::test]
    async fn retry_limit_includes_transport_and_service_failures() -> Result<()> {
        for streaming in [false, true] {
            let (addr, handle, hits) = spawn_test_server(vec![
                Vec::new(),
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            ])?;
            let transport = AsyncTransport::new(
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
                    .send_stream(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
                    .await
                    .expect_err("expected service error")
                    .status()
                    .unwrap()
            } else {
                transport
                    .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
                    .await
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

    #[tokio::test]
    async fn send_retries_on_retryable_status_for_replayable_body() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn send_does_not_retry_on_non_retryable_4xx_status() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::FORBIDDEN);
        Ok(())
    }

    #[tokio::test]
    async fn send_retries_on_retryable_5xx_status() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn send_exhausts_retryable_status_without_nested_status_retries() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status().unwrap(), StatusCode::SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[tokio::test]
    async fn send_does_not_retry_on_retryable_status_for_non_idempotent_post() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(
                Method::POST,
                url,
                HeaderMap::new(),
                AsyncBody::Bytes(Bytes::from_static(b"hello")),
            )
            .await
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::SERVICE_UNAVAILABLE);
        Ok(())
    }

    #[tokio::test]
    async fn send_retries_on_transport_error_for_replayable_body() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn send_retries_when_buffered_body_read_fails() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn send_retries_on_embedded_retryable_service_error_xml() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn send_retries_on_retryable_service_error_code_from_4xx_body() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn send_stream_does_not_retry_for_non_replayable_body() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_millis(0),
            max_retry_after: Duration::from_secs(30),
        };
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;
        let body = AsyncBody::Stream {
            stream: boxed_byte_stream(futures_util::stream::empty::<
                std::result::Result<Bytes, std::io::Error>,
            >()),
            content_length: Some(0),
        };

        let resp = transport
            .send_stream(Method::PUT, url, HeaderMap::new(), body)
            .await
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::TOO_MANY_REQUESTS);
        Ok(())
    }

    #[tokio::test]
    async fn send_stream_retries_retryable_status_for_replayable_body() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let mut resp = transport
            .send_stream(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        use tokio::io::AsyncReadExt as _;
        let mut body = Vec::new();
        resp.read_to_end(&mut body)
            .await
            .map_err(|e| Error::transport("body stream error", Some(Box::new(e))))?;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body, b"ok");
        Ok(())
    }

    #[tokio::test]
    async fn send_rejects_body_for_get() -> Result<()> {
        let transport = AsyncTransport::new(
            RetryConfig::default(),
            None,
            Some(Duration::from_secs(1)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse("http://127.0.0.1:9/")
            .map_err(|_| Error::invalid_config("invalid test URL"))?;

        let err = transport
            .send(
                Method::GET,
                url,
                HeaderMap::new(),
                AsyncBody::Bytes(Bytes::from_static(b"body")),
            )
            .await
            .expect_err("GET body should be rejected");
        match err {
            Error::InvalidConfig { message } => {
                assert!(message.contains("does not accept a request body"));
            }
            other => panic!("expected invalid config, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn send_stream_preserves_content_encoding_and_raw_bytes() -> Result<()> {
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
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send_stream(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        assert_eq!(
            resp.headers()
                .get(http::header::CONTENT_ENCODING)
                .and_then(|v| v.to_str().ok()),
            Some("gzip")
        );

        use tokio::io::AsyncReadExt as _;
        let mut resp = resp;
        let mut out = Vec::new();
        resp.read_to_end(&mut out)
            .await
            .map_err(|e| Error::transport("body stream error", Some(Box::new(e))))?;

        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;
        assert_eq!(out, gzipped);
        Ok(())
    }

    #[tokio::test]
    async fn send_stream_body_read_error_is_observable() -> Result<()> {
        let response =
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc".to_vec();
        let (addr, handle, _) = spawn_test_server(vec![response])?;

        let retry = RetryConfig::default();
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send_stream(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await?;
        use tokio::io::AsyncReadExt as _;
        let mut resp = resp;
        let mut saw_error = false;
        let mut out = Vec::new();
        if resp.read_to_end(&mut out).await.is_err() {
            saw_error = true;
        }
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;
        assert!(
            saw_error,
            "expected body stream error for truncated response"
        );
        Ok(())
    }

    #[tokio::test]
    async fn send_stream_into_response_limited_enforces_limit() -> Result<()> {
        let payload = "x".repeat(MAX_ERROR_RESPONSE_BODY_BYTES + 1);
        let response = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            payload.len(),
            payload
        );
        let (addr, handle, _) = spawn_test_server(vec![response.into_bytes()])?;

        let retry = RetryConfig::default();
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let limited = transport
            .send_stream(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await;
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

    #[tokio::test]
    async fn send_does_not_follow_redirect_by_default() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 301 Moved Permanently\r\nLocation: /next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let retry = RetryConfig::default();
        let transport = AsyncTransport::new(
            retry,
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let resp = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await
            .expect_err("expected service error");
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::MOVED_PERMANENTLY);
        Ok(())
    }

    #[tokio::test]
    async fn send_does_not_succeed_after_query_only_redirect() -> Result<()> {
        let (addr, handle, hits) = spawn_test_server(vec![
            b"HTTP/1.1 302 Found\r\nLocation: /?next=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ])?;

        let transport = AsyncTransport::new(
            RetryConfig::default(),
            None,
            Some(Duration::from_secs(5)),
            reqx::TlsRootStore::BackendDefault,
        )?;
        let url = Url::parse(&format!("http://{addr}/"))
            .map_err(|_| Error::invalid_config("invalid test server URL"))?;

        let outcome = transport
            .send(Method::GET, url, HeaderMap::new(), AsyncBody::Empty)
            .await;
        handle
            .join()
            .map_err(|_| Error::transport("test server thread panicked", None))?;

        let resp = outcome.expect_err("redirect must fail");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(resp.status().unwrap(), StatusCode::FOUND);
        Ok(())
    }

    #[tokio::test]
    async fn object_download_retries_service_codes_and_preserves_xml_objects() -> Result<()> {
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
            let client = crate::Client::builder(format!("http://{addr}"))?
                .region("us-east-1")
                .max_attempts(2)
                .build()?;
            let object = client.objects().get("bucket", "key").send().await?;
            let mut body = Vec::new();
            object.write_to(&mut body).await?;
            handle.join().unwrap();
            assert_eq!(hits.load(Ordering::SeqCst), 2);
            assert_eq!(body, object_body);
        }
        Ok(())
    }
}
