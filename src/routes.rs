use cached::Cached;
use dioxus::prelude::VirtualDom;
use futures::{future::try_join_all, TryStreamExt};
use isocountry::CountryCode;
use once_cell::sync::Lazy;
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
    ForecastComponent, ForecastComponentProps, WeatherComponent, WeatherComponentProps,
};
use weather_util_rust::{
    weather_api::WeatherLocation, weather_data::WeatherData, weather_forecast::WeatherForecast,
};

use crate::{
    api_options::ApiOptions,
    app::{
        get_weather_data, get_weather_forecast, AppState, GET_WEATHER_DATA, GET_WEATHER_FORECAST,
    },
    config::Config,
    errors::ServiceError as Error,
    get_forecast_plots, get_forecast_precip_plot, get_forecast_temp_plot, get_history_plots,
    get_history_precip_plot, get_history_temperature_plot,
    logged_user::LoggedUser,
    model::WeatherDataDB,
    pgpool::PgPool,
    polars_analysis::get_by_name_dates,
    GeoLocationWrapper, PlotDataWrapper, PlotPointWrapper, WeatherDataDBWrapper,
    WeatherDataWrapper, WeatherForecastWrapper,
};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

static WEATHER_STRING_LENGTH: Lazy<StringLengthMap> = Lazy::new(StringLengthMap::new);

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

    let weather = get_weather_data(&data.pool, &data.config, &api, &loc).await?;
    let forecast = get_weather_forecast(&api, &loc).await?;

    let body = {
        let mut app = VirtualDom::new_with_props(
            WeatherComponent,
            WeatherComponentProps { weather, forecast },
        );
        app.rebuild_in_place();
        let mut renderer = dioxus_ssr::Renderer::default();
        let mut buffer = String::new();
        renderer
            .render_to(&mut buffer, &app)
            .map_err(Into::<Error>::into)?;
        buffer
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
    let weather = get_weather_data(&data.pool, &data.config, &api, &loc).await?;

    let plots = get_forecast_plots(&query, &weather).map_err(Into::<Error>::into)?;

    let body = {
        let mut app = VirtualDom::new_with_props(
            ForecastComponent,
            ForecastComponentProps { weather, plots },
        );
        app.rebuild_in_place();
        let mut renderer = dioxus_ssr::Renderer::default();
        let mut buffer = String::new();
        renderer
            .render_to(&mut buffer, &app)
            .map_err(Into::<Error>::into)?;
        buffer
    };

    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/plot.html", body.len())
        .await;
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema, Clone)]
#[schema(component = "Statistics")]
pub struct StatisticsObject {
    #[schema(description = "Weather Data Cache Hits")]
    pub data_cache_hits: u64,
    #[schema(description = "Weather Data Cache Misses")]
    pub data_cache_misses: u64,
    #[schema(description = "Forecast Cache Hits")]
    pub forecast_cache_hits: u64,
    #[schema(description = "Forecast Cache Misses")]
    pub forecast_cache_misses: u64,
    #[schema(description = "Weather String Length Map")]
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
    let weather_data = get_weather_data(&data.pool, &data.config, &api, &loc).await?;
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
#[schema(component = "LocationCount")]
struct LocationCount {
    #[schema(description = "Location String")]
    location: StackString,
    #[schema(description = "Count")]
    count: i64,
}

#[derive(Debug, Serialize, Deserialize, Schema)]
#[schema(component = "Pagination")]
struct Pagination {
    #[schema(description = "Number of Entries Returned")]
    limit: usize,
    #[schema(description = "Number of Entries to Skip")]
    offset: usize,
    #[schema(description = "Total Number of Entries")]
    total: usize,
}

#[derive(Debug, Serialize, Deserialize, Schema)]
#[schema(component = "PaginatedLocationCount")]
struct PaginatedLocationCount {
    pagination: Pagination,
    data: Vec<LocationCount>,
}

#[derive(RwebResponse)]
#[response(description = "Get Weather History Locations")]
struct HistoryLocationsResponse(JsonBase<PaginatedLocationCount, Error>);

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
    let query = query.into_inner();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(10);

