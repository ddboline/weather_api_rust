use axum::extract::{Json, Query, State};
use cached::Cached;
use derive_more::{From, Into};
use dioxus::prelude::VirtualDom;
use futures::{TryStreamExt, future::try_join_all};
use isocountry::CountryCode;
use serde::{Deserialize, Serialize};
use stack_string::{StackString, format_sstr};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use time::{
    Date, OffsetDateTime, PrimitiveDateTime,
    macros::{date, time},
};
use tokio::sync::RwLock;
use utoipa::{OpenApi, PartialSchema, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_helper::{
    UtoipaResponse, html_response::HtmlResponse as HtmlBase,
    json_response::JsonResponse as JsonBase,
};

use weather_api_common::weather_element::{
    ForecastComponent, ForecastComponentProps, WeatherComponent, WeatherComponentProps,
};
use weather_util_rust::{
    weather_api::WeatherLocation, weather_data::WeatherData, weather_forecast::WeatherForecast,
};

use crate::{
    CityEntryWrapper, CoordWrapper, ForecastMainWrapper, GeoLocationWrapper, PlotDataWrapper,
    PlotPointWrapper, SysWrapper, WeatherCondWrapper, WeatherDataDBWrapper, WeatherDataWrapper,
    WeatherForecastWrapper, WeatherMainWrapper, WindWrapper,
    api_options::ApiOptions,
    app::{
        AppState, GET_WEATHER_DATA, GET_WEATHER_FORECAST, get_weather_data, get_weather_forecast,
    },
    config::Config,
    errors::ServiceError as Error,
    get_forecast_plots, get_forecast_precip_plot, get_forecast_temp_plot, get_history_plots,
    get_history_precip_plot, get_history_temperature_plot,
    logged_user::LoggedUser,
    model::WeatherDataDB,
    pgpool::PgPool,
    polars_analysis::get_by_name_dates,
};

type WarpResult<T> = Result<T, Error>;
type HttpResult<T> = Result<T, Error>;

static WEATHER_STRING_LENGTH: LazyLock<StringLengthMap> = LazyLock::new(StringLengthMap::new);

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

#[derive(UtoipaResponse)]
#[response(description = "Display Current Weather and Forecast", content = "text/html")]
#[rustfmt::skip]
struct IndexResponse(HtmlBase::<StackString>);

#[utoipa::path(get, path = "/weather/index.html", responses(IndexResponse, Error))]
async fn frontpage(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<IndexResponse> {
    let Query(query) = query;
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

#[derive(UtoipaResponse)]
#[response(description = "TimeseriesScript", content = "text/javascript")]
#[rustfmt::skip]
struct TimeseriesJsResponse(HtmlBase::<&'static str>);

#[utoipa::path(
    get,
    path = "/weather/timeseries.js",
    responses(TimeseriesJsResponse, Error)
)]
async fn timeseries_js() -> WarpResult<TimeseriesJsResponse> {
    Ok(HtmlBase::new(include_str!("../templates/timeseries.js")).into())
}

#[derive(UtoipaResponse)]
#[response(
    description = "Show Plot of Current Weather and Forecast",
    content = "text/html"
)]
#[rustfmt::skip]
struct WeatherPlotResponse(HtmlBase::<String>);

