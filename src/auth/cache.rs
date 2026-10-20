#[cfg(feature = "blocking")]
use std::sync::Condvar;
#[cfg(any(feature = "async", feature = "blocking"))]
use std::sync::{Mutex, MutexGuard};
#[cfg(any(feature = "async", feature = "blocking"))]
use std::time::Duration;

#[cfg(any(feature = "async", feature = "blocking"))]
use time::OffsetDateTime;

#[cfg(any(feature = "async", feature = "blocking"))]
use crate::{Error, Result};

#[cfg(feature = "async")]
use super::CredentialsFuture;
#[cfg(any(feature = "async", feature = "blocking"))]
use super::{CredentialsProvider, CredentialsSnapshot};

#[cfg(any(feature = "async", feature = "blocking"))]
#[derive(Debug)]
struct CachedState {
    cached: Option<CredentialsSnapshot>,
    refreshing: bool,
    last_refresh_attempt: Option<std::time::Instant>,
}

#[cfg(any(feature = "async", feature = "blocking"))]
enum RefreshDecision {
    UseCached(CredentialsSnapshot),
    Wait,
    Refresh,
}

/// Cached credentials wrapper with refresh and throttling.
///
/// This wrapper adds three behaviors to an underlying [`CredentialsProvider`]:
///
/// - caches the latest usable [`CredentialsSnapshot`]
/// - refreshes early before expiry
/// - coalesces concurrent refreshes so only one caller performs the refresh work
#[cfg(any(feature = "async", feature = "blocking"))]
#[derive(Debug)]
pub struct CachedProvider<P> {
    pub(super) inner: P,
    refresh_before: Duration,
    min_refresh_interval: Duration,
    state: Mutex<CachedState>,
    #[cfg(feature = "blocking")]
    condvar: Condvar,
    #[cfg(feature = "async")]
    notify: tokio::sync::Notify,
}

