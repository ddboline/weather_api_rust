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
use derive_more::{From, Into};
use rweb::{
    openapi::{ComponentDescriptor, ComponentOrInlineSchema, Entity},
    Schema,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;

use weather_util_rust::{
    weather_data::{Coord, Rain, Snow, Sys, WeatherCond, WeatherData, WeatherMain, Wind},
    weather_forecast::{CityEntry, ForecastEntry, ForecastMain, WeatherForecast},
};

#[derive(Into, From, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct CoordWrapper(Coord);

impl Entity for CoordWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _CoordWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _CoordWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _CoordWrapper {
    #[schema(description = "Longitude")]
    lon: f64,
    #[schema(description = "Latitude")]
    lat: f64,
}

// Weather Data
#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct WeatherDataWrapper(WeatherData);

impl Entity for WeatherDataWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _WeatherDataWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _WeatherDataWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _WeatherDataWrapper {
    #[schema(description = "Coordinates")]
    coord: CoordWrapper,
    #[schema(description = "Weather Conditions")]
    weather: Vec<WeatherCondWrapper>,
    base: StackString,
    main: WeatherMainWrapper,
    #[schema(description = "Visibility (m)")]
    visibility: Option<f64>,
    wind: WindWrapper,
    rain: Option<RainWrapper>,
    snow: Option<SnowWrapper>,
    #[schema(description = "Current Datetime (Unix Timestamp)")]
    dt: DateTime<Utc>,
    sys: SysWrapper,
    #[schema(description = "Timezone (seconds offset from UTC)")]
    timezone: i32,
    #[schema(description = "Location Name")]
    name: StackString,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WeatherCondWrapper(WeatherCond);

impl Entity for WeatherCondWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _WeatherCondWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _WeatherCondWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _WeatherCondWrapper {
    main: StackString,
    description: StackString,
}

#[derive(Into, From, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct WeatherMainWrapper(WeatherMain);

impl Entity for WeatherMainWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _WeatherMainWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _WeatherMainWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _WeatherMainWrapper {
    #[schema(description = "Temperature (K)")]
    temp: f64,
    #[schema(description = "Feels Like Temperature (K)")]
    feels_like: f64,
    #[schema(description = "Minimum Temperature (K)")]
    temp_min: f64,
    #[schema(description = "Maximum Temperature (K)")]
    temp_max: f64,
    #[schema(description = "Atmospheric Pressure (hPa, h=10^2)")]
    pressure: f64,
    #[schema(description = "Humidity %")]
    humidity: i64,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone, Copy)]
pub struct WindWrapper(Wind);

impl Entity for WindWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _WindWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _WindWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _WindWrapper {
    #[schema(description = "Speed (m/s)")]
    speed: f64,
    #[schema(description = "Direction (degrees)")]
    deg: Option<f64>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema, Copy)]
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

#[derive(Deserialize, Serialize, Debug, Clone, Schema, Copy)]
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

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct SysWrapper(Sys);

impl Entity for SysWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _SysWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _SysWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _SysWrapper {
    country: Option<StackString>,
    #[schema(description = "Sunrise (Unix Timestamp)")]
    sunrise: DateTime<Utc>,
    #[schema(description = "Sunset (Unix Timestamp)")]
    sunset: DateTime<Utc>,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct WeatherForecastWrapper(WeatherForecast);

impl Entity for WeatherForecastWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _WeatherForecastWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _WeatherForecastWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _WeatherForecastWrapper {
    #[schema(description = "Main Forecast Entries")]
    list: Vec<ForecastEntryWrapper>,
    #[schema(description = "City Information")]
    city: CityEntryWrapper,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub struct ForecastEntryWrapper(ForecastEntry);

impl Entity for ForecastEntryWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _ForecastEntryWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _ForecastEntryWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _ForecastEntryWrapper {
    #[schema(description = "Forecasted DateTime (Unix Timestamp)")]
    dt: DateTime<Utc>,
    main: ForecastMainWrapper,
    rain: Option<RainWrapper>,
    snow: Option<SnowWrapper>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub struct CityEntryWrapper(CityEntry);

impl Entity for CityEntryWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _CityEntryWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _CityEntryWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _CityEntryWrapper {
    #[schema(description = "Timezone (seconds offset from UTC)")]
    timezone: i32,
    #[schema(description = "Sunrise (Unix Timestamp)")]
    sunrise: DateTime<Utc>,
    #[schema(description = "Sunset (Unix Timestamp)")]
    sunset: DateTime<Utc>,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone, Copy)]
pub struct ForecastMainWrapper(ForecastMain);

impl Entity for ForecastMainWrapper {
    fn type_name() -> std::borrow::Cow<'static, str> {
        _ForecastMainWrapper::type_name()
    }
    fn describe(comp_d: &mut ComponentDescriptor) -> ComponentOrInlineSchema {
        _ForecastMainWrapper::describe(comp_d)
    }
}

#[allow(dead_code)]
#[derive(Schema)]
struct _ForecastMainWrapper {
    #[schema(description = "Temperature (K)")]
    temp: f64,
    #[schema(description = "Feels Like Temperature (K)")]
    feels_like: f64,
    #[schema(description = "Minimum Temperature (K)")]
    temp_min: f64,
    #[schema(description = "Maximum Temperature (K)")]
    temp_max: f64,
    #[schema(description = "Atmospheric Pressure (hPa, h=10^2)")]
    pressure: f64,
    #[schema(description = "Pressure at Sea Level (hPa, h=10^2)")]
    sea_level: f64,
    #[schema(description = "Pressure at Ground Level (hPa, h=10^2)")]
    grnd_level: f64,
    #[schema(description = "Humidity %")]
    humidity: i64,
}
