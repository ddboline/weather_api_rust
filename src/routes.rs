use cached::{proc_macro::cached, Cached, TimedSizedCache};
use chrono::FixedOffset;
use handlebars::Handlebars;
use lazy_static::lazy_static;
use maplit::hashmap;
use rweb::{get, Query, Rejection, Schema};
use serde::{Deserialize, Serialize};
use stack_string::{format_sstr, StackString};
use std::{collections::HashMap, fmt::Write};
use tokio::sync::RwLock;

use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, RwebResponse,
};
use weather_util_rust::{
    precipitation::Precipitation,
    weather_api::{WeatherApi, WeatherLocation},
    weather_data::WeatherData,
    weather_forecast::WeatherForecast,
};

use crate::{
    api_options::ApiOptions, app::AppState, errors::ServiceError as Error, WeatherDataWrapper,
    WeatherForecastWrapper,
};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

lazy_static! {
    static ref WEATHER_STRING_LENGTH: StringLengthMap = StringLengthMap::new();
}

struct StringLengthMap(RwLock<HashMap<StackString, usize>>);

impl StringLengthMap {
    fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }

    async fn insert_lenth(&self, key: &str, length: usize) {
        let current_max = self.0.read().await.get(key).map_or(0, |x| *x);
        if length > current_max {
            self.0.write().await.insert(key.into(), length);
        }
    }

    async fn get_map(&self) -> HashMap<String, usize> {
        self.0
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.into(), *v))
            .collect()
    }
}

pub fn get_templates() -> Result<Handlebars<'static>, Error> {
    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string("ts", include_str!("../templates/TIMESERIESTEMPLATE.js.hbr"))?;
    handlebars
        .register_template_string("ht", include_str!("../templates/PLOT_TEMPLATE.html.hbr"))?;
    Ok(handlebars)
}

#[cached(
    type = "TimedSizedCache<StackString, WeatherData>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format_sstr!("{:?}", loc) }"#,
    result = true
)]
async fn get_weather_data(api: &WeatherApi, loc: &WeatherLocation) -> Result<WeatherData, Error> {
    api.get_weather_data(loc).await.map_err(Into::into)
}

#[cached(
    type = "TimedSizedCache<StackString, WeatherForecast>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format_sstr!("{:?}", loc) }"#,
    result = true
)]
async fn get_weather_forecast(
    api: &WeatherApi,
    loc: &WeatherLocation,
) -> Result<WeatherForecast, Error> {
    api.get_weather_forecast(loc).await.map_err(Into::into)
}

#[derive(RwebResponse)]
#[response(description = "Display Current Weather and Forecast", content = "html")]
struct IndexResponse(HtmlBase<StackString, Error>);

#[get("/weather/index.html")]
pub async fn frontpage(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<IndexResponse> {
    let body = frontpage_body(data, query.into_inner()).await?;
    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/index.html", body.len())
        .await;
    Ok(HtmlBase::new(body).into())
}

async fn frontpage_body(data: AppState, query: ApiOptions) -> HttpResult<StackString> {
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location(&data.config)?;

    let weather_data = get_weather_data(&api, &loc).await?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;

    let weather_data = weather_data.get_current_conditions()?;
    let weather_forecast = weather_forecast.get_forecast()?;

    let lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let rows = lines.len() + 5;
    let body = format_sstr!(
        "<textarea readonly rows={rows} cols={cols}>{l}</textarea>",
        l = lines.join("\n")
    );

    let lines: Vec<_> = weather_forecast.iter().map(|s| s.trim_end()).collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 10;
    let body = format_sstr!(
        "{body}<textarea readonly rows={rows} cols={cols}>{l}</textarea>",
        l = lines.join("\n")
    );
    Ok(body)
}

#[derive(RwebResponse)]
#[response(
    description = "Show Plot of Current Weather and Forecast",
    content = "html"
)]
struct WeatherPlotResponse(HtmlBase<String, Error>);

