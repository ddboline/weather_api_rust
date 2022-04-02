#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::used_underscore_binding)]

pub mod api_options;
pub mod app;
pub mod config;
pub mod country_code_wrapper;
pub mod errors;
pub mod latitude_wrapper;
pub mod longitude_wrapper;
pub mod routes;
pub mod timestamp;

use derive_more::{From, Into};
use rweb::Schema;
use rweb_helper::{derive_rweb_schema, DateTimeType};
use serde::{Deserialize, Serialize};
use stack_string::StackString;

use weather_util_rust::{
    weather_data::{Coord, Rain, Snow, Sys, WeatherCond, WeatherData, WeatherMain, Wind},
    weather_forecast::{CityEntry, ForecastEntry, ForecastMain, WeatherForecast},
};

#[derive(Into, From, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct CoordWrapper(Coord);

derive_rweb_schema!(CoordWrapper, _CoordWrapper);

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

derive_rweb_schema!(WeatherDataWrapper, _WeatherDataWrapper);

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
    dt: DateTimeType,
    sys: SysWrapper,
    #[schema(description = "Timezone (seconds offset from UTC)")]
    timezone: i32,
    #[schema(description = "Location Name")]
    name: StackString,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WeatherCondWrapper(WeatherCond);

derive_rweb_schema!(WeatherCondWrapper, _WeatherCondWrapper);

#[allow(dead_code)]
#[derive(Schema)]
struct _WeatherCondWrapper {
    id: usize,
    main: StackString,
    description: StackString,
    icon: StackString,
}

#[derive(Into, From, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct WeatherMainWrapper(WeatherMain);

derive_rweb_schema!(WeatherMainWrapper, _WeatherMainWrapper);

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

derive_rweb_schema!(WindWrapper, _WindWrapper);

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

derive_rweb_schema!(SysWrapper, _SysWrapper);

#[allow(dead_code)]
#[derive(Schema)]
struct _SysWrapper {
    country: Option<StackString>,
    #[schema(description = "Sunrise (Unix Timestamp)")]
    sunrise: DateTimeType,
    #[schema(description = "Sunset (Unix Timestamp)")]
    sunset: DateTimeType,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct WeatherForecastWrapper(WeatherForecast);

derive_rweb_schema!(WeatherForecastWrapper, _WeatherForecastWrapper);

#[allow(dead_code)]
#[derive(Schema)]
struct _WeatherForecastWrapper {
    #[schema(description = "Main Forecast Entries")]
    list: Vec<ForecastEntryWrapper>,
    #[schema(description = "City Information")]
    city: CityEntryWrapper,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ForecastEntryWrapper(ForecastEntry);

derive_rweb_schema!(ForecastEntryWrapper, _ForecastEntryWrapper);

#[allow(dead_code)]
#[derive(Schema)]
struct _ForecastEntryWrapper {
    #[schema(description = "Forecasted DateTime (Unix Timestamp)")]
    dt: DateTimeType,
    main: ForecastMainWrapper,
    weather: Vec<WeatherCondWrapper>,
    rain: Option<RainWrapper>,
    snow: Option<SnowWrapper>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub struct CityEntryWrapper(CityEntry);

derive_rweb_schema!(CityEntryWrapper, _CityEntryWrapper);

#[allow(dead_code)]
#[derive(Schema)]
struct _CityEntryWrapper {
    #[schema(description = "Timezone (seconds offset from UTC)")]
    timezone: i32,
    #[schema(description = "Sunrise (Unix Timestamp)")]
    sunrise: DateTimeType,
    #[schema(description = "Sunset (Unix Timestamp)")]
    sunset: DateTimeType,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone, Copy)]
pub struct ForecastMainWrapper(ForecastMain);

derive_rweb_schema!(ForecastMainWrapper, _ForecastMainWrapper);

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

#[cfg(test)]
mod test {
    use rweb_helper::derive_rweb_test;

    use crate::{
        CityEntryWrapper, CoordWrapper, ForecastEntryWrapper, ForecastMainWrapper, SysWrapper,
        WeatherCondWrapper, WeatherDataWrapper, WeatherForecastWrapper, WeatherMainWrapper,
        WindWrapper, _CityEntryWrapper, _CoordWrapper, _ForecastEntryWrapper, _ForecastMainWrapper,
        _SysWrapper, _WeatherCondWrapper, _WeatherDataWrapper, _WeatherForecastWrapper,
        _WeatherMainWrapper, _WindWrapper,
    };

    #[test]
    fn test_types() {
        derive_rweb_test!(CoordWrapper, _CoordWrapper);
        derive_rweb_test!(WeatherDataWrapper, _WeatherDataWrapper);
        derive_rweb_test!(WeatherCondWrapper, _WeatherCondWrapper);
        derive_rweb_test!(WeatherMainWrapper, _WeatherMainWrapper);
        derive_rweb_test!(WindWrapper, _WindWrapper);
        derive_rweb_test!(SysWrapper, _SysWrapper);
        derive_rweb_test!(WeatherForecastWrapper, _WeatherForecastWrapper);
        derive_rweb_test!(ForecastEntryWrapper, _ForecastEntryWrapper);
        derive_rweb_test!(CityEntryWrapper, _CityEntryWrapper);
        derive_rweb_test!(ForecastMainWrapper, _ForecastMainWrapper);
    }
}
