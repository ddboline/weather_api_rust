use actix_web::http::StatusCode;
use actix_web::web::{Data, Query};
use actix_web::HttpResponse;
use cached::Cached;
use serde::{Deserialize, Serialize};

use weather_util_rust::latitude::Latitude;
use weather_util_rust::longitude::Longitude;
use weather_util_rust::weather_api::WeatherApi;

use crate::app::AppState;
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

pub async fn frontpage(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let opts = query.into_inner();
    let api = opts.get_weather_api(WeatherApi::clone(&data.api))?;

    let hash = api.weather_api_hash();
    println!("{}", hash);

    let weather_data = data.data.lock().await.cache_get(&hash).cloned();
    let weather_forecast = data.forecast.lock().await.cache_get(&hash).cloned();

    let weather_data = if let Some(d) = weather_data {
        d
    } else {
        let d = api.get_weather_data().await?;
        data.data.lock().await.cache_set(hash.clone(), d.clone());
        d
    };

    println!("got data");

    let weather_forecast = if let Some(d) = weather_forecast {
        d
    } else {
        let d = api.get_weather_forecast().await?;
        data.forecast
            .lock()
            .await
            .cache_set(hash.clone(), d.clone());
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

impl ApiOptions {
    fn get_weather_api(&self, api: WeatherApi) -> Result<WeatherApi, Error> {
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
    let weather_data = data.data.lock().await.cache_get(&hash).cloned();

    let weather_data = if let Some(d) = weather_data {
        d
    } else {
        let d = api.get_weather_data().await?;
        data.data.lock().await.cache_set(hash, d.clone());
        d
    };

    to_json(&weather_data)
}

pub async fn forecast(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let api = query.get_weather_api(WeatherApi::clone(&data.api))?;
    let hash = api.weather_api_hash();
    let weather_forecast = data.forecast.lock().await.cache_get(&hash).cloned();

    let weather_forecast = if let Some(d) = weather_forecast {
        d
    } else {
        let d = api.get_weather_forecast().await?;
        data.forecast.lock().await.cache_set(hash, d.clone());
        d
    };

    to_json(&weather_forecast)
}
