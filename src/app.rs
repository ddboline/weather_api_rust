use actix_web::{web, App, HttpServer};
use anyhow::Error;
use cached::TimedCache;
use lazy_static::lazy_static;
use std::sync::Arc;
use tokio::sync::Mutex;

use weather_util_rust::{
    weather_api::WeatherApi, weather_data::WeatherData, weather_forecast::WeatherForecast,
};

use super::{
    config::Config,
    routes::{forecast, forecast_plot, frontpage, statistics, weather},
};

lazy_static! {
    pub static ref CONFIG: Config = Config::init_config().expect("Failed to load config");
}

type Cache<K, V> = Arc<Mutex<TimedCache<K, V>>>;

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<WeatherApi>,
    pub data: Cache<String, Arc<WeatherData>>,
    pub forecast: Cache<String, Arc<WeatherForecast>>,
}

pub async fn start_app() -> Result<(), Error> {
    let config = &CONFIG;

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
        data: Arc::new(Mutex::new(TimedCache::with_lifespan_and_capacity(
            3600, 100,
        ))),
        forecast: Arc::new(Mutex::new(TimedCache::with_lifespan_and_capacity(
            3600, 100,
        ))),
    };

    HttpServer::new(move || {
        App::new()
            .data(app.clone())
            .service(web::resource("/weather/index.html").route(web::get().to(frontpage)))
            .service(web::resource("/weather/plot.html").route(web::get().to(forecast_plot)))
            .service(web::resource("/weather/weather").route(web::get().to(weather)))
            .service(web::resource("/weather/forecast").route(web::get().to(forecast)))
            .service(web::resource("/weather/statistics").route(web::get().to(statistics)))
    })
    .bind(&format!("127.0.0.1:{}", port))?
    .run()
    .await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use std::convert::TryInto;

    use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

    use crate::{app::run_app, config::Config};

    #[actix_rt::test]
    async fn test_run_app() -> Result<(), Error> {
        let config = Config::init_config()?;
        let test_port = 12345;
        actix_rt::spawn(async move { run_app(&config, test_port).await.unwrap() });
        actix_rt::time::delay_for(std::time::Duration::from_secs(10)).await;

        let url = format!("http://localhost:{}/weather/weather?zip=55427", test_port);
        let weather: WeatherData = reqwest::get(&url).await?.error_for_status()?.json().await?;
        assert_eq!(weather.name.as_str(), "Minneapolis");

        let url = format!("http://localhost:{}/weather/forecast?zip=55427", test_port);
        let forecast: WeatherForecast =
            reqwest::get(&url).await?.error_for_status()?.json().await?;
        println!("{:?}", forecast);
        assert_eq!(forecast.list.len(), 40);
        assert_eq!(forecast.city.timezone, (-18000).try_into()?);
        Ok(())
    }
}
