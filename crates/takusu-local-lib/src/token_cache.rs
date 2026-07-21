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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use takusu_util::{TokenClaims, jwt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenState {
    Valid(TokenClaims),
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

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn is_claims_expired(claims: &TokenClaims) -> bool {
    claims.exp.is_some_and(|exp| exp <= now_seconds())
}

fn entry_ttl(base: Duration, state: &TokenState) -> Duration {
    let TokenState::Valid(claims) = state else {
        return base;
    };
    let Some(exp) = claims.exp else {
        return base;
    };
    let now = now_seconds();
    if exp <= now {
        // Already expired; expire immediately.
        return Duration::from_secs(0);
    }
    let remaining = (exp - now) as u64;
    std::cmp::min(base, Duration::from_secs(remaining))
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
        if let TokenState::Valid(ref claims) = entry.state
            && is_claims_expired(claims)
        {
            let invalid = Entry {
                state: TokenState::Invalid,
                expires_at: Instant::now() + self.ttl,
            };
            guard.insert(hash, invalid);
            return Some(TokenState::Invalid);
        }
        Some(entry.state.clone())
    }

    pub fn put(&self, token: &str, state: TokenState) {
        let hash = Self::hash(token);
        let ttl = entry_ttl(self.ttl, &state);
        let entry = Entry {
            state,
            expires_at: Instant::now() + ttl,
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
        jwt::hex(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_claims(scope: &str) -> TokenClaims {
        TokenClaims {
            sub: "sub".into(),
            jti: "jti".into(),
            scope: scope.into(),
            label: None,
            aud: takusu_util::DEFAULT_AUD.into(),
            iss: takusu_util::DEFAULT_ISS.into(),
            iat: 0,
            exp: None,
        }
    }

    #[test]
    fn hit_returns_state() {
        let cache = TokenCache::new(Duration::from_secs(10));
        cache.put("abc", TokenState::Valid(dummy_claims("read-write")));
        assert!(matches!(cache.get("abc"), Some(TokenState::Valid(_))));
    }

    #[test]
    fn miss_returns_none() {
        let cache = TokenCache::new(Duration::from_secs(10));
        assert_eq!(cache.get("xyz"), None);
    }

    #[test]
    fn expired_entry_returns_none() {
        let cache = TokenCache::new(Duration::from_millis(0));
        cache.put("abc", TokenState::Valid(dummy_claims("read-write")));
        std::thread::sleep(Duration::from_millis(1));
        assert_eq!(cache.get("abc"), None);
    }

    #[test]
    fn invalidate_clears_all() {
        let cache = TokenCache::new(Duration::from_secs(10));
        cache.put("a", TokenState::Valid(dummy_claims("read-write")));
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

    #[test]
    fn ttl_zero_entry_is_immediately_expired() {
        // With TTL=0, expires_at == now at put time, so the `<=` check in
        // get treats it as already expired. This documents that boundary.
        let cache = TokenCache::new(Duration::from_secs(0));
        cache.put("a", TokenState::Valid(dummy_claims("read-write")));
        assert_eq!(cache.get("a"), None, "TTL=0 entry should be expired on get");
    }

    #[test]
    fn overwrite_replaces_state_and_resets_ttl() {
        let cache = TokenCache::new(Duration::from_secs(10));
        cache.put("a", TokenState::Invalid);
        assert_eq!(cache.get("a"), Some(TokenState::Invalid));
        cache.put("a", TokenState::Valid(dummy_claims("read-write")));
        assert!(matches!(cache.get("a"), Some(TokenState::Valid(_))));
    }

    #[test]
    fn distinct_tokens_have_distinct_entries() {
        let cache = TokenCache::new(Duration::from_secs(10));
        cache.put("token-one", TokenState::Valid(dummy_claims("read-write")));
        cache.put("token-two", TokenState::Invalid);
        assert!(matches!(cache.get("token-one"), Some(TokenState::Valid(_))));
        assert_eq!(cache.get("token-two"), Some(TokenState::Invalid));
        // Invalidating must clear both.
        cache.invalidate();
        assert_eq!(cache.get("token-one"), None);
        assert_eq!(cache.get("token-two"), None);
    }
}
