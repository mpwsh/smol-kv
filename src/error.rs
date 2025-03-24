use crate::kv::KvStoreError;
use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use log::error;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize)]
pub struct ApiError {
    #[serde(rename = "error")]
    message: String,
    #[serde(skip)]
    status: StatusCode,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        self.status
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status).json(self)
    }
}

impl From<KvStoreError> for ApiError {
    fn from(err: KvStoreError) -> Self {
        ApiError::internal("Database operation failed", err)
    }
}

impl ApiError {
    pub fn internal(context: impl fmt::Display, err: impl fmt::Debug) -> Self {
        error!("{}: {:?}", context, err);
        Self {
            message: format!("{context}"),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::UNAUTHORIZED,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::BAD_REQUEST,
        }
    }

    pub fn missing_key() -> Self {
        Self {
            message: "Missing key parameter".into(),
            status: StatusCode::BAD_REQUEST,
        }
    }
}