    let (total, data) = {
        let pool = &data.pool;
        let total = WeatherDataDB::get_total_locations(pool)
            .await
            .map_err(Into::<Error>::into)?;
        let history: Vec<_> = WeatherDataDB::get_locations(pool, Some(offset), Some(limit))
            .await
            .map_err(Into::<Error>::into)?
            .map_ok(|(location, count)| LocationCount { location, count })
            .try_collect()
            .await
            .map_err(Into::<Error>::into)?;
        (total, history)
    };
    let pagination = Pagination {
        limit,
        offset,
        total,
    };
    let result = PaginatedLocationCount { pagination, data };
    Ok(JsonBase::new(result).into())
}

#[derive(Deserialize, Schema)]
struct HistoryRequest {
    name: Option<StackString>,
    server: Option<StackString>,
    start_time: Option<DateType>,
    end_time: Option<DateType>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Schema)]
#[schema(component = "PaginatedWeatherDataDB")]
struct PaginatedWeatherDataDB {
    pagination: Pagination,
    data: Vec<WeatherDataDBWrapper>,
}

#[derive(RwebResponse)]
#[response(description = "Get Weather History")]
struct HistoryResponse(JsonBase<PaginatedWeatherDataDB, Error>);

#[get("/weather/history")]
pub async fn history(
    #[data] data: AppState,
    query: Query<HistoryRequest>,
    _: LoggedUser,
) -> WarpResult<HistoryResponse> {
    let query = query.into_inner();
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(0);
    let (total, data) = {
        let pool = &data.pool;
        let server = query
            .server
            .as_ref()
            .map_or(data.config.server.as_str(), StackString::as_str);
        let name = query.name.as_ref().map(StackString::as_str);
        let start_time: Date = query.start_time.map_or(
            (OffsetDateTime::now_utc() - Duration::days(7)).date(),
            Into::into,
        );
        let end_time = query.end_time.map(Into::into);
        let total = WeatherDataDB::get_total_by_name_dates(
            pool,
            name,
            Some(server),
            Some(start_time),
            end_time,
        )
        .await
        .map_err(Into::<Error>::into)?;
        let data: Vec<_> = WeatherDataDB::get_by_name_dates(
            pool,
            query.name.as_ref().map(StackString::as_str),
            Some(server),
            Some(start_time),
            end_time,
            Some(offset),
            Some(limit),
        )
        .await
        .map_err(Into::<Error>::into)?
        .map_ok(Into::<WeatherDataDBWrapper>::into)
        .try_collect()
        .await
        .map_err(Into::<Error>::into)?;
        (total, data)
    };
    let pagination = Pagination {
        limit,
        offset,
        total,
    };
    let result = PaginatedWeatherDataDB { pagination, data };
    Ok(JsonBase::new(result).into())
}

#[derive(Serialize, Deserialize, Schema)]
#[schema(component = "HistoryUpdateRequest")]
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
    let inserts = {
        let pool = &data.pool;
        let futures = payload.updates.into_iter().map(|update| async move {
            let entry: WeatherDataDB = update.into();
            entry.insert(pool).await.map_err(Into::<Error>::into)
        });
        let results: Result<Vec<u64>, Error> = try_join_all(futures).await;
        results?.into_iter().sum()
    };
    Ok(JsonBase::new(inserts).into())
}

#[derive(Deserialize, Schema, Serialize)]
#[schema(component = "HistoryPlotRequest")]
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
            Some(0),
            Some(1),
        )
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(Into::<WeatherData>::into)
        .collect()
    } else {
        let pool = &data.pool;
        WeatherDataDB::get_by_name_dates(
            pool,
            Some(&query.name),
            query.server.as_ref().map(StackString::as_str),
            query.start_time.map(Into::into),
            query.end_time.map(Into::into),
            Some(0),
            Some(1),
        )
        .await
        .map_err(Into::<Error>::into)?
        .map_ok(Into::<WeatherData>::into)
        .try_collect()
        .await
        .map_err(Into::<Error>::into)?
    };

    if history.is_empty() {
        return Ok(HtmlBase::new(String::new()).into());
    }
    let weather = history.first().unwrap().clone();
    let query_string = serde_urlencoded::to_string(&query).map_err(Into::<Error>::into)?;
    let plots = get_history_plots(&query_string, &weather);

    let body = {
        let mut app = VirtualDom::new_with_props(
            ForecastComponent,
            ForecastComponentProps { weather, plots },
        );
        app.rebuild_in_place();
        let mut renderer = dioxus_ssr::Renderer::default();
        let mut buffer = String::new();
        renderer
            .render_to(&mut buffer, &app)
            .map_err(Into::<Error>::into)?;
        buffer
    };

    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/history_plot.html", body.len())
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

