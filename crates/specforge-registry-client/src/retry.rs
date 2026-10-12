//! A rate-limited registry call is sent again (`retry_registry_request`, ADR 0044 as amended by plan 16).

use std::sync::Arc;
use std::time::Duration;

use specforge_package::PackageName;
use specforge_package::Version;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry_wire::{PackageMetadata, PackageSignature, SearchHit, SearchQuery};

use crate::registry_client::{RegistryClient, RegistryError};
use crate::registry_config::{RegistryConfig, RegistryCredential};

/// How long to wait before each retry of a rate-limited registry call, and how many retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// The first retry's backoff; each later one doubles it.
    pub base: Duration,
    /// The longest wait: a backoff is capped at it, and a `Retry-After` longer than it is not waited for.
    pub max_delay: Duration,
    /// How many times a call is sent again.
    pub max_retries: u32,
}

impl RetryPolicy {
    /// The spec's policy: 1 s, doubling, at most 30 s, at most 3 retries.
    pub const REGISTRY: RetryPolicy = RetryPolicy {
        base: Duration::from_secs(1),
        max_delay: Duration::from_secs(30),
        max_retries: 3,
    };

    /// The backoff before retry `attempt` (0 for the first): `base * 2^attempt`, at most `max_delay`.
    pub fn backoff(&self, attempt: u32) -> Duration {
        let factor = 2u32.saturating_pow(attempt);
        self.base.saturating_mul(factor).min(self.max_delay)
    }

    /// The wait before retry `attempt` after a registry asked for `retry_after`: the longer of the backoff and
    /// `retry_after`. `None` when `attempt` is `max_retries` or more, or when `retry_after` exceeds `max_delay`
    /// (the registry will not answer sooner, so the call fails now with its R003).
    pub fn wait(&self, attempt: u32, retry_after: Duration) -> Option<Duration> {
        if attempt >= self.max_retries || retry_after > self.max_delay {
            return None;
        }
        Some(self.backoff(attempt).max(retry_after))
    }
}

/// A [`RegistryClient`] that answers as `C` does, except that a call answered `RateLimited` is sent again after
/// [`RetryPolicy::wait`], until it is answered otherwise or the policy gives up; the last `RateLimited` is then
/// the answer. Every call is retried alike (versions, metadata, download, search, publish, authenticate). A
/// publish is safe to resend: a registry refuses a rate-limited publish before it reads the upload
/// (`specforge-registry-server`'s `publish_package`). Timeouts and every other error pass through: R004 already
/// says to retry.
pub struct Retrying<C> {
    inner: C,
    policy: RetryPolicy,
    sleep: Arc<dyn Fn(Duration) + Send + Sync>,
}

impl<C: RegistryClient> Retrying<C> {
    /// `inner` under [`RetryPolicy::REGISTRY`], waiting with `std::thread::sleep`.
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            policy: RetryPolicy::REGISTRY,
            sleep: Arc::new(std::thread::sleep),
        }
    }

    /// Retry by `policy` instead.
    pub fn with_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Wait with `sleep` instead of the thread's (a test records the waits).
    pub fn with_sleep(mut self, sleep: impl Fn(Duration) + Send + Sync + 'static) -> Self {
        self.sleep = Arc::new(sleep);
        self
    }
}

impl<C> Retrying<C> {
    /// `call`, sent again while it answers `RateLimited` and the policy allows.
    fn sent<T>(&self, call: impl Fn() -> Result<T, RegistryError>) -> Result<T, RegistryError> {
        let mut attempt = 0;
        loop {
            match call() {
                Err(RegistryError::RateLimited { retry_after_ms }) => {
                    let asked = Duration::from_millis(retry_after_ms);
                    match self.policy.wait(attempt, asked) {
                        Some(wait) => {
                            (self.sleep)(wait);
                            attempt += 1;
                        }
                        None => return Err(RegistryError::RateLimited { retry_after_ms }),
                    }
                }
                answer => return answer,
            }
        }
    }
}

impl<C: RegistryClient> RegistryClient for Retrying<C> {
    fn versions(
        &self,
        name: &PackageName,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<String>, RegistryError> {
        self.sent(|| self.inner.versions(name, registry, credential))
    }

    fn metadata(
        &self,
        name: &PackageName,
        version: &Version,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<PackageMetadata, RegistryError> {
        self.sent(|| self.inner.metadata(name, version, registry, credential))
    }

    fn download(
        &self,
        wasm_url: &str,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<u8>, RegistryError> {
        self.sent(|| self.inner.download(wasm_url, registry, credential))
    }

    fn search(
        &self,
        query: &SearchQuery,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<Vec<SearchHit>, RegistryError> {
        self.sent(|| self.inner.search(query, registry, credential))
    }

    fn publish(
        &self,
        package: &[u8],
        declaration: &ExtensionDeclaration,
        manifest_json: &str,
        signature: &PackageSignature,
        registry: &RegistryConfig,
        credential: Option<&RegistryCredential>,
    ) -> Result<String, RegistryError> {
        self.sent(|| {
            self.inner.publish(
                package,
                declaration,
                manifest_json,
                signature,
                registry,
                credential,
            )
        })
    }

    fn authenticate(
        &self,
        registry: &RegistryConfig,
        credential: &RegistryCredential,
    ) -> Result<Option<String>, RegistryError> {
        self.sent(|| self.inner.authenticate(registry, credential))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_the_cap() {
        let policy = RetryPolicy::REGISTRY;
        let seconds: Vec<u64> = (0..=5).map(|a| policy.backoff(a).as_secs()).collect();
        assert_eq!(seconds, [1, 2, 4, 8, 16, 30]);
    }

    #[test]
    fn wait_is_none_past_the_last_retry_or_beyond_the_cap() {
        let policy = RetryPolicy::REGISTRY;
        let zero = Duration::ZERO;
        assert_eq!(policy.wait(0, zero), Some(Duration::from_secs(1)));
        assert_eq!(policy.wait(2, zero), Some(Duration::from_secs(4)));
        assert_eq!(policy.wait(3, zero), None);
        assert_eq!(
            policy.wait(0, Duration::from_secs(5)),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            policy.wait(0, Duration::from_secs(30)),
            Some(Duration::from_secs(30))
        );
        assert_eq!(policy.wait(0, Duration::from_secs(31)), None);
    }
}
