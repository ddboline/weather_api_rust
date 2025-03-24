#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::similar_names)]
#![allow(clippy::result_large_err)]
#![allow(clippy::unused_async)]
#![allow(clippy::unsafe_derive_deserialize)]
#![allow(clippy::missing_errors_doc)]

pub mod api_options;
pub mod app;
pub mod config;
pub mod country_code_wrapper;
pub mod date_time_wrapper;
pub mod errors;
pub mod latitude_wrapper;
pub mod logged_user;
pub mod longitude_wrapper;
pub mod model;
pub mod parse_opts;
pub mod pgpool;
pub mod polars_analysis;
pub mod routes;
pub mod s3_sync;

use anyhow::{Error, format_err};
use api_options::ApiOptions;
use date_time_wrapper::DateTimeWrapper;
use derive_more::{From, Into};
use rand::{
    distr::{Distribution, Uniform},
    rng as thread_rng,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{future::Future, path::Path, time::Duration};
use time::{OffsetDateTime, UtcOffset};
use tokio::{process::Command, time::sleep};
use utoipa::ToSchema;
use utoipa_helper::derive_utoipa_schema;
use uuid::Uuid;

use weather_api_common::weather_element::{PlotData, PlotPoint};
use weather_util_rust::{
    StringType,
    precipitation::Precipitation,
    weather_api::GeoLocation,
    weather_data::{Coord, Rain, Snow, Sys, WeatherCond, WeatherData, WeatherMain, Wind},
    weather_forecast::{CityEntry, ForecastEntry, ForecastMain, WeatherForecast},
};

use crate::model::WeatherDataDB;

#[derive(Into, From, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct CoordWrapper(Coord);

derive_utoipa_schema!(CoordWrapper, _CoordWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// Coordinates")]
struct _CoordWrapper {
    // Longitude")]
    lon: f64,
    // Latitude")]
    lat: f64,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct WeatherDataDBWrapper(WeatherDataDB);

derive_utoipa_schema!(WeatherDataDBWrapper, _WeatherDataDBWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// WeatherDataDB")]
struct _WeatherDataDBWrapper {
    // ID")]
    id: Uuid,
    // Unix Timestamp")]
    dt: i32,
    // Created At Datetime")]
    created_at: OffsetDateTime,
    // Location Name")]
    location_name: StringType,
    // Latitude")]
    latitude: f64,
    // Longitude")]
    longitude: f64,
    // Condition")]
    condition: StringType,
    // Temperature (K)")]
    temperature: f64,
    // Minimum Temperature (K)")]
    temperature_minimum: f64,
    // Maximum Temperature (K)")]
    temperature_maximum: f64,
    // Pressure (kPa)")]
    pressure: f64,
    // Humidity (percent x 100)")]
    humidity: i32,
    // Visibility (meters)")]
    visibility: Option<f64>,
    // Rain (mm per hour)")]
    rain: Option<f64>,
    // Snow (mm per hour)")]
    snow: Option<f64>,
    // Wind Speed (m/s)")]
    wind_speed: f64,
    // Wind Direction (degrees)")]
    wind_direction: Option<f64>,
    // Country Code (ISO 3166-1 alpha-2)")]
    country: StringType,
    // Sunrise Datetime")]
    sunrise: OffsetDateTime,
    // Sunset Datetime")]
    sunset: OffsetDateTime,
    // Timezone UTC Offset (seconds)")]
    timezone: i32,
    // Server (dilepton-tower/dilepton-cloud)")]
    server: StringType,
}

// Weather Data
#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct WeatherDataWrapper(WeatherData);

derive_utoipa_schema!(WeatherDataWrapper, _WeatherDataWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// WeatherData")]
struct _WeatherDataWrapper {
    // Coordinates")]
    coord: CoordWrapper,
    // Weather Conditions")]
    weather: Vec<WeatherCondWrapper>,
    base: StringType,
    main: WeatherMainWrapper,
    // Visibility (m)")]
    visibility: Option<f64>,
    wind: WindWrapper,
    rain: Option<RainWrapper>,
    snow: Option<SnowWrapper>,
    // Current Datetime (Unix Timestamp)")]
    dt: OffsetDateTime,
    sys: SysWrapper,
    // Timezone (seconds offset from UTC)")]
    timezone: i32,
    // Location Name")]
    name: StringType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WeatherCondWrapper(WeatherCond);

derive_utoipa_schema!(WeatherCondWrapper, _WeatherCondWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// WeatherConditions")]
struct _WeatherCondWrapper {
    id: usize,
    main: StringType,
    description: StringType,
    icon: StringType,
}

#[derive(Into, From, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct WeatherMainWrapper(WeatherMain);

derive_utoipa_schema!(WeatherMainWrapper, _WeatherMainWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// WeatherMain")]
struct _WeatherMainWrapper {
    // Temperature (K)")]
    temp: f64,
    // Feels Like Temperature (K)")]
    feels_like: f64,
    // Minimum Temperature (K)")]
    temp_min: f64,
    // Maximum Temperature (K)")]
    temp_max: f64,
    // Atmospheric Pressure (hPa, h=10^2)")]
    pressure: f64,
    // Humidity %")]
    humidity: i64,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone, Copy)]
pub struct WindWrapper(Wind);

derive_utoipa_schema!(WindWrapper, _WindWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// Wind")]
struct _WindWrapper {
    // Speed (m/s)")]
    speed: f64,
    // Direction (degrees)")]
    deg: Option<f64>,
}

#[derive(Deserialize, Serialize, Debug, Clone, ToSchema, Copy)]
// Rain")]
pub struct RainWrapper {
    #[serde(alias = "3h", skip_serializing_if = "Option::is_none")]
    // Rain (mm over previous 3 hours)")]
    pub three_hour: Option<f64>,
    #[serde(alias = "1h", skip_serializing_if = "Option::is_none")]
    // Rain (mm over previous hour)")]
    pub one_hour: Option<f64>,
}

impl From<Rain> for RainWrapper {
    fn from(item: Rain) -> Self {
        Self {
            three_hour: item.three_hour.map(Precipitation::millimeters),
            one_hour: item.one_hour.map(Precipitation::millimeters),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, ToSchema, Copy)]
// Snow")]
pub struct SnowWrapper {
    #[serde(alias = "3h", skip_serializing_if = "Option::is_none")]
    // Snow (mm over previous 3 hours)")]
    pub three_hour: Option<f64>,
    #[serde(alias = "1h", skip_serializing_if = "Option::is_none")]
    // Rain (mm over previous hour)")]
    pub one_hour: Option<f64>,
}

impl From<Snow> for SnowWrapper {
    fn from(item: Snow) -> Self {
        Self {
            three_hour: item.three_hour.map(Precipitation::millimeters),
            one_hour: item.one_hour.map(Precipitation::millimeters),
        }
    }
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct SysWrapper(Sys);

derive_utoipa_schema!(SysWrapper, _SysWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// SystemData")]
struct _SysWrapper {
    country: Option<StringType>,
    // Sunrise (Unix Timestamp)")]
    sunrise: OffsetDateTime,
    // Sunset (Unix Timestamp)")]
    sunset: OffsetDateTime,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct WeatherForecastWrapper(WeatherForecast);

derive_utoipa_schema!(WeatherForecastWrapper, _WeatherForecastWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// WeatherForecast")]
struct _WeatherForecastWrapper {
    // Main Forecast Entries")]
    list: Vec<ForecastEntryWrapper>,
    // City Information")]
    city: CityEntryWrapper,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct GeoLocationWrapper(GeoLocation);

derive_utoipa_schema!(GeoLocationWrapper, _GeoLocationWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// GeoLocation")]
struct _GeoLocationWrapper {
    name: StringType,
    lat: f64,
    lon: f64,
    country: StringType,
    zip: Option<StringType>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ForecastEntryWrapper(ForecastEntry);

derive_utoipa_schema!(ForecastEntryWrapper, _ForecastEntryWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// ForecastEntry")]
struct _ForecastEntryWrapper {
    // Forecasted DateTime (Unix Timestamp)")]
    dt: OffsetDateTime,
    main: ForecastMainWrapper,
    weather: Vec<WeatherCondWrapper>,
    rain: Option<RainWrapper>,
    snow: Option<SnowWrapper>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub struct CityEntryWrapper(CityEntry);

derive_utoipa_schema!(CityEntryWrapper, _CityEntryWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// CityEntry")]
struct _CityEntryWrapper {
    // Timezone (seconds offset from UTC)")]
    timezone: i32,
    // Sunrise (Unix Timestamp)")]
    sunrise: OffsetDateTime,
    // Sunset (Unix Timestamp)")]
    sunset: OffsetDateTime,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone, Copy)]
pub struct ForecastMainWrapper(ForecastMain);

derive_utoipa_schema!(ForecastMainWrapper, _ForecastMainWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// ForecastMain")]
struct _ForecastMainWrapper {
    // Temperature (K)")]
    temp: f64,
    // Feels Like Temperature (K)")]
    feels_like: f64,
    // Minimum Temperature (K)")]
    temp_min: f64,
    // Maximum Temperature (K)")]
    temp_max: f64,
    // Atmospheric Pressure (hPa, h=10^2)")]
    pressure: f64,
    // Pressure at Sea Level (hPa, h=10^2)")]
    sea_level: f64,
    // Pressure at Ground Level (hPa, h=10^2)")]
    grnd_level: f64,
    // Humidity %")]
    humidity: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
struct PlotPointWrapper {
    datetime: DateTimeWrapper,
    value: f64,
}

impl From<PlotPoint> for PlotPointWrapper {
    fn from(v: PlotPoint) -> Self {
        Self {
            datetime: v.datetime.into(),
            value: v.value,
        }
    }
}

impl From<PlotPointWrapper> for PlotPoint {
    fn from(v: PlotPointWrapper) -> Self {
        Self {
            datetime: v.datetime.into(),
            value: v.value,
        }
    }
}

derive_utoipa_schema!(PlotPointWrapper, _PlotPointWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// PlotPoint")]
struct _PlotPointWrapper {
    // Datetime")]
    datetime: OffsetDateTime,
    // Value")]
    value: f64,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct PlotDataWrapper(PlotData);

derive_utoipa_schema!(PlotDataWrapper, _PlotDataWrapper);

#[allow(dead_code)]
#[derive(ToSchema)]
// PlotData")]
struct _PlotDataWrapper {
    // Plot Data")]
    plot_data: Vec<PlotPointWrapper>,
    // Plot Title")]
    title: String,
    // Plot X-axis Label")]
    xaxis: String,
    // Plot Y-axis Label")]
    yaxis: String,
}

/// # Errors
/// Return error after timeout
pub async fn exponential_retry<T, U, F>(closure: T) -> Result<U, Error>
where
    T: Fn() -> F,
    F: Future<Output = Result<U, Error>>,
{
    let mut timeout: f64 = 1.0;
    let range = Uniform::try_from(0..1000)?;
    loop {
        match closure().await {
            Ok(resp) => return Ok(resp),
            Err(err) => {
                sleep(Duration::from_millis((timeout * 1000.0) as u64)).await;
                timeout *= 4.0 * f64::from(range.sample(&mut thread_rng())) / 1000.0;
                if timeout >= 64.0 {
                    return Err(err);
                }
            }
        }
    }
}

/// # Errors
/// Return error if `md5sum` fails
pub async fn get_md5sum(filename: &Path) -> Result<StackString, Error> {
    if !Path::new("/usr/bin/md5sum").exists() {
        return Err(format_err!(
            "md5sum not installed (or not present at /usr/bin/md5sum"
        ));
    }
    let output = Command::new("/usr/bin/md5sum")
        .args([filename])
        .output()
        .await?;
    if output.status.success() {
        let buf = String::from_utf8_lossy(&output.stdout);
        for line in buf.split('\n') {
            if let Some(entry) = line.split_whitespace().next() {
                return Ok(entry.into());
            }
        }
    }
    Err(format_err!("Command failed"))
}

/// # Errors
/// Returns error if there is a syntax or parsing error
pub fn get_forecast_plots(
    options: &ApiOptions,
    weather: &WeatherData,
) -> Result<Vec<PlotData>, Error> {
    let mut plots = Vec::new();

    let options = serde_urlencoded::to_string(options)?;
    let plot_url = format!("/weather/forecast-plots/temperature?{options}");

    plots.push(PlotData {
        plot_url,
        title: format!(
            "Temperature Forecast {:0.1} F / {:0.1} C",
            weather.main.temp.fahrenheit(),
            weather.main.temp.celcius()
        ),
        xaxis: String::new(),
        yaxis: "F".into(),
    });

    let plot_url = format!("/weather/forecast-plots/precipitation?{options}");

    plots.push(PlotData {
        plot_url,
        title: "Precipitation Forecast".into(),
        xaxis: String::new(),
        yaxis: "in".into(),
    });

    Ok(plots)
}

#[must_use]
pub fn get_forecast_temp_plot(forecast: &WeatherForecast) -> Vec<PlotPoint> {
    let fo: UtcOffset = forecast.city.timezone.into();
    forecast
        .list
        .iter()
        .map(|entry| {
            let temp = entry.main.temp.fahrenheit();
            PlotPoint {
                datetime: entry.dt.to_offset(fo),
                value: temp,
            }
        })
        .collect()
}

#[must_use]
pub fn get_forecast_precip_plot(forecast: &WeatherForecast) -> Vec<PlotPoint> {
    let fo: UtcOffset = forecast.city.timezone.into();
    forecast
        .list
        .iter()
        .map(|entry| {
            let rain = if let Some(rain) = &entry.rain {
                rain.three_hour.unwrap_or_default()
            } else {
                Precipitation::default()
            };
            let snow = if let Some(snow) = &entry.snow {
                snow.three_hour.unwrap_or_default()
            } else {
                Precipitation::default()
            };
            PlotPoint {
                datetime: entry.dt.to_offset(fo),
                value: (rain + snow).inches(),
            }
        })
        .collect()
}

#[must_use]
pub fn get_history_plots(query: &str, weather: &WeatherData) -> Vec<PlotData> {
    let mut plots = Vec::new();

    let plot_url = format!("/weather/history-plots/temperature?{query}");

    plots.push(PlotData {
        plot_url,
        title: format!(
            "Temperature Forecast {:0.1} F / {:0.1} C",
            weather.main.temp.fahrenheit(),
            weather.main.temp.celcius()
        ),
        xaxis: String::new(),
        yaxis: "F".into(),
    });

    let plot_url = format!("/weather/history-plots/precipitation?{query}");

    plots.push(PlotData {
        plot_url,
        title: "Precipitation Forecast".into(),
        xaxis: String::new(),
        yaxis: "in".into(),
    });

    plots
}

#[must_use]
pub fn get_history_temperature_plot(history: &[WeatherData]) -> Vec<PlotPoint> {
    if let Some(weather) = history.last() {
        let fo: UtcOffset = weather.timezone.into();
        history
            .iter()
            .map(|w| {
                let temp = w.main.temp.fahrenheit();
                PlotPoint {
                    datetime: w.dt.to_offset(fo),
                    value: temp,
                }
            })
            .collect()
    } else {
        Vec::new()
    }
}

#[must_use]
pub fn get_history_precip_plot(history: &[WeatherData]) -> Vec<PlotPoint> {
    if let Some(weather) = history.last() {
        let fo: UtcOffset = weather.timezone.into();
        history
            .iter()
            .map(|w| {
                let rain = if let Some(rain) = &w.rain {
                    rain.one_hour.unwrap_or_default()
                } else {
                    Precipitation::default()
                };
                let snow = if let Some(snow) = &w.snow {
                    snow.one_hour.unwrap_or_default()
                } else {
                    Precipitation::default()
                };
                PlotPoint {
                    datetime: w.dt.to_offset(fo),
                    value: (rain + snow).inches(),
                }
            })
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod test {
    use utoipa_helper::derive_utoipa_test;

    use crate::{
        _CityEntryWrapper, _CoordWrapper, _ForecastEntryWrapper, _ForecastMainWrapper, _SysWrapper,
        _WeatherCondWrapper, _WeatherDataWrapper, _WeatherForecastWrapper, _WeatherMainWrapper,
        _WindWrapper, CityEntryWrapper, CoordWrapper, ForecastEntryWrapper, ForecastMainWrapper,
        SysWrapper, WeatherCondWrapper, WeatherDataWrapper, WeatherForecastWrapper,
        WeatherMainWrapper, WindWrapper,
    };

    #[test]
    fn test_types() {
        derive_utoipa_test!(CoordWrapper, _CoordWrapper);
        derive_utoipa_test!(WeatherDataWrapper, _WeatherDataWrapper);
        derive_utoipa_test!(WeatherCondWrapper, _WeatherCondWrapper);
        derive_utoipa_test!(WeatherMainWrapper, _WeatherMainWrapper);
        derive_utoipa_test!(WindWrapper, _WindWrapper);
        derive_utoipa_test!(SysWrapper, _SysWrapper);
        derive_utoipa_test!(WeatherForecastWrapper, _WeatherForecastWrapper);
        derive_utoipa_test!(ForecastEntryWrapper, _ForecastEntryWrapper);
        derive_utoipa_test!(CityEntryWrapper, _CityEntryWrapper);
        derive_utoipa_test!(ForecastMainWrapper, _ForecastMainWrapper);
    }
}