#[utoipa::path(
    get,
    path = "/weather/plot.html",
    responses(WeatherPlotResponse, Error)
)]
async fn forecast_plot(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<WeatherPlotResponse> {
    let Query(query) = query;
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

#[derive(Serialize, Deserialize, ToSchema, Clone)]
// Statistics
pub struct StatisticsObject {
    // Weather Data Cache Hits
    pub data_cache_hits: u64,
    // Weather Data Cache Misses
    pub data_cache_misses: u64,
    // Forecast Cache Hits
    pub forecast_cache_hits: u64,
    // Forecast Cache Misses
    pub forecast_cache_misses: u64,
    // Weather String Length Map
    pub weather_string_length_map: HashMap<String, usize>,
}

#[derive(UtoipaResponse)]
#[response(description = "Get Cache Statistics")]
#[rustfmt::skip]
struct StatisticsResponse(JsonBase::<StatisticsObject>);

#[utoipa::path(
    get,
    path = "/weather/statistics",
    responses(StatisticsResponse, Error)
)]
async fn statistics() -> WarpResult<StatisticsResponse> {
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

#[derive(UtoipaResponse)]
#[response(description = "Get WeatherData Api Json")]
#[rustfmt::skip]
struct WeatherResponse(JsonBase::<WeatherDataWrapper>);

#[utoipa::path(get, path = "/weather/weather", responses(WeatherResponse, Error))]
async fn weather(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<WeatherResponse> {
    let Query(query) = query;
    let weather_data = weather_json(&data, query).await?.into();
    Ok(JsonBase::new(weather_data).into())
}

async fn weather_json(data: &AppState, query: ApiOptions) -> HttpResult<WeatherData> {
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;
    let weather_data = get_weather_data(&data.pool, &data.config, &api, &loc).await?;
    Ok(weather_data)
}

#[derive(UtoipaResponse)]
#[response(description = "Get WeatherForecast Api Json")]
#[rustfmt::skip]
struct ForecastResponse(JsonBase::<WeatherForecastWrapper>);

#[utoipa::path(get, path = "/weather/forecast", responses(ForecastResponse, Error))]
async fn forecast(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<ForecastResponse> {
    let Query(query) = query;
    let weather_forecast = forecast_body(&data, query).await?.into();
    Ok(JsonBase::new(weather_forecast).into())
}

async fn forecast_body(data: &AppState, query: ApiOptions) -> HttpResult<WeatherForecast> {
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;
    Ok(weather_forecast)
}

#[derive(ToSchema, Serialize, Into, From)]
struct GeoLocationVec(Vec<GeoLocationWrapper>);

#[derive(UtoipaResponse)]
#[response(description = "Direct Geo Location")]
#[rustfmt::skip]
struct GeoDirectResponse(JsonBase::<GeoLocationVec>);

#[utoipa::path(get, path = "/weather/direct", responses(GeoDirectResponse, Error))]
async fn geo_direct(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<GeoDirectResponse> {
    let Query(query) = query;
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;
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
    Ok(GeoDirectResponse(JsonBase::new(geo_locations.into())))
}

#[derive(Serialize, Deserialize, ToSchema)]
struct ZipOptions {
    zip: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Zip Geo Location")]
#[rustfmt::skip]
struct GeoZipResponse(JsonBase::<GeoLocationWrapper>);

#[utoipa::path(get, path = "/weather/zip", responses(GeoZipResponse, Error))]
async fn geo_zip(
    data: State<Arc<AppState>>,
    query: Query<ZipOptions>,
) -> WarpResult<GeoZipResponse> {
    let Query(query) = query;
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

#[utoipa::path(get, path = "/weather/reverse", responses(GeoDirectResponse, Error))]
async fn geo_reverse(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<GeoDirectResponse> {
    let Query(query) = query;
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;
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
    }
    .into();
    Ok(GeoDirectResponse(JsonBase::new(geo_locations)))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
// LocationCount
pub struct LocationCount {
    // Location String
    pub location: StackString,
    // Count
    pub count: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
// Pagination
pub struct Pagination {
    // Number of Entries Returned
    pub limit: usize,
    // Number of Entries to Skip
    pub offset: usize,
    // Total Number of Entries
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
// PaginatedLocationCount
struct PaginatedLocationCount {
    pagination: Pagination,
    data: Vec<LocationCount>,
}

#[derive(UtoipaResponse)]
#[response(description = "Get Weather History Locations")]
#[rustfmt::skip]
struct HistoryLocationsResponse(JsonBase::<PaginatedLocationCount>);

#[derive(Deserialize, ToSchema)]
struct OffsetLocation {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = "/weather/locations",
    responses(HistoryLocationsResponse, Error)
)]
async fn locations(
    data: State<Arc<AppState>>,
    query: Query<OffsetLocation>,
) -> WarpResult<HistoryLocationsResponse> {
    let Query(query) = query;
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(10);

    let total = WeatherDataDB::get_total_locations(&data.pool)
        .await
        .map_err(Into::<Error>::into)?;

    let data: Vec<_> = WeatherDataDB::get_locations(&data.pool, Some(offset), Some(limit))
        .await
        .map_err(Into::<Error>::into)?
        .map_ok(|(location, count)| LocationCount { location, count })
        .try_collect()
        .await
        .map_err(Into::<Error>::into)?;

    let pagination = Pagination {
        limit,
        offset,
        total,
    };
    Ok(JsonBase::new(PaginatedLocationCount { pagination, data }).into())
}

#[derive(Deserialize, ToSchema)]
struct HistoryRequest {
    name: Option<StackString>,
    server: Option<StackString>,
    start_time: Option<Date>,
    end_time: Option<Date>,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
// PaginatedWeatherDataDB
struct PaginatedWeatherDataDB {
    pagination: Pagination,
    data: Vec<WeatherDataDBWrapper>,
}

#[derive(UtoipaResponse)]
#[response(description = "Get Weather History")]
#[rustfmt::skip]
struct HistoryResponse(JsonBase::<PaginatedWeatherDataDB>);

#[utoipa::path(get, path = "/weather/history", responses(HistoryResponse, Error))]
async fn history(
    data: State<Arc<AppState>>,
    query: Query<HistoryRequest>,
    _: LoggedUser,
) -> WarpResult<HistoryResponse> {
    let Query(query) = query;
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(10);

    let server = query.server.as_ref().map(StackString::as_str);
    let name = query.name.as_ref().map(StackString::as_str);
    let start_time: Option<Date> = query.start_time;
    let end_time = query.end_time;
    let total =
        WeatherDataDB::get_total_by_name_dates(&data.pool, name, server, start_time, end_time)
            .await
            .map_err(Into::<Error>::into)?;

    let data: Vec<_> = WeatherDataDB::get_by_name_dates(
        &data.pool,
        query.name.as_ref().map(StackString::as_str),
        server,
        start_time,
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

    let pagination = Pagination {
        limit,
        offset,
        total,
    };
    Ok(JsonBase::new(PaginatedWeatherDataDB { pagination, data }).into())
}

#[derive(Serialize, Deserialize, ToSchema)]
// HistoryUpdateRequest
struct HistoryUpdateRequest {
    updates: Vec<WeatherDataDBWrapper>,
}

#[derive(UtoipaResponse)]
#[response(description = "Update Weather History", status = "CREATED")]
#[rustfmt::skip]
struct HistoryUpdateResponse(HtmlBase::<StackString>);

#[utoipa::path(
    post,
    path = "/weather/history",
    responses(HistoryUpdateResponse, Error)
)]
async fn history_update(
    data: State<Arc<AppState>>,
    _: LoggedUser,
    payload: Json<HistoryUpdateRequest>,
) -> WarpResult<HistoryUpdateResponse> {
    let Json(payload) = payload;
    let inserts: u64 = {
        let pool = &data.pool;
        let futures = payload.updates.into_iter().map(|update| async move {
            let entry: WeatherDataDB = update.into();
            entry.insert(pool).await.map_err(Into::<Error>::into)
        });
        let results: Result<Vec<u64>, Error> = try_join_all(futures).await;
        results?.into_iter().sum()
    };
    Ok(HtmlBase::new(format_sstr!("{inserts}")).into())
}

#[derive(Deserialize, ToSchema, Serialize)]
// HistoryPlotRequest")]
struct HistoryPlotRequest {
    name: StackString,
    server: Option<StackString>,
    start_time: Option<Date>,
    end_time: Option<Date>,
}

#[derive(UtoipaResponse)]
#[response(description = "Show Plot of Historical Weather", content = "text/html")]
#[rustfmt::skip]
struct HistoryPlotResponse(HtmlBase::<String>);

#[utoipa::path(
    get,
    path = "/weather/history_plot.html",
    responses(HistoryPlotResponse, Error)
)]
async fn history_plot(
    data: State<Arc<AppState>>,
    query: Query<HistoryPlotRequest>,
) -> WarpResult<HistoryPlotResponse> {
    let Query(query) = query;
    let history = get_history_data(&query, &data.config, &data.pool).await?;

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

#[derive(UtoipaResponse)]
#[response(description = "Logged in User")]
#[rustfmt::skip]
struct UserResponse(JsonBase::<LoggedUser>);

#[utoipa::path(get, path = "/weather/user", responses(UserResponse, Error))]
async fn user(user: LoggedUser) -> WarpResult<UserResponse> {
    Ok(JsonBase::new(user).into())
}

#[derive(ToSchema, Serialize, Into, From)]
struct PlotDataVec(Vec<PlotDataWrapper>);

#[derive(UtoipaResponse)]
#[response(description = "Forecast Plot Data")]
#[rustfmt::skip]
struct ForecastPlotsResponse(JsonBase::<PlotDataVec>);

#[utoipa::path(
    get,
    path = "/weather/forecast-plots",
    responses(ForecastPlotsResponse, Error)
)]
async fn forecast_plots(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<ForecastPlotsResponse> {
    let Query(query) = query;
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let weather = get_weather_data(&data.pool, &data.config, &api, &loc).await?;

    let plots: Vec<_> = get_forecast_plots(&query, &weather)
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots.into()).into())
}

#[derive(ToSchema, Serialize, Into, From)]
struct PlotDataInner(Vec<PlotPointWrapper>);

#[derive(UtoipaResponse)]
#[response(description = "Plot Data")]
#[rustfmt::skip]
struct PlotDataResponse(JsonBase::<PlotDataInner>);

#[utoipa::path(
    get,
    path = "/weather/forecast-plots/temperature",
    responses(PlotDataResponse, Error)
)]
async fn forecast_temp_plot(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<PlotDataResponse> {
    let Query(query) = query;
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let forecast = get_weather_forecast(&api, &loc).await?;
    let plots: Vec<PlotPointWrapper> = get_forecast_temp_plot(&forecast)
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots.into()).into())
}

#[utoipa::path(
    get,
    path = "/weather/forecast-plots/precipitation",
    responses(PlotDataResponse, Error)
)]
async fn forecast_precip_plot(
    data: State<Arc<AppState>>,
    query: Query<ApiOptions>,
) -> WarpResult<PlotDataResponse> {
    let Query(query) = query;
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let forecast = get_weather_forecast(&api, &loc).await?;
    let plots: Vec<_> = get_forecast_precip_plot(&forecast)
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots.into()).into())
}

