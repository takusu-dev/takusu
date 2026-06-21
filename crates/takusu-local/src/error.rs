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

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        use axum::Json;
        use axum::http::StatusCode;
        let (status, body) = match &self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, serde_json::json!({ "message": m })),
            AppError::BadRequest(m) => {
                (StatusCode::BAD_REQUEST, serde_json::json!({ "message": m }))
            }
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                serde_json::json!({ "message": "unauthorized" }),
            ),
            AppError::Conflict { message } => (
                StatusCode::CONFLICT,
                serde_json::json!({ "message": message }),
            ),
            AppError::Internal(m) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({ "message": m }),
            ),
        };
        (status, Json(body)).into_response()
    }
}
