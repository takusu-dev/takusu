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
