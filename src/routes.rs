use actix_web::{
    http::StatusCode,
    web::{Data, Query},
    HttpResponse,
};
use cached::Cached;
use chrono::FixedOffset;
use handlebars::Handlebars;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use weather_util_rust::{
    latitude::Latitude,
    longitude::Longitude,
    precipitation::Precipitation,
    weather_api::{WeatherApi, WeatherLocation},
};

use crate::{
    app::{AppState, CONFIG},
    errors::ServiceError as Error,
};

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
        let result = $mutex.lock().await.cache_get(&$hash).map(Clone::clone);
        if let Some(d) = result {
            d
        } else {
            let d = Arc::new($call.await?);
            $mutex.lock().await.cache_set($hash.clone(), d.clone());
            d
        }
    }};
}

pub async fn frontpage(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let opts = query.into_inner();
    let api = opts.get_weather_api(&data.api)?;
    let loc = opts.get_weather_location()?;

    let hash = format!("{:?}", loc);

    let weather_data = get_cached!(hash, data.data, api.get_weather_data(&loc));
    let weather_forecast = get_cached!(hash, data.forecast, api.get_weather_forecast(&loc));

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

pub async fn forecast_plot(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let opts = query.into_inner();
    let api = opts.get_weather_api(&data.api)?;
    let loc = opts.get_weather_location()?;

    let hash = format!("{:?}", loc);

    let weather_data = get_cached!(hash, data.data, api.get_weather_data(&loc));
    let weather_forecast = get_cached!(hash, data.forecast, api.get_weather_forecast(&loc));

    let mut buf = Vec::new();
    weather_data.get_current_conditions(&mut buf)?;
    let weather_data = String::from_utf8(buf)?;
    let lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let rows = lines.len() + 5;
    let body = format!(
        "<textarea rows={} cols={}>{}</textarea>",
        rows,
        cols,
        lines.join("\n")
    );

    let fo: FixedOffset = weather_forecast.city.timezone.into();
    let data: Vec<_> = weather_forecast
        .list
        .iter()
        .map(|entry| {
            (
                entry
                    .dt
                    .with_timezone(&fo)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
                entry.main.temp.fahrenheit(),
            )
        })
        .collect();

    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string("ts", include_str!("../templates/TIMESERIESTEMPLATE.js.hbr"))?;
    handlebars.register_template_string("ht", include_str!("../templates/PLOT_TEMPLATE.html.hbr"))?;

    let js_str = serde_json::to_string(&data).unwrap_or_else(|_| "".to_string());

    let params = hashmap! {
        "DATA" => js_str.as_str(),
        "YAXIS" => "F",
        "XAXIS" => "",
        "EXAMPLETITLE" => "Temperature Forecast",
        "NAME" => "temperature_forecast",
    };

    let body = format!(
        "{}<br>{}",
        body,
        handlebars.render("ts", &params)?
    );

    let data: Vec<_> = weather_forecast
        .list
        .iter()
        .map(|entry| {
            let rain = if let Some(rain) = &entry.rain {
                rain.three_hour.unwrap_or_default()
            } else {
                Precipitation::default()
            };
            let snow = if let Some(snow) = &entry.snow {
                snow.three_hour.unwrap_or_default()
            } else {
                Precipitation::default()
            };
            (
                entry
                    .dt
                    .with_timezone(&fo)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
                (rain + snow).inches(),
            )
        })
        .collect();

    let js_str = serde_json::to_string(&data).unwrap_or_else(|_| "".to_string());

    let params = hashmap! {
        "DATA"=> js_str.as_str(),
        "YAXIS"=> "in",
        "XAXIS"=> "",
        "EXAMPLETITLE"=> "Precipitation Forecast",
        "NAME"=> "precipitation_forecast",
    };

    let body = format!(
        "{}<br>{}",
        body,
        handlebars.render("ts", &params)?
    );

    let body = handlebars.render("ht", &hashmap! {"INSERTOTHERIMAGESHERE" => &body})?;

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
    fn get_weather_api(&self, api: &WeatherApi) -> Result<WeatherApi, Error> {
        let api = if let Some(appid) = &self.appid {
            api.clone().with_key(&appid)
        } else {
            api.clone()
        };
        Ok(api)
    }

    fn get_weather_location(&self) -> Result<WeatherLocation, Error> {
        let config = &CONFIG;
        let loc = if let Some(zipcode) = self.zip {
            if let Some(country_code) = &self.country_code {
                WeatherLocation::from_zipcode_country_code(zipcode, country_code)
            } else {
                WeatherLocation::from_zipcode(zipcode)
            }
        } else if let Some(city_name) = &self.q {
            WeatherLocation::from_city_name(city_name)
        } else if self.lat.is_some() && self.lon.is_some() {
            let lat = self.lat.unwrap();
            let lon = self.lon.unwrap();
            WeatherLocation::from_lat_lon(lat, lon)
        } else if let Some(zipcode) = config.zipcode {
            if let Some(country_code) = &config.country_code {
                WeatherLocation::from_zipcode_country_code(zipcode, country_code)
            } else {
                WeatherLocation::from_zipcode(zipcode)
            }
        } else if let Some(city_name) = &config.city_name {
            WeatherLocation::from_city_name(city_name)
        } else if config.lat.is_some() && config.lon.is_some() {
            let lat = config.lat.unwrap();
            let lon = config.lon.unwrap();
            WeatherLocation::from_lat_lon(lat, lon)
        } else {
            return Err(Error::BadRequest(
                "\n\nERROR: You must specify at least one option".into(),
            ));
        };
        Ok(loc)
    }
}

pub async fn weather(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location()?;
    let hash = format!("{:?}", loc);

    let weather_data = get_cached!(hash, data.data, api.get_weather_data(&loc));

    to_json(&(*weather_data))
}

pub async fn forecast(
    query: Query<ApiOptions>,
    data: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location()?;
    let hash = format!("{:?}", loc);
    let weather_forecast = get_cached!(hash, data.forecast, api.get_weather_forecast(&loc));

    to_json(&(*weather_forecast))
}
