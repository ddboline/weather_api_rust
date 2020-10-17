use actix_web::{error::ResponseError, HttpResponse};
use anyhow::Error as AnyhowError;
use handlebars::{RenderError, TemplateError};
use std::{fmt::Debug, string::FromUtf8Error};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Internal Server Error")]
    InternalServerError,
    #[error("BadRequest: {}", _0)]
    BadRequest(String),
    #[error("Anyhow error {0}")]
    AnyhowError(#[from] AnyhowError),
    #[error("io Error {0}")]
    IoError(#[from] std::io::Error),
    #[error("invalid utf8")]
    Utf8Error(#[from] FromUtf8Error),
    #[error("render error")]
    RenderError(#[from] RenderError),
    #[error("template error")]
    TemplateError(#[from] TemplateError),
}

// impl ResponseError trait allows to convert our errors into http responses
// with appropriate data
impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            Self::BadRequest(ref message) => HttpResponse::BadRequest().json(message),
            _ => HttpResponse::InternalServerError().json(format!(
                "Internal Server Error, Please try later {:?}",
                self
            )),
        }
    }
}

#[cfg(test)]
mod test {
    use actix_web::error::ResponseError;
    use anyhow::Error;

    use crate::errors::ServiceError;

    #[test]
    fn test_service_error() -> Result<(), Error> {
        let err = ServiceError::BadRequest("TEST ERROR".into());
        let resp = err.error_response();
        assert_eq!(resp.status().as_u16(), 400);

        let err = ServiceError::InternalServerError;
        let resp = err.error_response();
        assert_eq!(resp.status().as_u16(), 500);
        Ok(())
    }
}
