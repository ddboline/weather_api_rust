#![allow(clippy::pedantic)]
#![allow(clippy::too_many_arguments)]

pub mod weather_element;

use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
pub mod wasm_utils;

#[cfg(not(target_arch = "wasm32"))]
pub mod non_wasm_utils;

use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

#[derive(Clone, Debug)]
pub struct WeatherEntry {
    pub weather: Option<WeatherData>,
    pub forecast: Option<WeatherForecast>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocationCount {
    pub location: String,
    pub count: i64,
}
