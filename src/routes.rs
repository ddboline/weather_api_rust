use actix_web::http::StatusCode;
use actix_web::web::{Data, Query};
use actix_web::HttpResponse;
use cached::Cached;
use serde::{Deserialize, Serialize};

use weather_util_rust::latitude::Latitude;
use weather_util_rust::longitude::Longitude;
use weather_util_rust::weather_api::WeatherApi;

use crate::app::{AppState, CONFIG};
use crate::errors::ServiceError as Error;

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

fn to_json<T>(js: &T) -> Result<HttpResponse, Error>
where
    T: Serialize,
{
    Ok(HttpResponse::Ok().json2(js))
}

#[derive(Serialize, Deserialize)]
pub struct ApiOptions {
    pub zip: Option<u64>,
    pub country_code: Option<String>,
    pub q: Option<String>,
    pub lat: Option<Latitude>,
    pub lon: Option<Longitude>,
    #[serde(rename = "APPID")]
    pub appid: Option<String>,
}

macro_rules! get_cached {
    ($hash:ident, $mutex:expr, $call:expr) => {{
        let result = $mutex.lock().await.cache_get(&$hash).map(|d| d.clone());
        match result {
            Some(d) => d,
            None => {
                let d = $call.await?;
                $mutex.lock().await.cache_set($hash.clone(), d.clone());
                d
            }
        }
    }};
}

pub async fn frontpage(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let opts = query.into_inner();
    let api = opts.get_weather_api(WeatherApi::clone(&data.api))?;

    let hash = api.weather_api_hash();

    let weather_data = get_cached!(hash, data.data, api.get_weather_data());
    let weather_forecast = get_cached!(hash, data.forecast, api.get_weather_forecast());

    let mut buf = Vec::new();
    weather_data.get_current_conditions(&mut buf)?;
    let weather_data = String::from_utf8(buf)?;

    let mut buf = Vec::new();
    weather_forecast.get_forecast(&mut buf)?;
    let weather_forecast = String::from_utf8(buf)?;

    let lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let rows = lines.len() + 5;
    let body = format!(
        "<textarea rows={} cols={}>{}</textarea>",
        rows,
        cols,
        lines.join("\n")
    );

    let lines: Vec<_> = weather_forecast.split('\n').map(str::trim_end).collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 10;
    let body = format!(
        "{}<textarea rows={} cols={}>{}</textarea>",
        body,
        rows,
        cols,
        lines.join("\n")
    );

    form_http_response(body)
}

pub async fn statistics(data: Data<AppState>) -> Result<HttpResponse, Error> {
    let data_cache = data.data.lock().await;
    let forecast_cache = data.forecast.lock().await;
    let body = format!(
        "data hits {}, misses {} : forecast hits {}, misses {}",
        data_cache.cache_hits().unwrap_or(0),
        data_cache.cache_misses().unwrap_or(0),
        forecast_cache.cache_hits().unwrap_or(0),
        forecast_cache.cache_misses().unwrap_or(0)
    );
    form_http_response(body)
}

impl ApiOptions {
    fn get_weather_api(&self, api: WeatherApi) -> Result<WeatherApi, Error> {
        let config = &CONFIG;

        let api = if let Some(appid) = &self.appid {
            api.with_key(&appid)
        } else {
            api
        };

        let api = if let Some(zipcode) = self.zip {
            api.with_zipcode(zipcode)
        } else if let Some(country_code) = &self.country_code {
            api.with_country_code(country_code)
        } else if let Some(city_name) = &self.q {
            api.with_city_name(city_name)
        } else if self.lat.is_some() && self.lon.is_some() {
            let lat = self.lat.unwrap();
            let lon = self.lon.unwrap();
            api.with_lat_lon(lat, lon)
        } else if let Some(zipcode) = config.zipcode {
            api.with_zipcode(zipcode)
        } else if let Some(country_code) = &config.country_code {
            api.with_country_code(country_code)
        } else if let Some(city_name) = &config.city_name {
            api.with_city_name(city_name)
        } else if config.lat.is_some() && config.lon.is_some() {
            let lat = config.lat.unwrap();
            let lon = config.lon.unwrap();
            api.with_lat_lon(lat, lon)
        } else {
            return Err(Error::BadRequest(
                "\n\nERROR: You must specify at least one option".into(),
            ));
        };
        Ok(api)
    }
}

pub async fn weather(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let api = query.get_weather_api(WeatherApi::clone(&data.api))?;
    let hash = api.weather_api_hash();

    let weather_data = get_cached!(hash, data.data, api.get_weather_data());

    to_json(&weather_data)
}

pub async fn forecast(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let api = query.get_weather_api(WeatherApi::clone(&data.api))?;
    let hash = api.weather_api_hash();
    let weather_forecast = get_cached!(hash, data.forecast, api.get_weather_forecast());

    to_json(&weather_forecast)
}