#[get("/weather/plot.html")]
pub async fn forecast_plot(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<WeatherPlotResponse> {
    let body = forecast_plot_body(data, query.into_inner()).await?;
    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/plot.html", body.len())
        .await;
    Ok(HtmlBase::new(body).into())
}

async fn forecast_plot_body(data: AppState, query: ApiOptions) -> HttpResult<String> {
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location(&data.config)?;

    let weather_data = get_weather_data(&api, &loc).await?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;

    let weather_data = weather_data.get_current_conditions()?;

    let lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let rows = lines.len() + 5;
    let body = format_sstr!(
        "<textarea readonly rows={rows} cols={cols}>{l}</textarea>",
        l = lines.join("\n")
    );

    let fo: FixedOffset = weather_forecast.city.timezone.into();
    let forecast_data: Vec<_> = weather_forecast
        .list
        .iter()
        .map(|entry| {
            let date_str =
                StackString::from_display(entry.dt.with_timezone(&fo).format("%Y-%m-%dT%H:%M:%S"));
            let temp = entry.main.temp.fahrenheit();
            (date_str, temp)
        })
        .collect();

    let js_str = serde_json::to_string(&forecast_data).unwrap_or_else(|_| "".to_string());

    let params = hashmap! {
        "DATA" => js_str.as_str(),
        "YAXIS" => "F",
        "XAXIS" => "",
        "EXAMPLETITLE" => "Temperature Forecast",
        "NAME" => "temperature_forecast",
    };
    let ts = data.hbr.render("ts", &params)?;
    let body = format_sstr!("{body}<br>{ts}");

    let forecast_data: Vec<_> = weather_forecast
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
            let dt_str =
                StackString::from_display(entry.dt.with_timezone(&fo).format("%Y-%m-%dT%H:%M:%S"));
            (dt_str, (rain + snow).inches())
        })
        .collect();

    let js_str = serde_json::to_string(&forecast_data).unwrap_or_else(|_| "".to_string());

    let params = hashmap! {
        "DATA"=> js_str.as_str(),
        "YAXIS"=> "in",
        "XAXIS"=> "",
        "EXAMPLETITLE"=> "Precipitation Forecast",
        "NAME"=> "precipitation_forecast",
    };
    let ts = data.hbr.render("ts", &params)?;
    let body = format_sstr!("{body}<br>{ts}");

    Ok(data
        .hbr
        .render("ht", &hashmap! {"INSERTOTHERIMAGESHERE" => &body})?)
}

#[derive(Serialize, Deserialize, Schema, Clone)]
pub struct StatisticsObject {
    pub data_cache_hits: u64,
    pub data_cache_misses: u64,
    pub forecast_cache_hits: u64,
    pub forecast_cache_misses: u64,
    pub weather_string_length_map: HashMap<String, usize>,
}

#[derive(RwebResponse)]
#[response(description = "Get Cache Statistics")]
struct StatisticsResponse(JsonBase<StatisticsObject, Error>);

#[get("/weather/statistics")]
pub async fn statistics() -> WarpResult<StatisticsResponse> {
    let data_cache = GET_WEATHER_DATA.lock().await;
    let forecast_cache = GET_WEATHER_FORECAST.lock().await;
    let weather_string_length_map = WEATHER_STRING_LENGTH.get_map().await;

    let stat = StatisticsObject {
        data_cache_hits: data_cache.cache_hits().unwrap_or(0),
        data_cache_misses: data_cache.cache_misses().unwrap_or(0),
        forecast_cache_hits: forecast_cache.cache_hits().unwrap_or(0),
        forecast_cache_misses: forecast_cache.cache_misses().unwrap_or(0),
        weather_string_length_map,
    };

    Ok(JsonBase::new(stat).into())
}

#[derive(RwebResponse)]
#[response(description = "Get WeatherData Api Json")]
struct WeatherResponse(JsonBase<WeatherDataWrapper, Error>);

#[get("/weather/weather")]
pub async fn weather(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<WeatherResponse> {
    let weather_data = weather_json(data, query.into_inner()).await?.into();
    Ok(JsonBase::new(weather_data).into())
}

async fn weather_json(data: AppState, query: ApiOptions) -> HttpResult<WeatherData> {
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location(&data.config)?;
    let weather_data = get_weather_data(&api, &loc).await?;
    Ok(weather_data)
}

#[derive(RwebResponse)]
#[response(description = "Get WeatherForecast Api Json")]
struct ForecastResponse(JsonBase<WeatherForecastWrapper, Error>);

#[get("/weather/forecast")]
pub async fn forecast(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<ForecastResponse> {
    let weather_forecast = forecast_body(data, query.into_inner()).await?.into();
    Ok(JsonBase::new(weather_forecast).into())
}

async fn forecast_body(data: AppState, query: ApiOptions) -> HttpResult<WeatherForecast> {
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location(&data.config)?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;
    Ok(weather_forecast)
}
