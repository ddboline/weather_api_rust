use anyhow::Error as AnyhowError;
use axum::{
    extract::Json,
    http::{StatusCode, header::InvalidHeaderName},
};
use log::error;
use postgres_query::Error as PgError;
use serde::Serialize;
use serde_json::Error as SerdeJsonError;
use serde_urlencoded::ser::Error as UrlEncodedError;
use serde_yml::Error as YamlError;
use stack_string::{StackString, format_sstr};
use std::{
    fmt::{Debug, Error as FmtError},
    net::AddrParseError,
    num::ParseIntError,
    string::FromUtf8Error,
};
use thiserror::Error;
use time::error::Format as FormatError;
use tokio::task::JoinError;
use utoipa::{
    IntoResponses, PartialSchema, ToSchema,
    openapi::{ResponseBuilder, ResponsesBuilder, content::ContentBuilder},
};
use weather_util_rust::Error as WeatherUtilError;

use authorized_users::errors::AuthUsersError;

use crate::logged_user::LOGIN_HTML;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("JoinError {0}")]
    JoinError(#[from] JoinError),
    #[error("AddrParseError {0}")]
    AddrParseError(#[from] AddrParseError),
    #[error("YamlError {0}")]
    YamlError(#[from] YamlError),
    #[error("InvalidHeaderName {0}")]
    InvalidHeaderName(#[from] InvalidHeaderName),
    #[error("AuthUsersError {0}")]
    AuthUsersError(#[from] AuthUsersError),
    #[error("Unauthorized")]
    Unauthorized,
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
    #[error("SerdeJsonError {0}")]
    SerdeJsonError(#[from] SerdeJsonError),
    #[error("TimeFormatError {0}")]
    TimeFormatError(#[from] FormatError),
    #[error("AnyhowError {0}")]
    AnyhowError(#[from] AnyhowError),
    #[error("PgError {0}")]
    PgError(#[from] PgError),
    #[error("ParseIntError {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("UrlEncodedError {0}")]
    UrlEncodedError(#[from] UrlEncodedError),
    #[error("FmtError {0}")]
    FmtError(#[from] FmtError),
}

impl axum::response::IntoResponse for ServiceError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Unauthorized | Self::AuthUsersError(_) | Self::InvalidHeaderName(_) => {
                (StatusCode::OK, LOGIN_HTML).into_response()
            }
            Self::BadRequest(s) => {
                (StatusCode::BAD_REQUEST, ErrorMessage { message: s.into() }).into_response()
            }
            e => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorMessage {
                    message: format_sstr!("Internal Server Error: {e}"),
                },
            )
                .into_response(),
        }
    }
}

#[derive(Serialize, ToSchema)]
struct ErrorMessage {
    message: StackString,
}

impl axum::response::IntoResponse for ErrorMessage {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}

impl IntoResponses for ServiceError {
    fn responses() -> std::collections::BTreeMap<
        String,
        utoipa::openapi::RefOr<utoipa::openapi::response::Response>,
    > {
        let error_message_content = ContentBuilder::new()
            .schema(Some(ErrorMessage::schema()))
            .build();
        ResponsesBuilder::new()
            .response(
                StatusCode::UNAUTHORIZED.as_str(),
                ResponseBuilder::new()
                    .description("Not Authorized")
                    .content(
                        "text/html",
                        ContentBuilder::new().schema(Some(String::schema())).build(),
                    ),
            )
            .response(
                StatusCode::BAD_REQUEST.as_str(),
                ResponseBuilder::new()
                    .description("Bad Request")
                    .content("application/json", error_message_content.clone()),
            )
            .response(
                StatusCode::INTERNAL_SERVER_ERROR.as_str(),
                ResponseBuilder::new()
                    .description("Internal Server Error")
                    .content("application/json", error_message_content.clone()),
            )
            .build()
            .into()
    }
}

// // impl `ResponseError` trait allows to convert our errors into http
// responses // with appropriate data
// / # Errors
// / Will never return an error
// #[allow(clippy::unused_async)]
// pub async fn error_response(err: Rejection) -> Result<Box<dyn Reply>,
// Infallible> {     let code;
//     let message;

//     if err.is_not_found() {
//         code = StatusCode::NOT_FOUND;
//         message = "NOT FOUND";
//     } else if let Some(service_err) = err.find::<ServiceError>() {
//         match service_err {
//             ServiceError::BadRequest(msg) => {
//                 code = StatusCode::BAD_REQUEST;
//                 message = msg.as_str();
//             }
//             ServiceError::Unauthorized => {
//                 return Ok(Box::new(login_html()));
//             }
//             _ => {
//                 error!("{service_err:?}");
//                 code = StatusCode::INTERNAL_SERVER_ERROR;
//                 message = "Internal Server Error, Please try again later";
//             }
//         }
//     } else if err.find::<rweb::reject::MethodNotAllowed>().is_some() {
//         code = StatusCode::METHOD_NOT_ALLOWED;
//         message = "METHOD NOT ALLOWED";
//     } else {
//         code = StatusCode::INTERNAL_SERVER_ERROR;
//         message = "Internal Server Error, Please try again later";
//     };

//     let json = rweb::reply::json(&ErrorMessage {
//         code: code.as_u16(),
//         message: message.into(),
//     });
//     let reply = rweb::reply::with_status(json, code);

//     Ok(Box::new(reply))
// }

// impl Entity for ServiceError {
//     fn type_name() -> Cow<'static, str> {
//         rweb::http::Error::type_name()
//     }
//     fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema
// {         rweb::http::Error::describe(comp_d)
//     }
// }

// impl ResponseEntity for ServiceError {
//     fn describe_responses(_: &mut ComponentDescriptor) -> Responses {
//         let mut map = Responses::new();

//         let error_responses = [
//             (StatusCode::NOT_FOUND, "Not Found"),
//             (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
//             (StatusCode::BAD_REQUEST, "Bad Request"),
//             (StatusCode::METHOD_NOT_ALLOWED, "Method not allowed"),
//         ];

//         for (code, msg) in &error_responses {
//             map.insert(
//                 Cow::Owned(code.as_str().into()),
//                 Response {
//                     description: Cow::Borrowed(*msg),
//                     ..Response::default()
//                 },
//             );
//         }

//         map
//     }
// }

// #[cfg(test)]
// mod test {
//     use anyhow::Error;

//     use crate::errors::{error_response, ServiceError};

//     #[tokio::test]
//     async fn test_service_error() -> Result<(), Error> {
//         let err = ServiceError::BadRequest("TEST ERROR".into()).into();
//         let resp = error_response(err).await?.into_response();
//         assert_eq!(resp.status().as_u16(), 400);

//         let err = ServiceError::InternalServerError.into();
//         let resp = error_response(err).await?.into_response();
//         assert_eq!(resp.status().as_u16(), 500);
//         Ok(())
//     }
// }
