#![allow(clippy::pedantic)]
#![allow(clippy::too_many_arguments)]

pub mod weather_element;

#[cfg(target_arch = "wasm32")]
pub mod wasm_utils;

#[cfg(target_arch = "wasm32")]
pub mod wasm_components;

#[cfg(not(target_arch = "wasm32"))]
pub mod non_wasm_utils;

use serde::{Deserialize, Serialize};
use std::fmt;

use weather_util_rust::{
    weather_api::WeatherLocation, weather_data::WeatherData, weather_forecast::WeatherForecast,
};

#[derive(Clone, Debug)]
pub struct WeatherEntry {
    pub weather: Option<WeatherData>,
    pub forecast: Option<WeatherForecast>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationCount {
    pub location: String,
    pub count: i64,
}

pub static DEFAULT_STR: &str = "11106";
pub static DEFAULT_URL: &str = "https://www.ddboline.net";

pub static DEFAULT_LOCATION: &str = "10001";

pub fn get_parameters(search_str: &str) -> WeatherLocation {
    let mut opts = WeatherLocation::from_city_name(search_str);
    if let Ok(zip) = search_str.parse::<u64>() {
        opts = WeatherLocation::from_zipcode(zip);
    } else if search_str.contains(',') {
        let mut iter = search_str.split(',');
        if let Some(lat) = iter.next() {
            if let Ok(lat) = lat.parse() {
                if let Some(lon) = iter.next() {
                    if let Ok(lon) = lon.parse() {
                        opts = WeatherLocation::from_lat_lon(lat, lon);
                    }
                }
            }
        }
    }
    opts
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WeatherPage {
    Index,
    Plot,
    HistoryPlot,
    Wasm,
}

impl WeatherPage {
    fn to_str(self) -> &'static str {
        match self {
            Self::Index => "weather/index.html",
            Self::Plot => "weather/plot.html",
            Self::HistoryPlot => "weather/history_plot.html",
            Self::Wasm => "wasm_weather/index.html",
        }
    }
}

impl fmt::Display for WeatherPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}