#[cfg(any(feature = "async", feature = "blocking"))]
struct RefreshGuard<'a, P: CredentialsProvider> {
    provider: &'a CachedProvider<P>,
    active: bool,
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl<P: CredentialsProvider> RefreshGuard<'_, P> {
    fn finish(mut self, refreshed: Result<CredentialsSnapshot>) -> Result<CredentialsSnapshot> {
        let result = {
            let mut state = self.provider.lock_state();
            let result = CachedProvider::<P>::finish_refresh_state(&mut state, refreshed);
            self.active = false;
            result
        };
        self.provider.notify_refresh_waiters();
        result
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl<P: CredentialsProvider> Drop for RefreshGuard<'_, P> {
    fn drop(&mut self) {
        if self.active {
            // Cancellation or a provider panic must release the refresh slot.
            self.provider.lock_state().refreshing = false;
            self.provider.notify_refresh_waiters();
        }
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl<P> CachedProvider<P>
where
    P: CredentialsProvider,
{
    /// Wraps a provider with caching and refresh behavior.
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            refresh_before: Duration::from_secs(300),
            min_refresh_interval: Duration::from_secs(5),
            state: Mutex::new(CachedState {
                cached: None,
                refreshing: false,
                last_refresh_attempt: None,
            }),
            #[cfg(feature = "blocking")]
            condvar: Condvar::new(),
            #[cfg(feature = "async")]
            notify: tokio::sync::Notify::new(),
        }
    }

    /// Sets how long before expiration to refresh.
    pub fn refresh_before(mut self, duration: Duration) -> Self {
        self.refresh_before = duration;
        self
    }

    /// Sets the minimum interval between refresh attempts.
    pub fn min_refresh_interval(mut self, duration: Duration) -> Self {
        self.min_refresh_interval = duration;
        self
    }

    /// Seeds the cache with an initial snapshot.
    pub fn with_initial(mut self, snapshot: CredentialsSnapshot) -> Self {
        self.state
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cached = Some(snapshot);
        self
    }

    /// Forces a refresh in async mode.
    #[cfg(feature = "async")]
    pub async fn force_refresh_async(&self) -> Result<CredentialsSnapshot> {
        self.get_async(true).await
    }

    /// Forces a refresh in blocking mode.
    #[cfg(feature = "blocking")]
    pub fn force_refresh_blocking(&self) -> Result<CredentialsSnapshot> {
        self.get_blocking(true)
    }

    fn should_refresh(
        &self,
        snapshot: &CredentialsSnapshot,
        now: OffsetDateTime,
        force: bool,
    ) -> bool {
        if force {
            return true;
        }
        match snapshot.expires_at() {
            Some(expires_at) => {
                let Ok(refresh_before) = time::Duration::try_from(self.refresh_before) else {
                    return true;
                };
                now.checked_add(refresh_before)
                    .is_none_or(|refresh_at| refresh_at >= expires_at)
            }
            None => false,
        }
    }

    fn is_expired(snapshot: &CredentialsSnapshot, now: OffsetDateTime) -> bool {
        snapshot
            .expires_at()
            .is_some_and(|expires_at| now >= expires_at)
    }

    fn refresh_throttle_remaining(
        &self,
        state: &CachedState,
        now: std::time::Instant,
    ) -> Option<Duration> {
        let last = state.last_refresh_attempt?;
        let elapsed = now.saturating_duration_since(last);
        if elapsed >= self.min_refresh_interval {
            None
        } else {
            Some(self.min_refresh_interval - elapsed)
        }
    }

    fn throttled_refresh_error(retry_after: Duration) -> Error {
        Error::transport(
            format!(
                "credentials refresh throttled; retry after {}ms",
                retry_after.as_millis()
            ),
            None,
        )
    }

    fn begin_refresh(
        &self,
        state: &mut CachedState,
        now_utc: OffsetDateTime,
        now: std::time::Instant,
        force: bool,
    ) -> Result<RefreshDecision> {
        if let Some(cached) = state.cached.as_ref() {
            if !self.should_refresh(cached, now_utc, force) {
                return Ok(RefreshDecision::UseCached(cached.clone()));
            }

            if !force
                && !Self::is_expired(cached, now_utc)
                && self.refresh_throttle_remaining(state, now).is_some()
            {
                return Ok(RefreshDecision::UseCached(cached.clone()));
            }
        }

        if state.refreshing {
            return Ok(RefreshDecision::Wait);
        }

        let has_usable_fallback = state
            .cached
            .as_ref()
            .is_some_and(|cached| !Self::is_expired(cached, now_utc));
        if !force
            && !has_usable_fallback
            && let Some(retry_after) = self.refresh_throttle_remaining(state, now)
        {
            return Err(Self::throttled_refresh_error(retry_after));
        }

        state.refreshing = true;
        state.last_refresh_attempt = Some(now);
        Ok(RefreshDecision::Refresh)
    }

    fn finish_refresh_state(
        state: &mut CachedState,
        refreshed: Result<CredentialsSnapshot>,
    ) -> Result<CredentialsSnapshot> {
        state.refreshing = false;
        let now = OffsetDateTime::now_utc();

        let refresh_error = match refreshed {
            Ok(snapshot) if !Self::is_expired(&snapshot, now) => {
                state.cached = Some(snapshot.clone());
                return Ok(snapshot);
            }
            Ok(_) => Error::invalid_config("credentials are expired"),
            Err(err) => err,
        };

        if let Some(snapshot) = state.cached.as_ref().filter(|s| !Self::is_expired(s, now)) {
            Ok(snapshot.clone())
        } else {
            state.cached = None;
            Err(refresh_error)
        }
    }

    fn notify_refresh_waiters(&self) {
        #[cfg(feature = "async")]
        self.notify.notify_waiters();

        #[cfg(feature = "blocking")]
        self.condvar.notify_all();
    }

    fn lock_state(&self) -> MutexGuard<'_, CachedState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(feature = "blocking")]
    fn wait_for_blocking_refresh(&self) {
        let mut state = self.lock_state();
        while state.refreshing {
            state = self
                .condvar
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    #[cfg(feature = "blocking")]
    fn get_blocking(&self, force: bool) -> Result<CredentialsSnapshot> {
        use std::time::Instant;

        loop {
            let decision = self.begin_refresh(
                &mut self.lock_state(),
                OffsetDateTime::now_utc(),
                Instant::now(),
                force,
            )?;

            match decision {
                RefreshDecision::UseCached(snapshot) => return Ok(snapshot),
                RefreshDecision::Wait => {
                    self.wait_for_blocking_refresh();
                    continue;
                }
                RefreshDecision::Refresh => {
                    let guard = RefreshGuard {
                        provider: self,
                        active: true,
                    };
                    let refreshed = self.inner.credentials_blocking();
                    return guard.finish(refreshed);
                }
            }
        }
    }

    #[cfg(feature = "async")]
    async fn get_async(&self, force: bool) -> Result<CredentialsSnapshot> {
        use std::time::Instant;

        loop {
            // Subscribe before inspecting state so completion cannot be missed.
            let notified = self.notify.notified();
            let decision = self.begin_refresh(
                &mut self.lock_state(),
                OffsetDateTime::now_utc(),
                Instant::now(),
                force,
            )?;
            match decision {
                RefreshDecision::UseCached(snapshot) => return Ok(snapshot),
                RefreshDecision::Wait => notified.await,
                RefreshDecision::Refresh => {
                    let guard = RefreshGuard {
                        provider: self,
                        active: true,
                    };
                    let refreshed = self.inner.credentials_async().await;
                    return guard.finish(refreshed);
                }
            }
        }
    }
}

#[cfg(any(feature = "async", feature = "blocking"))]
impl<P> CredentialsProvider for CachedProvider<P>
where
    P: CredentialsProvider,
{
    #[cfg(feature = "async")]
    fn credentials_async(&self) -> CredentialsFuture<'_> {
        Box::pin(async move { self.get_async(false).await })
    }

    #[cfg(feature = "blocking")]
    fn credentials_blocking(&self) -> Result<CredentialsSnapshot> {
        self.get_blocking(false)
    }
}
