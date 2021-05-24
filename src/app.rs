use anyhow::Error;
use rweb::Filter;
use std::{net::SocketAddr, sync::Arc};

use weather_util_rust::weather_api::WeatherApi;

use super::{
    config::Config,
    errors::error_response,
    routes::{forecast, forecast_plot, frontpage, statistics, weather},
};

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<WeatherApi>,
    pub config: Config,
}

pub async fn start_app() -> Result<(), Error> {
    let config = Config::init_config()?;

    let port = config.port;
    run_app(&config, port).await
}

async fn run_app(config: &Config, port: u32) -> Result<(), Error> {
    let app = AppState {
        api: Arc::new(WeatherApi::new(
            &config.api_key,
            &config.api_endpoint,
            &config.api_path,
        )),
        config: config.clone(),
    };

    let cors = rweb::cors()
        .allow_methods(vec!["GET"])
        .allow_header("content-type")
        .allow_any_origin()
        .build();

    let routes = rweb::path("weather")
        .and(
            frontpage(app.clone())
                .or(forecast_plot(app.clone()))
                .or(weather(app.clone()))
                .or(forecast(app.clone()))
                .or(statistics()),
        )
        .recover(error_response)
        .with(cors);

    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    rweb::serve(routes).bind(addr).await;

    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use chrono::{offset::FixedOffset, Offset};
    use chrono_tz::US::Central;
    use std::convert::TryInto;

    use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

    use crate::{app::run_app, config::Config};

    #[tokio::test]
    async fn test_run_app() -> Result<(), Error> {
        let config = Config::init_config()?;
        let test_port = 12345;
        tokio::task::spawn(async move {
            env_logger::init();
            run_app(&config, test_port).await.unwrap()
        });
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let url = format!("http://localhost:{}/weather/weather?zip=55427", test_port);
        let weather: WeatherData = reqwest::get(&url).await?.error_for_status()?.json().await?;
        assert_eq!(weather.name.as_str(), "Minneapolis");

        let url = format!("http://localhost:{}/weather/forecast?zip=55427", test_port);
        let forecast: WeatherForecast =
            reqwest::get(&url).await?.error_for_status()?.json().await?;
        println!("{:?}", forecast);
        assert_eq!(forecast.list.len(), 40);
        let city_offset: FixedOffset = forecast.city.timezone.into();

        let local = weather.dt.with_timezone(&Central);
        let expected_offset: FixedOffset = local.offset().fix();
        assert_eq!(city_offset, expected_offset);

        let url = format!(
            "http://localhost:{}/weather/index.html?zip=55427",
            test_port
        );
        let text = reqwest::get(&url).await?.error_for_status()?.text().await?;
        println!("{}", text);
        assert!(text.len() > 0);

        let url = format!("http://localhost:{}/weather/plot.html?zip=55427", test_port);
        let text = reqwest::get(&url).await?.error_for_status()?.text().await?;
        println!("{}", text);
        assert!(text.len() > 0);

        let url = format!("http://localhost:{}/weather/statistics", test_port);
        let text = reqwest::get(&url).await?.error_for_status()?.text().await?;
        assert!(text.len() > 0);
        assert!(text.contains("data hits"));
        assert!(text.contains("misses"));
        assert!(text.contains("forecast hits"));

        let url = format!(
            "http://localhost:{}/weather/weather?q=Minneapolis",
            test_port
        );
        let weather: WeatherData = reqwest::get(&url).await?.error_for_status()?.json().await?;
        assert_eq!(weather.name.as_str(), "Minneapolis");

        let url = format!("http://localhost:{}/weather/weather?lat=0&lon=0", test_port);
        let weather: WeatherData = reqwest::get(&url).await?.error_for_status()?.json().await?;
        assert_eq!(weather.coord.lat, 0.0.try_into()?);
        assert_eq!(weather.coord.lon, 0.0.try_into()?);

        Ok(())
    }
}
