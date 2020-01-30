use actix_web::http::StatusCode;
use actix_web::web::{block, Data, Json, Query};
use actix_web::HttpResponse;
use cached::Cached;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::fs::{remove_file, File};
use std::io::{Read, Write};
use std::path::Path;

use weather_util_rust::latitude::Latitude;
use weather_util_rust::longitude::Longitude;
use weather_util_rust::weather_api::WeatherApi;
use weather_util_rust::weather_data::WeatherData;
use weather_util_rust::weather_forecast::WeatherForecast;
use weather_util_rust::weather_opts::WeatherOpts;

use crate::app::AppState;
use crate::errors::ServiceError as Error;

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

pub async fn frontpage(
    query: Query<WeatherOpts>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let opts = query.into_inner();
    let api = opts.set_opts(WeatherApi::clone(&data.api))?;

    let hash = api.weather_api_hash();
    println!("{}", hash);

    let weather_data = data.data.lock().await.cache_get(&hash).cloned();
    let weather_forecast = data.forecast.lock().await.cache_get(&hash).cloned();

    let weather_data = if let Some(d) = weather_data {d} else {
        let d = api.get_weather_data().await?;
        data.data.lock().await.cache_set(hash.clone(), d.clone());
        d
    };

    println!("got data");

    let weather_forecast = if let Some(d) = weather_forecast {d} else {
        let d = api.get_weather_forecast().await?;
        data.forecast.lock().await.cache_set(hash.clone(), d.clone());
        d
    };

    println!("got forecast");

    println!(
        "data hits {}, misses {}",
        data.data.lock().await.cache_hits().unwrap_or(0),
        data.data.lock().await.cache_misses().unwrap_or(0)
    );
    println!(
        "forecast hits {}, misses {}",
        data.forecast.lock().await.cache_hits().unwrap_or(0),
        data.forecast.lock().await.cache_misses().unwrap_or(0)
    );

    let mut buf = Vec::new();
    weather_data.get_current_conditions(&mut buf)?;
    weather_forecast.get_forecast(&mut buf)?;
    let body = String::from_utf8(buf)?;
    let lines: Vec<_> = body.split('\n').collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let rows = lines.len() + 5;
    let body = format!("<textarea rows={} cols={}>{}</textarea>", rows, cols, body);
    form_http_response(body)
}

pub async fn weather(
    query: Query<WeatherOpts>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    form_http_response("Dummy".to_string())
}

pub async fn forecast(
    query: Query<WeatherOpts>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    form_http_response("Dummy".to_string())
}
