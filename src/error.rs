use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use log::error;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    details: Option<String>,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
    details: Option<String>,
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
        HttpResponse::build(self.status).json(ErrorResponse {
            error: self.message.clone(),
            details: self.details.clone(),
        })
    }
}

impl ApiError {
    pub fn internal(message: impl Into<String>, details: impl fmt::Debug) -> Self {
        error!("Internal error: {:?}", details);
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            details: Some(format!("{details:?}")),
        }
    }
}