#[derive(RwebResponse)]
#[response(description = "Forecast Plot Data")]
struct ForecastPlotsResponse(JsonBase<Vec<PlotDataWrapper>, Error>);

#[get("/weather/forecast-plots")]
pub async fn forecast_plots(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<ForecastPlotsResponse> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let weather = get_weather_data(&data.pool, &data.config, &api, &loc).await?;

    let plots = get_forecast_plots(&query, &weather)
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots).into())
}

#[derive(RwebResponse)]
#[response(description = "Plot Data")]
struct PlotDataResponse(JsonBase<Vec<PlotPointWrapper>, Error>);

#[get("/weather/forecast-plots/temperature")]
pub async fn forecast_temp_plot(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<PlotDataResponse> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let forecast = get_weather_forecast(&api, &loc).await?;
    let plots = get_forecast_temp_plot(&forecast)
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots).into())
}

#[get("/weather/forecast-plots/precipitation")]
pub async fn forecast_precip_plot(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<PlotDataResponse> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let forecast = get_weather_forecast(&api, &loc).await?;
    let plots = get_forecast_precip_plot(&forecast)
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots).into())
}

#[derive(RwebResponse)]
#[response(description = "Historical Plot Data")]
struct HistoryPlotsResponse(JsonBase<Vec<PlotDataWrapper>, Error>);

async fn get_history_data(
    query: &HistoryPlotRequest,
    config: &Config,
    pool: &PgPool,
) -> Result<Vec<WeatherData>, Error> {
    let now = OffsetDateTime::now_utc();
    let first_of_month = PrimitiveDateTime::new(
        Date::from_calendar_date(now.year(), now.month(), 1)
            .unwrap_or_else(|_| date!(2023 - 01 - 01)),
        time!(00:00),
    )
    .assume_utc()
    .date();

    let start_date: Option<Date> = query.start_time.map(Into::into);
    let end_date: Option<Date> = query.end_time.map(Into::into);

    let history: Vec<WeatherData> = if start_date.is_none() || start_date < Some(first_of_month) {
        get_by_name_dates(
            &config.cache_dir,
            Some(&query.name),
            query.server.as_ref().map(StackString::as_str),
            start_date,
            end_date,
            None,
            None,
        )
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(Into::<WeatherData>::into)
        .collect()
    } else {
        WeatherDataDB::get_by_name_dates(
            pool,
            Some(&query.name),
            query.server.as_ref().map(StackString::as_str),
            query.start_time.map(Into::into),
            query.end_time.map(Into::into),
            None,
            None,
        )
        .await
        .map_err(Into::<Error>::into)?
        .map_ok(Into::<WeatherData>::into)
        .try_collect()
        .await
        .map_err(Into::<Error>::into)?
    };
    Ok(history)
}

#[get("/weather/history-plots")]
pub async fn history_plots(
    #[data] data: AppState,
    query: Query<HistoryPlotRequest>,
) -> WarpResult<HistoryPlotsResponse> {
    let query = query.into_inner();
    let query_string = serde_urlencoded::to_string(&query).map_err(Into::<Error>::into)?;
    let history = get_history_data(&query, &data.config, &data.pool).await?;

    let plots = if let Some(weather) = history.first() {
        get_history_plots(&query_string, weather)
            .into_iter()
            .map(Into::into)
            .collect()
    } else {
        Vec::new()
    };

    Ok(JsonBase::new(plots).into())
}

#[get("/weather/history-plots/temperature")]
pub async fn history_temp_plot(
    #[data] data: AppState,
    query: Query<HistoryPlotRequest>,
) -> WarpResult<PlotDataResponse> {
    let query = query.into_inner();
    let history = get_history_data(&query, &data.config, &data.pool).await?;
    let plots = get_history_temperature_plot(&history)
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots).into())
}

#[get("/weather/history-plots/precipitation")]
pub async fn history_precip_plot(
    #[data] data: AppState,
    query: Query<HistoryPlotRequest>,
) -> WarpResult<PlotDataResponse> {
    let query = query.into_inner();
    let history = get_history_data(&query, &data.config, &data.pool).await?;
    let plots = get_history_precip_plot(&history)
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots).into())
}
