//! Error helpers.

use axum::response::IntoResponse;
use http::StatusCode;

/// Error helper type.
#[derive(Debug)]
pub enum CmError {
    /// [`worker::Error`]
    WorkerError(worker::Error),
    /// Generic internal server error.
    InternalServerError(String),
}
impl From<worker::Error> for CmError {
    fn from(value: worker::Error) -> Self {
        Self::WorkerError(value)
    }
}
impl IntoResponse for CmError {
    fn into_response(self) -> axum::response::Response {
        match self {
            CmError::WorkerError(worker_error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Worker error: {}", worker_error),
            )
                .into_response(),
            CmError::InternalServerError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
            }
        }
    }
}
