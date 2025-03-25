use axum::http::{Method, StatusCode};
use cached::{TimedSizedCache, proc_macro::cached};
use log::{error, info};
use stack_string::{StackString, format_sstr};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, task::spawn, time::interval};
use tower_http::cors::{Any, CorsLayer};
use utoipa_axum::router::OpenApiRouter;
use utoipa::OpenApi;

use weather_util_rust::{
    weather_api::{WeatherApi, WeatherLocation},
    weather_data::WeatherData,
    weather_forecast::WeatherForecast,
};

use super::{
    config::Config,
    errors::ServiceError as Error,
    logged_user::{fill_from_db, get_secrets},
    model::{WeatherDataDB, WeatherLocationCache},
    pgpool::PgPool,
    routes::{get_api_path, ApiDoc},
};

/// # Errors
/// Returns error if query fails
#[cached(
    ty = "TimedSizedCache<StackString, WeatherData>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format_sstr!("{:?}", loc) }"#,
    result = true
)]
pub async fn get_weather_data(
    pool: &PgPool,
    config: &Config,
    api: &WeatherApi,
    loc: &WeatherLocation,
) -> Result<WeatherData, Error> {
    let location_name = format_sstr!("{loc}");
    let loc = {
        if let Some(l) = WeatherLocationCache::from_weather_location_cache(pool, loc).await? {
            l.get_lat_lon_location()?
        } else if let Ok(l) = WeatherLocationCache::from_weather_location(api, loc).await {
            info!("create_cache {l:?}");
            l.insert(pool).await?;
            l.get_lat_lon_location()?
        } else {
            loc.clone()
        }
    };
    let weather_data = api.get_weather_data(&loc).await?;
    let mut weather_data_db: WeatherDataDB = weather_data.clone().into();
    weather_data_db.set_location_name(&location_name);
    weather_data_db.set_server(&config.server);
    info!("writing {loc} to db");
    weather_data_db.insert(pool).await?;
    Ok(weather_data)
}

/// # Errors
/// Will return error if `WeatherApi::run_api` fails
#[cached(
    ty = "TimedSizedCache<StackString, WeatherForecast>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format_sstr!("{:?}", loc) }"#,
    result = true
)]
pub async fn get_weather_forecast(
    api: &WeatherApi,
    loc: &WeatherLocation,
) -> Result<WeatherForecast, Error> {
    api.get_weather_forecast(loc).await.map_err(Into::into)
}

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<WeatherApi>,
    pub config: Config,
    pub pool: PgPool,
}

/// # Errors
/// Returns error if Config init fails, or if `run_app` fails
pub async fn start_app() -> Result<(), Error> {
    let config = Config::init_config(None)?;
    get_secrets(&config.secret_path, &config.jwt_secret_path).await?;

    let port = config.port;
    run_app(&config, port).await
}

