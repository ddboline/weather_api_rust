use anyhow::Error as AnyhowError;
use axum::{
    extract::Json,
    http::{
        StatusCode,
        header::{CONTENT_TYPE, InvalidHeaderName},
    },
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
    WeatherUtilError(Box<WeatherUtilError>),
    #[error("io Error {0}")]
    IoError(#[from] std::io::Error),
    #[error("invalid utf8")]
    Utf8Error(Box<FromUtf8Error>),
    #[error("SerdeJsonError {0}")]
    SerdeJsonError(#[from] SerdeJsonError),
    #[error("TimeFormatError {0}")]
    TimeFormatError(#[from] FormatError),
    #[error("AnyhowError {0}")]
    AnyhowError(#[from] AnyhowError),
    #[error("PgError {0}")]
    PgError(Box<PgError>),
    #[error("ParseIntError {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("UrlEncodedError {0}")]
    UrlEncodedError(#[from] UrlEncodedError),
    #[error("FmtError {0}")]
    FmtError(#[from] FmtError),
}

impl From<WeatherUtilError> for ServiceError {
    fn from(value: WeatherUtilError) -> Self {
        Self::WeatherUtilError(value.into())
    }
}

impl From<FromUtf8Error> for ServiceError {
    fn from(value: FromUtf8Error) -> Self {
        Self::Utf8Error(value.into())
    }
}

impl From<PgError> for ServiceError {
    fn from(value: PgError) -> Self {
        Self::PgError(value.into())
    }
}

impl axum::response::IntoResponse for ServiceError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Unauthorized | Self::AuthUsersError(_) | Self::InvalidHeaderName(_) => (
                StatusCode::OK,
                [(CONTENT_TYPE, mime::TEXT_HTML.essence_str())],
                LOGIN_HTML,
            )
                .into_response(),
            Self::BadRequest(s) => (
                StatusCode::BAD_REQUEST,
                [(CONTENT_TYPE, mime::APPLICATION_JSON.essence_str())],
                ErrorMessage { message: s },
            )
                .into_response(),
            e => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(CONTENT_TYPE, mime::APPLICATION_JSON.essence_str())],
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
                        mime::TEXT_HTML.essence_str(),
                        ContentBuilder::new().schema(Some(String::schema())).build(),
                    ),
            )
            .response(
                StatusCode::BAD_REQUEST.as_str(),
                ResponseBuilder::new().description("Bad Request").content(
                    mime::APPLICATION_JSON.essence_str(),
                    error_message_content.clone(),
                ),
            )
            .response(
                StatusCode::INTERNAL_SERVER_ERROR.as_str(),
                ResponseBuilder::new()
                    .description("Internal Server Error")
                    .content(
                        mime::APPLICATION_JSON.essence_str(),
                        error_message_content.clone(),
                    ),
            )
            .build()
            .into()
    }
}

#[cfg(test)]
mod test {
    use anyhow::Error as AnyhowError;
    use authorized_users::errors::AuthUsersError;
    use axum::http::header::InvalidHeaderName;
    use postgres_query::Error as PgError;
    use serde_json::Error as SerdeJsonError;
    use serde_urlencoded::ser::Error as UrlEncodedError;
    use serde_yml::Error as YamlError;
    use std::{
        fmt::Error as FmtError, net::AddrParseError, num::ParseIntError, string::FromUtf8Error,
    };
    use time::error::Format as FormatError;
    use tokio::{task::JoinError, time::error::Elapsed};
    use weather_util_rust::Error as WeatherUtilError;

    use crate::errors::ServiceError as Error;

    #[test]
    fn test_error_size() {
        println!("JoinError {}", std::mem::size_of::<JoinError>());
        println!("SerdeJsonError {}", std::mem::size_of::<SerdeJsonError>());
        println!("Elapsed {}", std::mem::size_of::<Elapsed>());
        println!("FmtError {}", std::mem::size_of::<FmtError>());
        println!("AuthUsersError {}", std::mem::size_of::<AuthUsersError>());

        println!("JoinError {}", std::mem::size_of::<JoinError>());
        println!("AddrParseError {}", std::mem::size_of::<AddrParseError>());
        println!("YamlError  {}", std::mem::size_of::<YamlError>());
        println!(
            "InvalidHeaderName  {}",
            std::mem::size_of::<InvalidHeaderName>()
        );
        println!("AuthUsersError  {}", std::mem::size_of::<AuthUsersError>());
        println!(
            "Weather-util error  {}",
            std::mem::size_of::<WeatherUtilError>()
        );
        println!("io Error  {}", std::mem::size_of::<std::io::Error>());
        println!("invalid utf8  {}", std::mem::size_of::<FromUtf8Error>());
        println!("SerdeJsonError  {}", std::mem::size_of::<SerdeJsonError>());
        println!("TimeFormatError  {}", std::mem::size_of::<FormatError>());
        println!("AnyhowError  {}", std::mem::size_of::<AnyhowError>());
        println!("PgError  {}", std::mem::size_of::<PgError>());
        println!("ParseIntError  {}", std::mem::size_of::<ParseIntError>());
        println!(
            "UrlEncodedError  {}",
            std::mem::size_of::<UrlEncodedError>()
        );
        println!("FmtError  {}", std::mem::size_of::<FmtError>());

        assert_eq!(std::mem::size_of::<Error>(), 32);
    }
}
