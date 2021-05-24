use anyhow::Error as AnyhowError;
use handlebars::{RenderError, TemplateError};
use http::{Error as HTTPError, StatusCode};
use rweb::{reject::Reject, Rejection, Reply};
use serde::Serialize;
use serde_json::Error as SerdeJsonError;
use stack_string::StackString;
use std::{convert::Infallible, fmt::Debug, string::FromUtf8Error};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Internal Server Error")]
    InternalServerError,
    #[error("BadRequest: {}", _0)]
    BadRequest(StackString),
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
    #[error("HTTP error {0}")]
    HTTPError(#[from] HTTPError),
    #[error("SerdeJsonError {0}")]
    SerdeJsonError(#[from] SerdeJsonError),
}

impl Reject for ServiceError {}

#[derive(Serialize)]
struct ErrorMessage {
    code: u16,
    message: StackString,
}

// impl ResponseError trait allows to convert our errors into http responses
// with appropriate data
pub async fn error_response(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let message;

    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = "NOT FOUND";
    } else if let Some(service_err) = err.find::<ServiceError>() {
        match service_err {
            ServiceError::BadRequest(msg) => {
                code = StatusCode::BAD_REQUEST;
                message = msg.as_str();
            }
            _ => {
                code = StatusCode::INTERNAL_SERVER_ERROR;
                message = "Internal Server Error, Please try again later";
            }
        }
    } else if err.find::<rweb::reject::MethodNotAllowed>().is_some() {
        code = StatusCode::METHOD_NOT_ALLOWED;
        message = "METHOD NOT ALLOWED";
    } else {
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = "Internal Server Error, Please try again later";
    };

    let json = rweb::reply::json(&ErrorMessage {
        code: code.as_u16(),
        message: message.into(),
    });

    Ok(rweb::reply::with_status(json, code))
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use rweb::Reply;

    use crate::errors::{error_response, ServiceError};

    #[tokio::test]
    async fn test_service_error() -> Result<(), Error> {
        let err = ServiceError::BadRequest("TEST ERROR".into()).into();
        let resp = error_response(err).await?.into_response();
        assert_eq!(resp.status().as_u16(), 400);

        let err = ServiceError::InternalServerError.into();
        let resp = error_response(err).await?.into_response();
        assert_eq!(resp.status().as_u16(), 500);
        Ok(())
    }
}
