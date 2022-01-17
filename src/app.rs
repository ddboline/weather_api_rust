use anyhow::Error;
use handlebars::Handlebars;
use rweb::{
    filters::BoxedFilter,
    http::header::CONTENT_TYPE,
    openapi::{self, Info},
    reply, Filter, Reply,
};
use stack_string::format_sstr;
use std::{fmt::Write, net::SocketAddr, sync::Arc};

use weather_util_rust::weather_api::WeatherApi;

use super::{
    config::Config,
    errors::error_response,
    routes::{forecast, forecast_plot, frontpage, get_templates, statistics, weather},
};

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<WeatherApi>,
    pub config: Config,
    pub hbr: Arc<Handlebars<'static>>,
}

pub async fn start_app() -> Result<(), Error> {
    let config = Config::init_config()?;

    let port = config.port;
    run_app(&config, port).await
}

fn get_api_path(app: &AppState) -> BoxedFilter<(impl Reply,)> {
    let frontpage_path = frontpage(app.clone());
    let forecast_plot_path = forecast_plot(app.clone());
    let weather_path = weather(app.clone());
    let forecast_path = forecast(app.clone());
    let statistics_path = statistics();

    frontpage_path
        .or(forecast_plot_path)
        .or(weather_path)
        .or(forecast_path)
        .or(statistics_path)
        .boxed()
}

async fn run_app(config: &Config, port: u32) -> Result<(), Error> {
    let app = AppState {
        api: Arc::new(WeatherApi::new(
            &config.api_key,
            &config.api_endpoint,
            &config.api_path,
        )),
        config: config.clone(),
        hbr: Arc::new(get_templates()?),
    };

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
    println!("GOT HERE");
    let host = &config.host;
    let addr: SocketAddr = format_sstr!("{host}:{port}").parse()?;
    rweb::serve(routes).bind(addr).await;

    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use chrono::{offset::FixedOffset, Offset};
    use chrono_tz::US::Central;
    use stack_string::format_sstr;
    use std::{convert::TryInto, fmt::Write};

    use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

    use crate::{app::run_app, config::Config, routes::StatisticsObject};

    #[tokio::test]
    async fn test_run_app() -> Result<(), Error> {
        let config = Config::init_config()?;
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
        println!("{:?}", forecast);
        assert_eq!(forecast.list.len(), 40);
        let city_offset: FixedOffset = forecast.city.timezone.into();

        let local = weather.dt.with_timezone(&Central);
        let expected_offset: FixedOffset = local.offset().fix();
        assert_eq!(city_offset, expected_offset);

        let url = format_sstr!("http://localhost:{test_port}/weather/index.html?zip=55427");
        let text = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .text()
            .await?;
        println!("{}", text);
        assert!(text.len() > 0);

        let url = format_sstr!("http://localhost:{test_port}/weather/plot.html?zip=55427");
        let text = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .text()
            .await?;
        println!("{}", text);
        assert!(text.len() > 0);

        let url = format_sstr!("http://localhost:{test_port}/weather/statistics");
        let stats: StatisticsObject = reqwest::get(url.as_str())
            .await?
            .error_for_status()?
            .json()
            .await?;
        println!("{}", serde_json::to_string(&stats)?);
        assert!(stats.data_cache_hits == 2);
        assert!(stats.data_cache_misses == 1);
        assert!(stats.forecast_cache_hits == 2);
        assert!(stats.forecast_cache_misses == 1);

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
