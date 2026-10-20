#[cfg(any(feature = "async", feature = "blocking"))]
use crate::Error;
#[cfg(feature = "blocking")]
use crate::Result;
#[cfg(feature = "blocking")]
use std::sync::Condvar;
#[cfg(any(feature = "async", feature = "blocking"))]
use std::sync::Mutex;
#[cfg(any(feature = "async", feature = "blocking"))]
use std::sync::atomic::AtomicUsize;
#[cfg(any(feature = "async", feature = "blocking"))]
use std::sync::atomic::Ordering;
#[cfg(any(feature = "async", feature = "blocking"))]
use std::time::Duration;

use super::*;

#[test]
fn region_validates_non_empty() {
    assert!(Region::new("").is_err());
    assert!(Region::new("   ").is_err());
    assert!(Region::new(" us-east-1").is_err());
    assert!(Region::new("us-east-1 ").is_err());
    assert!(Region::new("us/east/1").is_err());
    assert!(Region::new("us east 1").is_err());
    assert!(Region::new("us-east-1\nprod").is_err());
    assert!(Region::new("us.east.1").is_err());
    assert!(Region::new("us-east-東京").is_err());
    assert!(Region::new("US-EAST-1").is_err());
    assert!(Region::new("us_east_1").is_err());
    assert!(Region::new("us-east-1").is_ok());
    assert!(Region::new("auto").is_ok());
}

#[test]
fn credentials_validate_and_redact_in_debug() {
    assert!(Credentials::new("", "secret").is_err());
    assert!(Credentials::new("akid", "").is_err());
    assert!(Credentials::new(" akid", "secret").is_err());
    assert!(Credentials::new("akid", "secret\n").is_err());
    assert!(Credentials::new("akid\tprod", "secret").is_err());
    assert!(Credentials::new("akid", "sec ret").is_err());
    assert!(Credentials::new("akidé", "secret").is_err());
    assert!(Credentials::new("akid", "secreté").is_err());

    let creds = Credentials::new("AKIA1234567890", "SECRET1234567890")
        .unwrap()
        .with_session_token("TOKEN1234567890")
        .unwrap();
    assert!(creds.clone().with_session_token(" TOKEN").is_err());
    assert!(creds.clone().with_session_token("TOK\nEN").is_err());
    assert!(creds.clone().with_session_token("TOKÉN").is_err());

    let dbg = format!("{creds:?}");
    assert!(!dbg.contains("SECRET1234567890"));
    assert!(!dbg.contains("TOKEN1234567890"));
    assert!(dbg.contains("<redacted>"));
}

#[cfg(any(feature = "async", feature = "blocking"))]
#[derive(Debug)]
struct CountingFailProvider {
    calls: std::sync::Arc<AtomicUsize>,
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl CredentialsProvider for CountingFailProvider {
    #[cfg(feature = "async")]
    fn credentials_async(&self) -> CredentialsFuture<'_> {
        let calls = self.calls.clone();
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(Error::invalid_config("refresh failed"))
        })
    }

    #[cfg(feature = "blocking")]
    fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(Error::invalid_config("refresh failed"))
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
#[derive(Debug)]
struct DelayedFailProvider {
    delay: Duration,
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl CredentialsProvider for DelayedFailProvider {
    #[cfg(feature = "async")]
    fn credentials_async(&self) -> CredentialsFuture<'_> {
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Err(Error::transport("refresh failed", None))
        })
    }

    #[cfg(feature = "blocking")]
    fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
        std::thread::sleep(self.delay);
        Err(Error::transport("refresh failed", None))
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
#[derive(Debug)]
struct CountingOkProvider {
    calls: std::sync::Arc<AtomicUsize>,
    expires_in: time::Duration,
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl CountingOkProvider {
    fn new(calls: std::sync::Arc<AtomicUsize>) -> Self {
        Self {
            calls,
            expires_in: time::Duration::seconds(3600),
        }
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl CredentialsProvider for CountingOkProvider {
    #[cfg(feature = "async")]
    fn credentials_async(&self) -> CredentialsFuture<'_> {
        let calls = self.calls.clone();
        let expires_in = self.expires_in;
        Box::pin(async move {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            let creds = Credentials::new(format!("AKIA_{n}"), "SECRET_TEST").unwrap();
            Ok(CredentialsSnapshot::new(creds)
                .with_expires_at(time::OffsetDateTime::now_utc() + expires_in))
        })
    }

    #[cfg(feature = "blocking")]
    fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let creds = Credentials::new(format!("AKIA_{n}"), "SECRET_TEST").unwrap();
        Ok(CredentialsSnapshot::new(creds)
            .with_expires_at(time::OffsetDateTime::now_utc() + self.expires_in))
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
#[derive(Debug)]
struct ExpiredOkProvider {
    calls: std::sync::Arc<AtomicUsize>,
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl CredentialsProvider for ExpiredOkProvider {
    #[cfg(feature = "async")]
    fn credentials_async(&self) -> CredentialsFuture<'_> {
        let calls = self.calls.clone();
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            let creds = Credentials::new("AKIA_EXPIRED", "SECRET_TEST").unwrap();
            Ok(CredentialsSnapshot::new(creds)
                .with_expires_at(time::OffsetDateTime::now_utc() - time::Duration::seconds(1)))
        })
    }

    #[cfg(feature = "blocking")]
    fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let creds = Credentials::new("AKIA_EXPIRED", "SECRET_TEST").unwrap();
        Ok(CredentialsSnapshot::new(creds)
            .with_expires_at(time::OffsetDateTime::now_utc() - time::Duration::seconds(1)))
    }
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_returns_stale_on_refresh_error_async() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingFailProvider {
        calls: calls.clone(),
    };

