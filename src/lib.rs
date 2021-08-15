#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::missing_errors_doc)]

pub mod api_options;
pub mod app;
pub mod config;
pub mod country_code_wrapper;
pub mod errors;
pub mod latitude_wrapper;
pub mod longitude_wrapper;
pub mod routes;
pub mod timestamp;

use chrono::{DateTime, Utc};
use rweb::Schema;
use serde::{Deserialize, Serialize};
use stack_string::StackString;

use weather_util_rust::{
    weather_data::{Coord, Rain, Snow, Sys, WeatherCond, WeatherData, WeatherMain, Wind},
    weather_forecast::{CityEntry, ForecastEntry, ForecastMain, WeatherForecast},
};

#[derive(Serialize, Deserialize, Debug, Clone, Schema)]
pub struct CoordWrapper {
    #[schema(description = "Longitude")]
    pub lon: f64,
    #[schema(description = "Latitude")]
    pub lat: f64,
}

impl From<Coord> for CoordWrapper {
    fn from(item: Coord) -> Self {
        Self {
            lon: item.lon.into(),
            lat: item.lat.into(),
        }
    }
}

// Weather Data
#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct WeatherDataWrapper {
    #[schema(description = "Coordinates")]
    pub coord: CoordWrapper,
    #[schema(description = "Weather Conditions")]
    pub weather: Vec<WeatherCondWrapper>,
    pub base: StackString,
    pub main: WeatherMainWrapper,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(description = "Visibility (m)")]
    pub visibility: Option<f64>,
    pub wind: WindWrapper,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rain: Option<RainWrapper>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow: Option<SnowWrapper>,
    #[serde(with = "timestamp")]
    #[schema(description = "Current Datetime (Unix Timestamp)")]
    pub dt: DateTime<Utc>,
    pub sys: SysWrapper,
    #[schema(description = "Timezone (seconds offset from UTC)")]
    pub timezone: i32,
    #[schema(description = "Location Name")]
    pub name: StackString,
}

