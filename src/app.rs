use actix_web::{web, App, HttpServer};
use cached::TimedCache;
use lazy_static::lazy_static;
use tokio::sync::RwLock;
use std::sync::Arc;

use weather_util_rust::weather_data::WeatherData;
use weather_util_rust::weather_forecast::WeatherForecast;

use super::config::Config;
use super::routes::{weather, forecast};

lazy_static! {
    pub static ref CONFIG: Config = Config::init_config().expect("Failed to load config");
}

type Cache<K, V> = Arc<RwLock<TimedCache<K, V>>>;

#[derive(Clone)]
pub struct AppState {
    pub data: Cache<String, WeatherData>,
    pub forecast: Cache<String, WeatherForecast>,
}

pub async fn start_app() {
    let config = &CONFIG;

    let port = config.port;

    let app = AppState {
        data: Arc::new(RwLock::new(TimedCache::with_lifespan_and_capacity(3600, 100))),
        forecast: Arc::new(RwLock::new(TimedCache::with_lifespan_and_capacity(3600, 100))),
    };

    HttpServer::new(move || {
        App::new()
            .data(app.clone())
            .service(web::resource("/weather/data").route(web::get().to(weather)))
            .service(web::resource("/weather/forecast").route(web::get().to(forecast)))
    })
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .run()
    .await
    .expect("Failed to start app");
}