async fn run_app(config: &Config, port: u32) -> Result<(), Error> {
    async fn update_db(pool: PgPool) {
        let mut i = interval(Duration::from_secs(60));
        loop {
            fill_from_db(&pool).await.unwrap_or(());
            i.tick().await;
        }
    }

    let pool = PgPool::new(&config.database_url)?;
    let app = AppState {
        api: Arc::new(WeatherApi::new(
            &config.api_key,
            &config.api_endpoint,
            &config.api_path,
            &config.geo_path,
        )),
        config: config.clone(),
        pool: pool.clone(),
    };
    let mut record_task = None;
    let mut db_task = None;

    db_task.replace(spawn(update_db(pool.clone())));

    let locations = app.config.locations_to_record.clone();
    if !locations.is_empty() {
        async fn update_db(app: AppState, locations: Vec<WeatherLocation>) {
            let mut i = interval(Duration::from_secs(300));
            loop {
                for loc in &locations {
                    info!("check {loc}");
                    if let Err(e) = get_weather_data(&app.pool, &app.config, &app.api, loc).await {
                        error!("Encountered error {e}");
                    }
                }
                i.tick().await;
            }
        }
        let app = app.clone();
        record_task.replace(spawn(update_db(app, locations)));
    }

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(["content-type".try_into()?, "jwt".try_into()?])
        .allow_origin(Any);

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(get_api_path(&app))
        .split_for_parts();

    let spec_json = serde_json::to_string_pretty(&api)?;
    let spec_yaml = serde_yml::to_string(&api)?;

    let router = router
        .route(
            "/weather/openapi/json",
            axum::routing::get(|| async move {
                (
                    StatusCode::OK,
                    [("content-type", "application/json")],
                    spec_json,
                )
            }),
        )
        .route(
            "/weather/openapi/yaml",
            axum::routing::get(|| async move {
                (StatusCode::OK, [("content-type", "text/yaml")], spec_yaml)
            }),
        )
        .layer(cors);

    let host = &config.host;
    let addr: SocketAddr = format_sstr!("{host}:{port}").parse()?;
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, router.into_make_service()).await?;

    if let Some(record_task) = record_task {
        record_task.await?;
    }
    if let Some(db_task) = db_task {
        db_task.await?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use log::info;
    use stack_string::format_sstr;
    use std::convert::TryInto;
    use time::UtcOffset;
    use time_tz::{Offset, TimeZone, timezones::db::us::CENTRAL};

    use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

    use crate::{app::run_app, config::Config, routes::StatisticsObject};

    #[tokio::test]
    async fn test_run_app() -> Result<(), Error> {
        let config = Config::init_config(None)?;

        let test_port = 12345;
        tokio::task::spawn({
            let config = config.clone();
            async move {
                env_logger::init();
                run_app(&config, test_port).await.unwrap()
            }
        });
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let client = reqwest::Client::new();

        let url = format_sstr!("http://localhost:{test_port}/weather/weather?zip=55416");
        let weather: WeatherData = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(weather.name.as_str(), "Saint Louis Park");

        let url = format_sstr!("http://localhost:{test_port}/weather/forecast?zip=55416");
        let forecast: WeatherForecast = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        info!("{:?}", forecast);
        assert_eq!(forecast.list.len(), 40);
        let city_offset: UtcOffset = forecast.city.timezone.into();
        let local = weather
            .dt
            .to_offset(CENTRAL.get_offset_utc(&weather.dt).to_utc());
        let expected_offset = local.offset();
        assert_eq!(city_offset, expected_offset);

        let url = format_sstr!("http://localhost:{test_port}/weather/index.html?zip=55416");
        let text = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        info!("{}", text);
        assert!(text.len() > 0);

        let url = format_sstr!("http://localhost:{test_port}/weather/plot.html?zip=55416");
        let text = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        info!("{}", text);
        assert!(text.len() > 0);

        let url = format_sstr!("http://localhost:{test_port}/weather/statistics");
        let stats: StatisticsObject = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        info!("{}", serde_json::to_string(&stats)?);
        assert!(stats.data_cache_hits >= 2);
        assert!(stats.data_cache_misses >= 1);
        assert!(stats.forecast_cache_hits >= 1);
        assert!(stats.forecast_cache_misses >= 1);

        let url = format_sstr!("http://localhost:{test_port}/weather/weather?q=Minneapolis");
        let weather: WeatherData = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(weather.name.as_str(), "Minneapolis");

        let url = format_sstr!("http://localhost:{test_port}/weather/weather?lat=0&lon=0");
        let weather: WeatherData = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(weather.coord.lat, 0.0.try_into()?);
        assert_eq!(weather.coord.lon, 0.0.try_into()?);

        let url = format_sstr!("http://localhost:{test_port}/weather/openapi/yaml");
        let spec_yaml = client.get(url.as_str()).send().await?.error_for_status()?.text().await?;

        tokio::fs::write("./scripts/openapi.yaml", &spec_yaml).await?;
        Ok(())
    }
}
