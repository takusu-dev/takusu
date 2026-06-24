use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("conflict: {message}")]
    Conflict { message: String },
    #[error("internal: {0}")]
    Internal(String),
}

pub(crate) fn storage_to_app(e: takusu_storage::StorageError) -> AppError {
    use takusu_storage::StorageError;
    match e {
        StorageError::NotFound(m) => AppError::NotFound(m),
        StorageError::BadRequest(m) => AppError::BadRequest(m),
        StorageError::Unauthorized => AppError::Unauthorized,
        StorageError::Conflict(m) => AppError::Conflict { message: m },
        StorageError::Internal(m) => AppError::Internal(m),
    }
}