#[derive(UtoipaResponse)]
#[response(description = "Historical Plot Data")]
#[rustfmt::skip]
struct HistoryPlotsResponse(JsonBase::<PlotDataVec>);

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

    let start_date: Option<Date> = query.start_time;
    let end_date: Option<Date> = query.end_time;

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
            query.start_time,
            query.end_time,
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

#[utoipa::path(
    get,
    path = "/weather/history-plots",
    responses(HistoryPlotsResponse, Error)
)]
async fn history_plots(
    data: State<Arc<AppState>>,
    query: Query<HistoryPlotRequest>,
) -> WarpResult<HistoryPlotsResponse> {
    let Query(query) = query;
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

    Ok(JsonBase::new(plots.into()).into())
}

#[utoipa::path(
    get,
    path = "/weather/history-plots/temperature",
    responses(PlotDataResponse, Error)
)]
async fn history_temp_plot(
    data: State<Arc<AppState>>,
    query: Query<HistoryPlotRequest>,
) -> WarpResult<PlotDataResponse> {
    let Query(query) = query;
    let history = get_history_data(&query, &data.config, &data.pool).await?;
    let plots: Vec<_> = get_history_temperature_plot(&history)
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots.into()).into())
}

#[utoipa::path(
    get,
    path = "/weather/history-plots/precipitation",
    responses(PlotDataResponse, Error)
)]
async fn history_precip_plot(
    data: State<Arc<AppState>>,
    query: Query<HistoryPlotRequest>,
) -> WarpResult<PlotDataResponse> {
    let Query(query) = query;
    let history = get_history_data(&query, &data.config, &data.pool).await?;
    let plots: Vec<_> = get_history_precip_plot(&history)
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(plots.into()).into())
}