    let initial = CredentialsSnapshot::new(Credentials::new("AKIA_TEST", "SECRET_TEST").unwrap())
        .with_expires_at(time::OffsetDateTime::now_utc() + time::Duration::seconds(60));

    let cached = CachedProvider::new(inner)
        .min_refresh_interval(Duration::from_secs(0))
        .with_initial(initial.clone());

    let snapshot = cached.credentials_async().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        snapshot.credentials().access_key_id(),
        initial.credentials().access_key_id()
    );
    assert_eq!(snapshot.expires_at(), initial.expires_at());
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_singleflight_refresh_async() {
    use tokio::sync::oneshot;

    #[derive(Debug)]
    struct SlowProvider {
        calls: std::sync::Arc<AtomicUsize>,
        started: std::sync::Arc<Mutex<Option<oneshot::Sender<()>>>>,
        proceed: std::sync::Arc<tokio::sync::Notify>,
    }

    impl CredentialsProvider for SlowProvider {
        fn credentials_async(&self) -> CredentialsFuture<'_> {
            let calls = self.calls.clone();
            let started = self.started.clone();
            let proceed = self.proceed.clone();
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                if let Some(tx) = started.lock().unwrap_or_else(|p| p.into_inner()).take() {
                    let _ = tx.send(());
                }
                proceed.notified().await;
                let creds = Credentials::new("AKIA_TEST", "SECRET_TEST").unwrap();
                Ok(CredentialsSnapshot::new(creds)
                    .with_expires_at(time::OffsetDateTime::now_utc() + time::Duration::seconds(60)))
            })
        }

        #[cfg(feature = "blocking")]
        fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
            Err(Error::invalid_config(
                "blocking credentials not supported in async test",
            ))
        }
    }

    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let (tx, rx) = oneshot::channel();
    let inner = SlowProvider {
        calls: calls.clone(),
        started: std::sync::Arc::new(Mutex::new(Some(tx))),
        proceed: std::sync::Arc::new(tokio::sync::Notify::new()),
    };

    let cached = std::sync::Arc::new(
        CachedProvider::new(inner)
            .refresh_before(Duration::from_secs(0))
            .min_refresh_interval(Duration::from_secs(0)),
    );
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let cached = cached.clone();
        tasks.push(tokio::spawn(async move {
            cached
                .credentials_async()
                .await
                .map(|s| s.credentials().access_key_id().to_string())
        }));
    }

    let _ = rx.await;
    tokio::task::yield_now().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    cached.inner.proceed.notify_one();

    for task in tasks {
        let key = task.await.unwrap().unwrap();
        assert_eq!(key, "AKIA_TEST");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_recovers_and_wakes_waiters_after_cancelled_refresh() {
    #[derive(Debug)]
    struct CancelFirstProvider(AtomicUsize);

    impl CredentialsProvider for CancelFirstProvider {
        fn credentials_async(&self) -> CredentialsFuture<'_> {
            Box::pin(async {
                if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    std::future::pending::<()>().await;
                }
                Ok(CredentialsSnapshot::new(
                    Credentials::new("AKIA_RECOVERED", "SECRET_TEST").unwrap(),
                ))
            })
        }

        #[cfg(feature = "blocking")]
        fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
            Err(Error::invalid_config("async test provider"))
        }
    }

    let cached = std::sync::Arc::new(
        CachedProvider::new(CancelFirstProvider(AtomicUsize::new(0)))
            .min_refresh_interval(Duration::ZERO),
    );
    let mut refresh = cached.credentials_async();
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(refresh.as_mut().poll(&mut cx).is_pending());

    let waiter = {
        let cached = cached.clone();
        tokio::spawn(async move { cached.credentials_async().await })
    };
    tokio::task::yield_now().await;
    assert_eq!(cached.inner.0.load(Ordering::SeqCst), 1);
    drop(refresh);

    let snapshot = tokio::time::timeout(Duration::from_secs(2), waiter)
        .await
        .expect("cancelled refresh must wake waiters")
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.credentials().access_key_id(), "AKIA_RECOVERED");
    assert_eq!(cached.inner.0.load(Ordering::SeqCst), 2);
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_recovers_after_provider_panics() {
    #[derive(Debug)]
    struct PanicFirstProvider(AtomicUsize);

    impl CredentialsProvider for PanicFirstProvider {
        #[cfg(feature = "async")]
        fn credentials_async(&self) -> CredentialsFuture<'_> {
            Box::pin(async { self.credentials_blocking() })
        }

        fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
            assert_ne!(
                self.0.fetch_add(1, Ordering::SeqCst),
                0,
                "provider panicked"
            );
            Ok(CredentialsSnapshot::new(
                Credentials::new("AKIA_RECOVERED", "SECRET_TEST").unwrap(),
            ))
        }
    }

    let cached = CachedProvider::new(PanicFirstProvider(AtomicUsize::new(0)))
        .min_refresh_interval(Duration::ZERO);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cached.credentials_blocking()
        }))
        .is_err()
    );
    let snapshot = cached.credentials_blocking().unwrap();
    assert_eq!(snapshot.credentials().access_key_id(), "AKIA_RECOVERED");
    assert_eq!(cached.inner.0.load(Ordering::SeqCst), 2);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_refresh_before_and_throttle_async() {
    let now = time::OffsetDateTime::now_utc();

    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingOkProvider::new(calls.clone());

    let initial = CredentialsSnapshot::new(Credentials::new("OLD", "SECRET_TEST").unwrap())
        .with_expires_at(now + time::Duration::seconds(60));
    let cached = CachedProvider::new(inner)
        .refresh_before(Duration::from_secs(10))
        .with_initial(initial);

    let snapshot = cached.credentials_async().await.unwrap();
    assert_eq!(snapshot.credentials().access_key_id(), "OLD");
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingFailProvider {
        calls: calls.clone(),
    };
    let initial = CredentialsSnapshot::new(Credentials::new("STALE", "SECRET_TEST").unwrap())
        .with_expires_at(now + time::Duration::seconds(60));
    let cached = CachedProvider::new(inner)
        .min_refresh_interval(Duration::from_secs(60))
        .with_initial(initial.clone());

    let first = cached.credentials_async().await.unwrap();
    let second = cached.credentials_async().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.credentials().access_key_id(), "STALE");
    assert_eq!(second.credentials().access_key_id(), "STALE");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_huge_refresh_before_does_not_overflow_async() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingOkProvider::new(calls.clone());
    let initial = CredentialsSnapshot::new(Credentials::new("OLD", "SECRET_TEST").unwrap())
        .with_expires_at(time::OffsetDateTime::now_utc() + time::Duration::seconds(60));
    let cached = CachedProvider::new(inner)
        .refresh_before(Duration::MAX)
        .min_refresh_interval(Duration::from_secs(0))
        .with_initial(initial);

    let snapshot = cached.credentials_async().await.unwrap();

    assert_eq!(snapshot.credentials().access_key_id(), "AKIA_0");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_force_refresh_bypasses_throttle_async() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingOkProvider::new(calls.clone());
    let cached = CachedProvider::new(inner).min_refresh_interval(Duration::from_secs(60));

    let first = cached.force_refresh_async().await.unwrap();
    let second = cached.force_refresh_async().await.unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_ne!(
        first.credentials().access_key_id(),
        second.credentials().access_key_id()
    );
}

