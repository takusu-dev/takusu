use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Unauthorized,
    Conflict {
        message: String,
        warnings: Vec<String>,
    },
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "not found: {msg}"),
            AppError::BadRequest(msg) => write!(f, "bad request: {msg}"),
            AppError::Unauthorized => write!(f, "unauthorized"),
            AppError::Conflict { message, .. } => write!(f, "conflict: {message}"),
            AppError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, serde_json::json!({ "error": msg })),
            AppError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg }))
            }
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                serde_json::json!({ "error": "unauthorized" }),
            ),
            AppError::Conflict { message, warnings } => (
                StatusCode::CONFLICT,
                serde_json::json!({ "message": message, "warnings": warnings }),
            ),
            AppError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({ "error": msg }),
            ),
        };
        (status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
