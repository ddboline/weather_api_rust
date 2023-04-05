use anyhow::Error;
use cached::{proc_macro::cached, TimedSizedCache};
use log::info;
use rweb::{
    filters::BoxedFilter,
    http::header::CONTENT_TYPE,
    openapi::{self, Info},
    reply, Filter, Reply,
};
use stack_string::{format_sstr, StackString};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{task::spawn, time::interval};

use weather_api_common::weather_element::get_parameters;

use weather_util_rust::{
    weather_api::{WeatherApi, WeatherLocation},
    weather_data::WeatherData,
    weather_forecast::WeatherForecast,
};

use super::{
    config::Config,
    errors::{error_response, ServiceError},
    model::{WeatherDataDB, WeatherLocationCache},
    pgpool::PgPool,
    routes::{
        forecast, forecast_plot, frontpage, geo_direct, geo_reverse, geo_zip, history,
        history_plot, locations, statistics, timeseries_js, weather,
    },
};

#[cached(
    type = "TimedSizedCache<StackString, WeatherData>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format_sstr!("{:?}", loc) }"#,
    result = true
)]
pub async fn get_weather_data(
    pool: Option<&PgPool>,
    config: &Config,
    api: &WeatherApi,
    loc: &WeatherLocation,
) -> Result<WeatherData, ServiceError> {
    println!("GOT HERE {loc:?}");
    let loc = if let Some(pool) = pool {
        if let Some(l) = WeatherLocationCache::from_weather_location_cache(pool, loc).await? {
            println!("cached {l:?}");
            l.get_lat_lon_location()?
        } else {
            let l = WeatherLocationCache::from_weather_location(api, loc).await?;
            println!("create_cache {l:?}");
            l.insert(pool).await?;
            l.get_lat_lon_location()?
        }
    } else {
        println!("no pool {loc:?}");
        loc.to_lat_lon(api).await?
    };
    let weather_data = api.get_weather_data(&loc).await?;
    if let Some(pool) = pool {
        let mut weather_data_db: WeatherDataDB = weather_data.clone().into();
        weather_data_db.set_server(&config.server);
        info!("writing {loc} to db");
        weather_data_db.insert(pool).await?;
    } else {
        info!("using cache for {loc}");
    }
    Ok(weather_data)
}

#[cached(
    type = "TimedSizedCache<StackString, WeatherForecast>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format_sstr!("{:?}", loc) }"#,
    result = true
)]
pub async fn get_weather_forecast(
    api: &WeatherApi,
    loc: &WeatherLocation,
) -> Result<WeatherForecast, ServiceError> {
    api.get_weather_forecast(loc).await.map_err(Into::into)
}

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<WeatherApi>,
    pub config: Config,
    pub pool: Option<PgPool>,
}

/// # Errors
/// Returns error if Config init fails, or if `run_app` fails
pub async fn start_app() -> Result<(), Error> {
    let config = Config::init_config(None)?;

    let port = config.port;
    run_app(&config, port).await
}

fn get_api_path(app: &AppState) -> BoxedFilter<(impl Reply,)> {
    let frontpage_path = frontpage(app.clone()).boxed();
    let forecast_plot_path = forecast_plot(app.clone()).boxed();
    let timeseries_js_path = timeseries_js().boxed();
    let weather_path = weather(app.clone()).boxed();
    let forecast_path = forecast(app.clone()).boxed();
    let statistics_path = statistics().boxed();
    let locations_path = locations(app.clone()).boxed();
    let history_path = history(app.clone()).boxed();
    let history_plot_path = history_plot(app.clone()).boxed();
    let geo_direct_path = geo_direct(app.clone()).boxed();
    let geo_zip_path = geo_zip(app.clone()).boxed();
    let geo_reverse_path = geo_reverse(app.clone()).boxed();

    frontpage_path
        .or(forecast_plot_path)
        .or(weather_path)
        .or(forecast_path)
        .or(statistics_path)
        .or(timeseries_js_path)
        .or(locations_path)
        .or(history_path)
        .or(history_plot_path)
        .or(geo_direct_path)
        .or(geo_zip_path)
        .or(geo_reverse_path)
        .boxed()
}

