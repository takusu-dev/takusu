//! TTL-based cache for token verification results.
//!
//! Avoids a storage round-trip on every authenticated request. Cache key is
//! the SHA-256 hash of the token (raw tokens are never stored). Cache values
//! expire after `TAKUSU_TOKEN_CACHE_TTL_SECS` seconds (default 60).
//!
//! Invariant: a cached `Invalid` answer is also valid until TTL. To make
//! revocation effective sooner, call `invalidate` after revoking.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenState {
    Valid,
    Invalid,
}

struct Entry {
    state: TokenState,
    expires_at: Instant,
}

pub struct TokenCache {
    inner: Mutex<HashMap<String, Entry>>,
    ttl: Duration,
}

impl TokenCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    pub fn with_default_ttl() -> Self {
        let secs: u64 = std::env::var("TAKUSU_TOKEN_CACHE_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        Self::new(Duration::from_secs(secs))
    }

    pub fn get(&self, token: &str) -> Option<TokenState> {
        let hash = Self::hash(token);
        let mut guard = self.inner.lock().ok()?;
        let entry = guard.get(&hash)?;
        if entry.expires_at <= Instant::now() {
            guard.remove(&hash);
            return None;
        }
        Some(entry.state)
    }

    pub fn put(&self, token: &str, state: TokenState) {
        let hash = Self::hash(token);
        let entry = Entry {
            state,
            expires_at: Instant::now() + self.ttl,
        };
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(hash, entry);
        }
    }

    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.clear();
        }
    }

    fn hash(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_returns_state() {
        let cache = TokenCache::new(Duration::from_secs(10));
        cache.put("abc", TokenState::Valid);
        assert_eq!(cache.get("abc"), Some(TokenState::Valid));
    }

    #[test]
    fn miss_returns_none() {
        let cache = TokenCache::new(Duration::from_secs(10));
        assert_eq!(cache.get("xyz"), None);
    }

    #[test]
    fn expired_entry_returns_none() {
        let cache = TokenCache::new(Duration::from_millis(0));
        cache.put("abc", TokenState::Valid);
        std::thread::sleep(Duration::from_millis(1));
        assert_eq!(cache.get("abc"), None);
    }

    #[test]
    fn invalidate_clears_all() {
        let cache = TokenCache::new(Duration::from_secs(10));
        cache.put("a", TokenState::Valid);
        cache.put("b", TokenState::Invalid);
        cache.invalidate();
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), None);
    }

    #[test]
    fn invalid_state_is_also_cached() {
        let cache = TokenCache::new(Duration::from_secs(10));
        cache.put("a", TokenState::Invalid);
        assert_eq!(cache.get("a"), Some(TokenState::Invalid));
    }
}
