use sha2::{Digest, Sha256};
use worker::Env;

use crate::error::WorkerError;

pub fn root_token(env: &Env) -> Result<String, WorkerError> {
    env.secret("TAKUSU_ROOT_TOKEN")
        .map(|s| s.to_string())
        .map_err(|e| WorkerError::Internal(format!("TAKUSU_ROOT_TOKEN secret not set: {e}")))
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn new_token() -> String {
    format!("tsk_{}", uuid::Uuid::now_v7())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_token_is_deterministic() {
        let a = hash_token("hello");
        let b = hash_token("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_token_output_format() {
        let hash = hash_token("test-token");
        // SHA-256 hex: 64 chars, all lowercase hex
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(hash.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn hash_token_different_inputs_differ() {
        let a = hash_token("token-a");
        let b = hash_token("token-b");
        assert_ne!(a, b);
    }

    #[test]
    fn new_token_format() {
        let token = new_token();
        assert!(token.starts_with("tsk_"), "token should start with tsk_");
        // UUID v7 is 36 chars, plus "tsk_" prefix = 40
        assert_eq!(token.len(), 40);
        // The part after "tsk_" should be a valid UUID
        let uuid_part = &token[4..];
        assert!(uuid::Uuid::try_parse(uuid_part).is_ok());
    }

    #[test]
    fn new_token_produces_unique_values() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
    }

    #[test]
    fn hash_token_empty_string() {
        let hash = hash_token("");
        assert_eq!(hash.len(), 64);
        // Known SHA-256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
