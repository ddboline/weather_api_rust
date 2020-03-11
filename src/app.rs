use actix_web::{web, App, HttpServer};
use cached::TimedCache;
use lazy_static::lazy_static;
use std::sync::Arc;
use tokio::sync::Mutex;

use weather_util_rust::{
    weather_api::WeatherApi, weather_data::WeatherData, weather_forecast::WeatherForecast,
};

use super::{
    config::Config,
    routes::{forecast, frontpage, statistics, weather, forecast_plot},
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

pub async fn start_app() {
    let config = &CONFIG;

    let port = config.port;

    let api_key = config.api_key.as_ref().expect("No API Key");
    let api_endpoint = config
        .api_endpoint
        .as_ref()
        .map_or("api.openweathermap.org", String::as_str);
    let api_path = config.api_path.as_ref().map_or("data/2.5/", String::as_str);

    let app = AppState {
        api: Arc::new(WeatherApi::new(api_key, api_endpoint, api_path)),
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
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .run()
    .await
    .expect("Failed to start app");
}
