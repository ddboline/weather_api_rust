use anyhow::Error as AnyhowError;
use http::{Error as HTTPError, StatusCode};
use indexmap::IndexMap;
use postgres_query::Error as PgError;
use rweb::{
    openapi::{
        ComponentDescriptor, ComponentOrInlineSchema, Entity, Response, ResponseEntity, Responses,
    },
    reject::Reject,
    Rejection, Reply,
};
use serde::Serialize;
use serde_json::Error as SerdeJsonError;
use stack_string::StackString;
use std::{borrow::Cow, convert::Infallible, fmt::Debug, string::FromUtf8Error};
use thiserror::Error;
use time::error::Format as FormatError;
use weather_util_rust::Error as WeatherUtilError;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Internal Server Error")]
    InternalServerError,
    #[error("BadRequest: {}", _0)]
    BadRequest(StackString),
    #[error("Weather-util error {0}")]
    WeatherUtilError(#[from] WeatherUtilError),
    #[error("io Error {0}")]
    IoError(#[from] std::io::Error),
    #[error("invalid utf8")]
    Utf8Error(#[from] FromUtf8Error),
    #[error("HTTP error {0}")]
    HTTPError(#[from] HTTPError),
    #[error("SerdeJsonError {0}")]
    SerdeJsonError(#[from] SerdeJsonError),
    #[error("TimeFormatError {0}")]
    TimeFormatError(#[from] FormatError),
    #[error("AnyhowError {0}")]
    AnyhowError(#[from] AnyhowError),
    #[error("PgError {0}")]
    PgError(#[from] PgError),
}

impl Reject for ServiceError {}

#[derive(Serialize)]
struct ErrorMessage {
    code: u16,
    message: StackString,
}

/// impl `ResponseError` trait allows to convert our errors into http responses
/// with appropriate data
/// # Errors
/// Will never return an error
#[allow(clippy::unused_async)]
pub async fn error_response(err: Rejection) -> Result<impl Reply, Infallible> {
    let code;
    let message;

    if err.is_not_found() {
        code = StatusCode::NOT_FOUND;
        message = "NOT FOUND";
    } else if let Some(service_err) = err.find::<ServiceError>() {
        if let ServiceError::BadRequest(msg) = service_err {
            code = StatusCode::BAD_REQUEST;
            message = msg.as_str();
        } else {
            code = StatusCode::INTERNAL_SERVER_ERROR;
            message = "Internal Server Error, Please try again later";
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

impl Entity for ServiceError {
    fn type_name() -> Cow<'static, str> {
        rweb::http::Error::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        rweb::http::Error::describe(comp_d)
    }
}

impl ResponseEntity for ServiceError {
    fn describe_responses(_: &mut ComponentDescriptor) -> Responses {
        let mut map = IndexMap::new();

        let error_responses = [
            (StatusCode::NOT_FOUND, "Not Found"),
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
            (StatusCode::BAD_REQUEST, "Bad Request"),
            (StatusCode::METHOD_NOT_ALLOWED, "Method not allowed"),
        ];

        for (code, msg) in &error_responses {
            map.insert(
                Cow::Owned(code.as_str().into()),
                Response {
                    description: Cow::Borrowed(*msg),
                    ..Response::default()
                },
            );
        }

        map
    }
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
