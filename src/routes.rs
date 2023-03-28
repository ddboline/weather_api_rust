use cached::Cached;
use dioxus::prelude::VirtualDom;
use futures::TryStreamExt;
use lazy_static::lazy_static;
use rweb::{get, Query, Rejection, Schema};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{collections::HashMap, convert::Infallible};
use tokio::sync::RwLock;

use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, DateType,
    RwebResponse,
};
use weather_api_common::weather_element::{
    get_forecast_plots, get_history_plots, weather_component, weather_componentProps,
};
use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

use crate::{
    api_options::ApiOptions,
    app::{
        get_weather_data, get_weather_forecast, AppState, GET_WEATHER_DATA, GET_WEATHER_FORECAST,
    },
    errors::ServiceError as Error,
    model::WeatherDataDB,
    WeatherDataDBWrapper, WeatherDataWrapper, WeatherForecastWrapper,
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

#[derive(RwebResponse)]
#[response(description = "Display Current Weather and Forecast", content = "html")]
struct IndexResponse(HtmlBase<StackString, Error>);

#[get("/weather/index.html")]
pub async fn frontpage(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<IndexResponse> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let weather = get_weather_data(data.pool.as_ref(), &data.config, &api, &loc).await?;
    let forecast = get_weather_forecast(&api, &loc).await?;

    let body = {
        let mut app = VirtualDom::new_with_props(
            weather_component,
            weather_componentProps {
                weather,
                forecast: Some(forecast),
                plot: None,
            },
        );
        drop(app.rebuild());
        dioxus_ssr::render(&app)
    };
    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/index.html", body.len())
        .await;
    Ok(HtmlBase::new(body.into()).into())
}

#[derive(RwebResponse)]
#[response(description = "TimeseriesScript", content = "js")]
struct TimeseriesJsResponse(HtmlBase<&'static str, Infallible>);

#[get("/weather/timeseries.js")]
pub async fn timeseries_js() -> WarpResult<TimeseriesJsResponse> {
    Ok(HtmlBase::new(include_str!("../templates/timeseries.js")).into())
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
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let weather = get_weather_data(data.pool.as_ref(), &data.config, &api, &loc).await?;
    let forecast = get_weather_forecast(&api, &loc).await?;

    let plots = get_forecast_plots(&weather, &forecast).map_err(Into::<Error>::into)?;

    let body = {
        let mut app = VirtualDom::new_with_props(
            weather_component,
            weather_componentProps {
                weather,
                forecast: None,
                plot: Some(plots),
            },
        );
        drop(app.rebuild());
        dioxus_ssr::render(&app)
    };

    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/plot.html", body.len())
        .await;
    Ok(HtmlBase::new(body).into())
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
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;
    let weather_data = get_weather_data(data.pool.as_ref(), &data.config, &api, &loc).await?;
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
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;
    Ok(weather_forecast)
}

#[derive(RwebResponse)]
#[response(description = "Get Weather History Locations")]
struct HistoryLocationsResponse(JsonBase<Vec<StackString>, Error>);

#[derive(Deserialize, Schema)]
struct OffsetLocation {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[get("/weather/locations")]
pub async fn locations(
    #[data] data: AppState,
    query: Query<OffsetLocation>,
) -> WarpResult<HistoryLocationsResponse> {
    let history = if let Some(pool) = &data.pool {
        let query = query.into_inner();
        WeatherDataDB::get_locations(pool, query.offset, query.limit)
            .await
            .map_err(Into::<Error>::into)?
            .try_collect()
            .await
            .map_err(Into::<Error>::into)?
    } else {
        Vec::new()
    };
    Ok(JsonBase::new(history).into())
}

#[derive(Deserialize, Schema)]
struct HistoryRequest {
    name: Option<StackString>,
    server: Option<StackString>,
    start_time: Option<DateType>,
    end_time: Option<DateType>,
}

#[derive(RwebResponse)]
#[response(description = "Get Weather History")]
struct HistoryResponse(JsonBase<Vec<WeatherDataDBWrapper>, Error>);

#[get("/weather/history")]
pub async fn history(
    #[data] data: AppState,
    query: Query<HistoryRequest>,
) -> WarpResult<HistoryResponse> {
    let history = if let Some(pool) = &data.pool {
        let query = query.into_inner();
        WeatherDataDB::get_by_name_dates(
            pool,
            query.name.as_ref().map(StackString::as_str),
            query.server.as_ref().map(StackString::as_str),
            query.start_time.map(Into::into),
            query.end_time.map(Into::into),
        )
        .await
        .map_err(Into::<Error>::into)?
        .map_ok(Into::<WeatherDataDBWrapper>::into)
        .try_collect()
        .await
        .map_err(Into::<Error>::into)?
    } else {
        Vec::new()
    };
    Ok(JsonBase::new(history).into())
}

#[derive(Deserialize, Schema)]
struct HistoryPlotRequest {
    name: StackString,
    server: Option<StackString>,
    start_time: Option<DateType>,
    end_time: Option<DateType>,
}

#[derive(RwebResponse)]
#[response(description = "Show Plot of Historical Weather", content = "html")]
struct HistoryPlotResponse(HtmlBase<String, Error>);

#[get("/weather/history_plot.html")]
pub async fn history_plot(
    #[data] data: AppState,
    query: Query<HistoryPlotRequest>,
) -> WarpResult<HistoryPlotResponse> {
    let history = if let Some(pool) = &data.pool {
        let query = query.into_inner();
        WeatherDataDB::get_by_name_dates(
            pool,
            Some(&query.name),
            query.server.as_ref().map(StackString::as_str),
            query.start_time.map(Into::into),
            query.end_time.map(Into::into),
        )
        .await
        .map_err(Into::<Error>::into)?
        .map_ok(Into::<WeatherData>::into)
        .try_collect()
        .await
        .map_err(Into::<Error>::into)?
    } else {
        Vec::new()
    };
    if history.is_empty() {
        return Ok(HtmlBase::new(String::new()).into());
    }
    let weather = history.get(0).unwrap().clone();
    let plots = get_history_plots(&history).map_err(Into::<Error>::into)?;

    let body = {
        let mut app = VirtualDom::new_with_props(
            weather_component,
            weather_componentProps {
                weather,
                forecast: None,
                plot: Some(plots),
            },
        );
        drop(app.rebuild());
        dioxus_ssr::render(&app)
    };

    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/plot.html", body.len())
        .await;
    Ok(HtmlBase::new(body).into())
}
