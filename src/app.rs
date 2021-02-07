use anyhow::Error;
use lazy_static::lazy_static;
use std::{net::SocketAddr, sync::Arc};
use warp::{Filter, Rejection};

use weather_util_rust::weather_api::WeatherApi;

use super::{
    config::Config,
    errors::error_response,
    routes::{forecast, forecast_plot, frontpage, statistics, weather},
};

lazy_static! {
    pub static ref CONFIG: Config = Config::init_config().expect("Failed to load config");
}

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<WeatherApi>,
}

pub async fn start_app() -> Result<(), Error> {
    let config = &CONFIG;

    let port = config.port;
    run_app(&config, port).await
}

async fn run_app(config: &Config, port: u32) -> Result<(), Error> {
    let app = AppState {
        api: Arc::new(WeatherApi::new(&config.api_key, &config.api_endpoint, &config.api_path)),
    };

    let data = warp::any().map(move || app.clone());
    let cors = warp::cors()
        .allow_methods(vec!["GET"])
        .allow_header("content-type")
        .allow_header("authorization")
        .allow_any_origin()
        .build();

    let frontpage_path = warp::path("index.html")
        .and(warp::get())
        .and(warp::path::end())
        .and(data.clone())
        .and(warp::query())
        .and_then(|data, query| async move {
            frontpage(data, query)
                .await
                .map_err(Into::<Rejection>::into)
        });

    let forecast_plot_path = warp::path("plot.html")
        .and(warp::get())
        .and(warp::path::end())
        .and(data.clone())
        .and(warp::query())
        .and_then(|data, query| async move {
            forecast_plot(data, query)
                .await
                .map_err(Into::<Rejection>::into)
        });

    let weather_path = warp::path("weather")
        .and(warp::get())
        .and(warp::path::end())
        .and(data.clone())
        .and(warp::query())
        .and_then(|data, query| async move {
            weather(data, query).await.map_err(Into::<Rejection>::into)
        });

    let forecast_path = warp::path("forecast")
        .and(warp::get())
        .and(warp::path::end())
        .and(data.clone())
        .and(warp::query())
        .and_then(|data, query| async move {
            forecast(data, query).await.map_err(Into::<Rejection>::into)
        });

    let statistics_path = warp::path("statistics")
        .and(warp::get())
        .and(warp::path::end())
        .and_then(|| async move { statistics().await.map_err(Into::<Rejection>::into) });

    let routes = warp::path("weather")
        .and(
            frontpage_path
                .or(forecast_plot_path)
                .or(weather_path)
                .or(forecast_path)
                .or(statistics_path),
        )
        .recover(error_response)
        .with(cors);
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    warp::serve(routes).bind(addr).await;

    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use chrono::{offset::FixedOffset, Offset};
    use chrono_tz::US::Central;
    use std::convert::TryInto;

    use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

    use crate::app::{run_app, CONFIG};

    #[tokio::test]
    async fn test_run_app() -> Result<(), Error> {
        let config = CONFIG.clone();
        let test_port = 12345;
        tokio::task::spawn(async move { run_app(&config, test_port).await.unwrap() });
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
