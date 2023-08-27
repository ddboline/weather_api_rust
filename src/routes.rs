use cached::Cached;
use dioxus::prelude::VirtualDom;
use futures::{future::try_join_all, TryStreamExt};
use isocountry::CountryCode;
use lazy_static::lazy_static;
use rweb::{get, post, Json, Query, Rejection, Schema};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{collections::HashMap, convert::Infallible};
use time::{
    macros::{date, time},
    Date, Duration, OffsetDateTime, PrimitiveDateTime,
};
use tokio::sync::RwLock;

use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, DateType,
    RwebResponse,
};
use weather_api_common::weather_element::{
    get_forecast_plots, get_history_plots, weather_component, weather_componentProps,
};
use weather_util_rust::{
    weather_api::WeatherLocation, weather_data::WeatherData, weather_forecast::WeatherForecast,
};

use crate::{
    api_options::ApiOptions,
    app::{
        get_weather_data, get_weather_forecast, AppState, GET_WEATHER_DATA, GET_WEATHER_FORECAST,
    },
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    model::WeatherDataDB,
    polars_analysis::get_by_name_dates,
    GeoLocationWrapper, WeatherDataDBWrapper, WeatherDataWrapper, WeatherForecastWrapper,
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
#[response(description = "Direct Geo Location")]
struct GeoDirectResponse(JsonBase<Vec<GeoLocationWrapper>, Error>);

#[get("/weather/direct")]
pub async fn geo_direct(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<GeoDirectResponse> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query
        .get_weather_location(&data.config)
        .map_err(Into::<Error>::into)?;
    let geo_locations: Vec<GeoLocationWrapper> = if let WeatherLocation::CityName(city_name) = loc {
        api.get_direct_location(&city_name)
            .await
            .map_err(Into::<Error>::into)?
            .into_iter()
            .map(Into::into)
            .collect()
    } else {
        Vec::new()
    };
    Ok(GeoDirectResponse(JsonBase::new(geo_locations)))
}

#[derive(Serialize, Deserialize, Schema)]
struct ZipOptions {
    zip: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Zip Geo Location")]
struct GeoZipResponse(JsonBase<GeoLocationWrapper, Error>);

#[get("/weather/zip")]
pub async fn geo_zip(
    #[data] data: AppState,
    query: Query<ZipOptions>,
) -> WarpResult<GeoZipResponse> {
    let query = query.into_inner();
    let api = &data.api;
    let zip_country: Vec<_> = query.zip.split(',').take(2).collect();
    let zip: u64 = zip_country
        .first()
        .expect("zip invalid")
        .parse()
        .map_err(Into::<Error>::into)?;
    let country_code: Option<CountryCode> = zip_country
        .get(1)
        .and_then(|s| CountryCode::for_alpha2(s).ok());
    let loc = api
        .get_zip_location(zip, country_code)
        .await
        .map_err(Into::<Error>::into)?;
    Ok(GeoZipResponse(JsonBase::new(loc.into())))
}

#[get("/weather/reverse")]
pub async fn geo_reverse(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<GeoDirectResponse> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query
        .get_weather_location(&data.config)
        .map_err(Into::<Error>::into)?;
    let geo_locations = if let WeatherLocation::LatLon {
        latitude,
        longitude,
    } = loc
    {
        api.get_geo_location(latitude, longitude)
            .await
            .map_err(Into::<Error>::into)?
            .into_iter()
            .map(Into::into)
            .collect()
    } else {
        Vec::new()
    };
    Ok(GeoDirectResponse(JsonBase::new(geo_locations)))
}

#[derive(Debug, Serialize, Deserialize, Schema)]
struct LocationCount {
    location: StackString,
    count: i64,
}

#[derive(RwebResponse)]
#[response(description = "Get Weather History Locations")]
struct HistoryLocationsResponse(JsonBase<Vec<LocationCount>, Error>);

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
            .map_ok(|(location, count)| LocationCount { location, count })
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
    _: LoggedUser,
) -> WarpResult<HistoryResponse> {
    let history = if let Some(pool) = &data.pool {
        let query = query.into_inner();
        let server = query
            .server
            .as_ref()
            .map_or(data.config.server.as_str(), StackString::as_str);
        let start_time: Date = query.start_time.map_or(
            (OffsetDateTime::now_utc() - Duration::days(7)).date(),
            Into::into,
        );
        WeatherDataDB::get_by_name_dates(
            pool,
            query.name.as_ref().map(StackString::as_str),
            Some(server),
            Some(start_time),
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

#[derive(Serialize, Deserialize, Schema)]
struct HistoryUpdateRequest {
    updates: Vec<WeatherDataDBWrapper>,
}

#[derive(RwebResponse)]
#[response(description = "Update Weather History", status = "CREATED")]
struct HistoryUpdateResponse(JsonBase<u64, Error>);

#[post("/weather/history")]
pub async fn history_update(
    #[data] data: AppState,
    query: Query<ApiOptions>,
    payload: Json<HistoryUpdateRequest>,
    _: LoggedUser,
) -> WarpResult<HistoryUpdateResponse> {
    let payload = payload.into_inner();
    let query = query.into_inner();
    let appid = query
        .appid
        .ok_or_else(|| Error::BadRequest("Missing appid".into()))?;
    if appid != data.config.api_key {
        return Err(Error::BadRequest("Incorrect appid".into()).into());
    }
    let inserts = if let Some(pool) = &data.pool {
        let futures = payload.updates.into_iter().map(|update| async move {
            let entry: WeatherDataDB = update.into();
            entry.insert(pool).await.map_err(Into::<Error>::into)
        });
        let results: Result<Vec<u64>, Error> = try_join_all(futures).await;
        results?.into_iter().sum()
    } else {
        0
    };
    Ok(JsonBase::new(inserts).into())
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
    let now = OffsetDateTime::now_utc();
    let first_of_month = PrimitiveDateTime::new(
        Date::from_calendar_date(now.year(), now.month(), 1)
            .unwrap_or_else(|_| date!(2023 - 01 - 01)),
        time!(00:00),
    )
    .assume_utc()
    .date();

    let query = query.into_inner();
    let start_date: Option<Date> = query.start_time.map(Into::into);
    let end_date: Option<Date> = query.end_time.map(Into::into);

    let history: Vec<WeatherData> = if start_date.is_none() || start_date < Some(first_of_month) {
        get_by_name_dates(
            &data.config.cache_dir,
            Some(&query.name),
            query.server.as_ref().map(StackString::as_str),
            start_date,
            end_date,
        )
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(Into::<WeatherData>::into)
        .collect()
    } else if let Some(pool) = &data.pool {
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

#[derive(RwebResponse)]
#[response(description = "Logged in User")]
struct UserResponse(JsonBase<LoggedUser, Error>);

#[get("/weather/user")]
pub async fn user(user: LoggedUser) -> WarpResult<UserResponse> {
    Ok(JsonBase::new(user).into())
}