async fn run_app(config: &Config, port: u32) -> Result<(), Error> {
    let pool = config.database_url.as_ref().map(|db| PgPool::new(db));
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
    let mut task = None;

    if let Some(locations_to_record) = app.config.locations_to_record.as_ref() {
        async fn update_db(app: AppState, locations: Vec<WeatherLocation>) {
            let mut i = interval(Duration::from_secs(300));
            loop {
                for loc in &locations {
                    info!("check {loc}");
                    get_weather_data(app.pool.as_ref(), &app.config, &app.api, loc)
                        .await
                        .map_or((), |_| ());
                }
                i.tick().await;
            }
        }
        let locations: Vec<_> = locations_to_record.split(';').map(get_parameters).collect();

        let app = app.clone();
        task.replace(spawn(update_db(app, locations)));
    }

    let (spec, api_path) = openapi::spec()
        .info(Info {
            title: "Weather App".into(),
            description: "Web App to disply weather from openweatherapi".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            ..Info::default()
        })
        .build(|| get_api_path(&app));
    let spec = Arc::new(spec);
    let spec_json_path = rweb::path!("weather" / "openapi" / "json")
        .and(rweb::path::end())
        .map({
            let spec = spec.clone();
            move || reply::json(spec.as_ref())
        });
    let spec_yaml = serde_yaml::to_string(spec.as_ref())?;
    let spec_yaml_path = rweb::path!("weather" / "openapi" / "yaml")
        .and(rweb::path::end())
        .map(move || {
            let reply = reply::html(spec_yaml.clone());
            reply::with_header(reply, CONTENT_TYPE, "text/yaml")
        });

    let cors = rweb::cors()
        .allow_methods(vec!["GET"])
        .allow_header("content-type")
        .allow_any_origin()
        .build();

    let routes = api_path
        .or(spec_json_path)
        .or(spec_yaml_path)
        .recover(error_response)
        .with(cors);
    let host = &config.host;
    let addr: SocketAddr = format_sstr!("{host}:{port}").parse()?;
    rweb::serve(routes).bind(addr).await;

    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use log::info;
    use stack_string::format_sstr;
    use std::convert::TryInto;
    use time::UtcOffset;
    use time_tz::{timezones::db::us::CENTRAL, Offset, TimeZone};

    use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

    use crate::{app::run_app, config::Config, routes::StatisticsObject};

    #[tokio::test]
    async fn test_run_app() -> Result<(), Error> {
        let config = Config::init_config(None)?;
        let test_port = 12345;
        tokio::task::spawn(async move {
            env_logger::init();
            run_app(&config, test_port).await.unwrap()
        });
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let url = format_sstr!("http://localhost:{test_port}/weather/weather?zip=55427");
        let weather: WeatherData = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(weather.name.as_str(), "Minneapolis");

        let url = format_sstr!("http://localhost:{test_port}/weather/forecast?zip=55427");
        let forecast: WeatherForecast = reqwest::get(url.as_str())
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

        let url = format_sstr!("http://localhost:{test_port}/weather/index.html?zip=55427");
        let text = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .text()
            .await?;
        info!("{}", text);
        assert!(text.len() > 0);

        let url = format_sstr!("http://localhost:{test_port}/weather/plot.html?zip=55427");
        let text = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .text()
            .await?;
        info!("{}", text);
        assert!(text.len() > 0);

        let url = format_sstr!("http://localhost:{test_port}/weather/statistics");
        let stats: StatisticsObject = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .json()
            .await?;
        info!("{}", serde_json::to_string(&stats)?);
        assert!(stats.data_cache_hits >= 2);
        assert!(stats.data_cache_misses >= 1);
        assert!(stats.forecast_cache_hits >= 2);
        assert!(stats.forecast_cache_misses >= 1);

        let url = format_sstr!("http://localhost:{test_port}/weather/weather?q=Minneapolis");
        let weather: WeatherData = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(weather.name.as_str(), "Minneapolis");

        let url = format_sstr!("http://localhost:{test_port}/weather/weather?lat=0&lon=0");
        let weather: WeatherData = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(weather.coord.lat, 0.0.try_into()?);
        assert_eq!(weather.coord.lon, 0.0.try_into()?);

        Ok(())
    }
}