#[cfg(all(feature = "async", feature = "blocking"))]
#[tokio::test]
async fn cached_provider_blocking_path_is_safe_inside_tokio_runtime() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let cached = CachedProvider::new(CountingOkProvider::new(calls.clone()));

    let snapshot = cached.credentials_blocking().unwrap();

    assert_eq!(snapshot.credentials().access_key_id(), "AKIA_0");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_throttles_failed_refresh_without_cache_async() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingFailProvider {
        calls: calls.clone(),
    };
    let cached = CachedProvider::new(inner).min_refresh_interval(Duration::from_secs(60));

    let first = cached.credentials_async().await;
    assert!(first.is_err(), "initial refresh should fail");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let second = cached.credentials_async().await;
    match second {
        Err(Error::Transport { message, .. }) => {
            assert!(message.contains("credentials refresh throttled"));
        }
        other => panic!("expected throttled transport error, got {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_rejects_expired_refresh_without_cache_async() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let cached = CachedProvider::new(ExpiredOkProvider {
        calls: calls.clone(),
    })
    .min_refresh_interval(Duration::from_secs(0));

    match cached.credentials_async().await {
        Err(Error::InvalidConfig { message }) => assert!(message.contains("expired")),
        other => panic!("expected expired credentials error, got {other:?}"),
    }

    let second = cached.credentials_async().await;
    assert!(second.is_err(), "expired snapshot must not be cached");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn cached_provider_does_not_return_expired_stale_after_slow_failed_refresh_async() {
    let initial = CredentialsSnapshot::new(Credentials::new("AKIA_TEST", "SECRET_TEST").unwrap())
        .with_expires_at(time::OffsetDateTime::now_utc() + time::Duration::milliseconds(20));
    let cached = CachedProvider::new(DelayedFailProvider {
        delay: Duration::from_millis(120),
    })
    .min_refresh_interval(Duration::from_secs(0))
    .with_initial(initial);

    match cached.credentials_async().await {
        Err(Error::Transport { message, .. }) => {
            assert!(message.contains("refresh failed"));
        }
        other => panic!("expected refresh failure, got {other:?}"),
    }
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_returns_stale_on_refresh_error_blocking() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingFailProvider {
        calls: calls.clone(),
    };

    let initial = CredentialsSnapshot::new(Credentials::new("AKIA_TEST", "SECRET_TEST").unwrap())
        .with_expires_at(time::OffsetDateTime::now_utc() + time::Duration::seconds(60));

    let cached = CachedProvider::new(inner)
        .min_refresh_interval(Duration::from_secs(0))
        .with_initial(initial.clone());

    let snapshot = cached.credentials_blocking().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        snapshot.credentials().access_key_id(),
        initial.credentials().access_key_id()
    );
    assert_eq!(snapshot.expires_at(), initial.expires_at());
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_singleflight_refresh_blocking() {
    use std::{sync::mpsc, thread};

    #[derive(Debug)]
    struct SlowProvider {
        calls: std::sync::Arc<AtomicUsize>,
        started: std::sync::Arc<Mutex<Option<mpsc::Sender<()>>>>,
        state: std::sync::Arc<(Mutex<bool>, Condvar)>,
    }

    impl CredentialsProvider for SlowProvider {
        #[cfg(feature = "async")]
        fn credentials_async(&self) -> CredentialsFuture<'_> {
            Box::pin(async {
                Err(Error::invalid_config(
                    "async credentials not supported in blocking test",
                ))
            })
        }

        fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(tx) = self
                .started
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take()
            {
                let _ = tx.send(());
            }
            let (lock, cvar) = &*self.state;
            let mut ready = lock.lock().unwrap_or_else(|p| p.into_inner());
            while !*ready {
                ready = cvar.wait(ready).unwrap_or_else(|p| p.into_inner());
            }
            drop(ready);
            let creds = Credentials::new("AKIA_TEST", "SECRET_TEST").unwrap();
            Ok(CredentialsSnapshot::new(creds)
                .with_expires_at(time::OffsetDateTime::now_utc() + time::Duration::seconds(60)))
        }
    }

    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::channel();
    let state = std::sync::Arc::new((Mutex::new(false), Condvar::new()));
    let inner = SlowProvider {
        calls: calls.clone(),
        started: std::sync::Arc::new(Mutex::new(Some(tx))),
        state: state.clone(),
    };

    let cached = std::sync::Arc::new(
        CachedProvider::new(inner)
            .refresh_before(Duration::from_secs(0))
            .min_refresh_interval(Duration::from_secs(0)),
    );
    let mut threads = Vec::new();
    for _ in 0..10 {
        let cached = cached.clone();
        threads.push(thread::spawn(move || {
            cached
                .credentials_blocking()
                .map(|s| s.credentials().access_key_id().to_string())
        }));
    }

    let _ = rx.recv();
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    {
        let (lock, cvar) = &*state;
        let mut ready = lock.lock().unwrap_or_else(|p| p.into_inner());
        *ready = true;
        cvar.notify_all();
    }

    for t in threads {
        let key = t.join().unwrap().unwrap();
        assert_eq!(key, "AKIA_TEST");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_refresh_before_and_throttle_blocking() {
    let now = time::OffsetDateTime::now_utc();

    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingOkProvider::new(calls.clone());

    let initial = CredentialsSnapshot::new(Credentials::new("OLD", "SECRET_TEST").unwrap())
        .with_expires_at(now + time::Duration::seconds(60));
    let cached = CachedProvider::new(inner)
        .refresh_before(Duration::from_secs(10))
        .with_initial(initial);

    let snapshot = cached.credentials_blocking().unwrap();
    assert_eq!(snapshot.credentials().access_key_id(), "OLD");
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingFailProvider {
        calls: calls.clone(),
    };
    let initial = CredentialsSnapshot::new(Credentials::new("STALE", "SECRET_TEST").unwrap())
        .with_expires_at(now + time::Duration::seconds(60));
    let cached = CachedProvider::new(inner)
        .min_refresh_interval(Duration::from_secs(60))
        .with_initial(initial.clone());

    let first = cached.credentials_blocking().unwrap();
    let second = cached.credentials_blocking().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.credentials().access_key_id(), "STALE");
    assert_eq!(second.credentials().access_key_id(), "STALE");
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_huge_refresh_before_does_not_overflow_blocking() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingOkProvider::new(calls.clone());
    let initial = CredentialsSnapshot::new(Credentials::new("OLD", "SECRET_TEST").unwrap())
        .with_expires_at(time::OffsetDateTime::now_utc() + time::Duration::seconds(60));
    let cached = CachedProvider::new(inner)
        .refresh_before(Duration::MAX)
        .min_refresh_interval(Duration::from_secs(0))
        .with_initial(initial);

    let snapshot = cached.credentials_blocking().unwrap();

    assert_eq!(snapshot.credentials().access_key_id(), "AKIA_0");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_force_refresh_bypasses_throttle_blocking() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingOkProvider::new(calls.clone());
    let cached = CachedProvider::new(inner).min_refresh_interval(Duration::from_secs(60));

    let first = cached.force_refresh_blocking().unwrap();
    let second = cached.force_refresh_blocking().unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_ne!(
        first.credentials().access_key_id(),
        second.credentials().access_key_id()
    );
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_throttles_failed_refresh_without_cache_blocking() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let inner = CountingFailProvider {
        calls: calls.clone(),
    };
    let cached = CachedProvider::new(inner).min_refresh_interval(Duration::from_secs(60));

    let first = cached.credentials_blocking();
    assert!(first.is_err(), "initial refresh should fail");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let second = cached.credentials_blocking();
    match second {
        Err(Error::Transport { message, .. }) => {
            assert!(message.contains("credentials refresh throttled"));
        }
        other => panic!("expected throttled transport error, got {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_rejects_expired_refresh_without_cache_blocking() {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let cached = CachedProvider::new(ExpiredOkProvider {
        calls: calls.clone(),
    })
    .min_refresh_interval(Duration::from_secs(0));

    match cached.credentials_blocking() {
        Err(Error::InvalidConfig { message }) => assert!(message.contains("expired")),
        other => panic!("expected expired credentials error, got {other:?}"),
    }

    let second = cached.credentials_blocking();
    assert!(second.is_err(), "expired snapshot must not be cached");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[cfg(feature = "blocking")]
#[test]
fn cached_provider_does_not_return_expired_stale_after_slow_failed_refresh_blocking() {
    let initial = CredentialsSnapshot::new(Credentials::new("AKIA_TEST", "SECRET_TEST").unwrap())
        .with_expires_at(time::OffsetDateTime::now_utc() + time::Duration::milliseconds(20));
    let cached = CachedProvider::new(DelayedFailProvider {
        delay: Duration::from_millis(120),
    })
    .min_refresh_interval(Duration::from_secs(0))
    .with_initial(initial);

    match cached.credentials_blocking() {
        Err(Error::Transport { message, .. }) => {
            assert!(message.contains("refresh failed"));
        }
        other => panic!("expected refresh failure, got {other:?}"),
    }
}