pub fn get_api_path(app: &AppState) -> OpenApiRouter {
    let app = Arc::new(app.clone());

    OpenApiRouter::new()
        .routes(routes!(frontpage))
        .routes(routes!(forecast_plot))
        .routes(routes!(timeseries_js))
        .routes(routes!(weather))
        .routes(routes!(forecast))
        .routes(routes!(statistics))
        .routes(routes!(locations))
        .routes(routes!(history))
        .routes(routes!(history_update))
        .routes(routes!(history_plot))
        .routes(routes!(geo_direct))
        .routes(routes!(geo_zip))
        .routes(routes!(geo_reverse))
        .routes(routes!(user))
        .routes(routes!(forecast_plots))
        .routes(routes!(history_plots))
        .routes(routes!(forecast_temp_plot))
        .routes(routes!(forecast_precip_plot))
        .routes(routes!(history_temp_plot))
        .routes(routes!(history_precip_plot))
        .with_state(app)
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Weather App",
        description = "Web App to disply weather from openweatherapi",
    ),
    components(schemas(
        LoggedUser,
        CoordWrapper,
        WeatherDataDBWrapper,
        WeatherDataWrapper,
        WeatherCondWrapper,
        WeatherMainWrapper,
        WindWrapper,
        SysWrapper,
        WeatherForecastWrapper,
        GeoLocationWrapper,
        CityEntryWrapper,
        ForecastMainWrapper,
        PlotPointWrapper,
        PlotDataWrapper,
        Pagination,
        LocationCount,
    ))
)]
pub struct ApiDoc;
