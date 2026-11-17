use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
#[derive(Debug)]
pub struct Error(pub StatusCode, pub &'static str, pub Value);
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn bad(code: &'static str) -> Self {
        Self(StatusCode::BAD_REQUEST, code, Value::Null)
    }
    pub fn missing() -> Self {
        Self(StatusCode::NOT_FOUND, "not_found", Value::Null)
    }
    pub fn conflict() -> Self {
        Self(
            StatusCode::CONFLICT,
            "name_conflict",
            json!({"choices":["overwrite","rename","cancel"]}),
        )
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let range_size = if self.0 == StatusCode::RANGE_NOT_SATISFIABLE {
            self.2["size"].as_u64()
        } else {
            None
        };
        let mut response = (
            self.0,
            Json(json!({"error":{"code":self.1,"details":self.2}})),
        )
            .into_response();
        if let Some(size) = range_size {
            response
                .headers_mut()
                .insert("content-range", format!("bytes */{size}").parse().unwrap());
        }
        response
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        if e.raw_os_error() == Some(libc::ENOSPC) {
            return Self(
                StatusCode::INSUFFICIENT_STORAGE,
                "insufficient_space",
                Value::Null,
            );
        }
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_io_error",
            Value::Null,
        )
    }
}
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "database_error",
            Value::Null,
        )
    }
}
