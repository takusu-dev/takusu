use sha2::{Digest, Sha256};

use crate::token_cache::{TokenCache, TokenState};

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn verify_token_with_cache(
    token: &str,
    root_token: &str,
    storage: &dyn takusu_storage::Storage,
    token_cache: &TokenCache,
) -> Result<bool, takusu_storage::StorageError> {
    if token == root_token {
        return Ok(true);
    }

    match token_cache.get(token) {
        Some(TokenState::Valid) => return Ok(true),
        Some(TokenState::Invalid) => return Ok(false),
        None => {}
    }

    let valid = storage.verify_token(token).await?;

    if valid {
        token_cache.put(token, TokenState::Valid);
    } else {
        token_cache.put(token, TokenState::Invalid);
    }

    Ok(valid)
}