impl From<WeatherData> for WeatherDataWrapper {
    fn from(item: WeatherData) -> Self {
        Self {
            coord: item.coord.into(),
            weather: item.weather.into_iter().map(Into::into).collect(),
            base: item.base,
            main: item.main.into(),
            visibility: item.visibility.map(Into::into),
            wind: item.wind.into(),
            rain: item.rain.map(Into::into),
            snow: item.snow.map(Into::into),
            dt: item.dt,
            sys: item.sys.into(),
            timezone: item.timezone.into(),
            name: item.name,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Schema)]
pub struct WeatherCondWrapper {
    pub main: StackString,
    pub description: StackString,
}

impl From<WeatherCond> for WeatherCondWrapper {
    fn from(item: WeatherCond) -> Self {
        Self {
            main: item.main,
            description: item.description,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Schema)]
pub struct WeatherMainWrapper {
    #[schema(description = "Temperature (K)")]
    pub temp: f64,
    #[schema(description = "Feels Like Temperature (K)")]
    pub feels_like: f64,
    #[schema(description = "Minimum Temperature (K)")]
    pub temp_min: f64,
    #[schema(description = "Maximum Temperature (K)")]
    pub temp_max: f64,
    #[schema(description = "Atmospheric Pressure (hPa, h=10^2)")]
    pub pressure: f64,
    #[schema(description = "Humidity %")]
    pub humidity: i64,
}

impl From<WeatherMain> for WeatherMainWrapper {
    fn from(item: WeatherMain) -> Self {
        Self {
            temp: item.temp.into(),
            feels_like: item.feels_like.into(),
            temp_min: item.temp_min.into(),
            temp_max: item.temp_max.into(),
            pressure: item.pressure.into(),
            humidity: item.humidity.into(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct WindWrapper {
    #[schema(description = "Speed (m/s)")]
    pub speed: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(description = "Direction (degrees)")]
    pub deg: Option<f64>,
}

impl From<Wind> for WindWrapper {
    fn from(item: Wind) -> Self {
        Self {
            speed: item.speed.into(),
            deg: item.deg.map(Into::into),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct RainWrapper {
    #[serde(alias = "3h", skip_serializing_if = "Option::is_none")]
    #[schema(description = "Rain (mm over previous 3 hours)")]
    pub three_hour: Option<f64>,
}

impl From<Rain> for RainWrapper {
    fn from(item: Rain) -> Self {
        Self {
            three_hour: item.three_hour.map(Into::into),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct SnowWrapper {
    #[serde(alias = "3h", skip_serializing_if = "Option::is_none")]
    #[schema(description = "Snow (mm over previous 3 hours)")]
    pub three_hour: Option<f64>,
}

impl From<Snow> for SnowWrapper {
    fn from(item: Snow) -> Self {
        Self {
            three_hour: item.three_hour.map(Into::into),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct SysWrapper {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<StackString>,
    #[serde(with = "timestamp")]
    #[schema(description = "Sunrise (Unix Timestamp)")]
    pub sunrise: DateTime<Utc>,
    #[serde(with = "timestamp")]
    #[schema(description = "Sunset (Unix Timestamp)")]
    pub sunset: DateTime<Utc>,
}

impl From<Sys> for SysWrapper {
    fn from(item: Sys) -> Self {
        Self {
            country: item.country,
            sunrise: item.sunrise,
            sunset: item.sunset,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct WeatherForecastWrapper {
    #[schema(description = "Main Forecast Entries")]
    pub list: Vec<ForecastEntryWrapper>,
    #[schema(description = "City Information")]
    pub city: CityEntryWrapper,
}

impl From<WeatherForecast> for WeatherForecastWrapper {
    fn from(item: WeatherForecast) -> Self {
        Self {
            list: item.list.into_iter().map(Into::into).collect(),
            city: item.city.into(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct ForecastEntryWrapper {
    #[serde(with = "timestamp")]
    #[schema(description = "Forecasted DateTime (Unix Timestamp)")]
    pub dt: DateTime<Utc>,
    pub main: ForecastMainWrapper,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rain: Option<RainWrapper>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow: Option<SnowWrapper>,
}

impl From<ForecastEntry> for ForecastEntryWrapper {
    fn from(item: ForecastEntry) -> Self {
        Self {
            dt: item.dt,
            main: item.main.into(),
            rain: item.rain.map(Into::into),
            snow: item.snow.map(Into::into),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct CityEntryWrapper {
    #[schema(description = "Timezone (seconds offset from UTC)")]
    pub timezone: i32,
    #[serde(with = "timestamp")]
    #[schema(description = "Sunrise (Unix Timestamp)")]
    pub sunrise: DateTime<Utc>,
    #[serde(with = "timestamp")]
    #[schema(description = "Sunset (Unix Timestamp)")]
    pub sunset: DateTime<Utc>,
}

impl From<CityEntry> for CityEntryWrapper {
    fn from(item: CityEntry) -> Self {
        Self {
            timezone: item.timezone.into(),
            sunrise: item.sunrise,
            sunset: item.sunset,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema)]
pub struct ForecastMainWrapper {
    #[schema(description = "Temperature (K)")]
    pub temp: f64,
    #[schema(description = "Feels Like Temperature (K)")]
    pub feels_like: f64,
    #[schema(description = "Minimum Temperature (K)")]
    pub temp_min: f64,
    #[schema(description = "Maximum Temperature (K)")]
    pub temp_max: f64,
    #[schema(description = "Atmospheric Pressure (hPa, h=10^2)")]
    pub pressure: f64,
    #[schema(description = "Pressure at Sea Level (hPa, h=10^2)")]
    pub sea_level: f64,
    #[schema(description = "Pressure at Ground Level (hPa, h=10^2)")]
    pub grnd_level: f64,
    #[schema(description = "Humidity %")]
    pub humidity: i64,
}

impl From<ForecastMain> for ForecastMainWrapper {
    fn from(item: ForecastMain) -> Self {
        Self {
            temp: item.temp.into(),
            feels_like: item.feels_like.into(),
            temp_min: item.temp_min.into(),
            temp_max: item.temp_max.into(),
            pressure: item.pressure.into(),
            sea_level: item.sea_level.into(),
            grnd_level: item.grnd_level.into(),
            humidity: item.humidity.into(),
        }
    }
}
