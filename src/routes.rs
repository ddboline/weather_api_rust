use actix_web::http::StatusCode;
use actix_web::web::{block, Data, Json, Query};
use actix_web::HttpResponse;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::fs::{remove_file, File};
use std::io::{Read, Write};
use std::path::Path;

use crate::errors::ServiceError as Error;
use crate::app::AppState;

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

pub async fn weather(data: Data<AppState>) -> Result<HttpResponse, Error> {
    form_http_response("Dummy".to_string())
}

pub async fn forecast(data: Data<AppState>) -> Result<HttpResponse, Error> {
    form_http_response("Dummy".to_string())
}
