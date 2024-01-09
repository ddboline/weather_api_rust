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

use anyhow::{format_err, Error};
use api_options::ApiOptions;
use date_time_wrapper::DateTimeWrapper;
use derive_more::{From, Into};
use rand::{
    distributions::{Distribution, Uniform},
    thread_rng,
};
use rweb::Schema;
use rweb_helper::{derive_rweb_schema, DateTimeType, UuidWrapper};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{future::Future, path::Path, time::Duration};
use time::UtcOffset;
use tokio::{process::Command, time::sleep};

use weather_api_common::weather_element::{PlotData, PlotPoint};
use weather_util_rust::{
    precipitation::Precipitation,
    weather_api::GeoLocation,
    weather_data::{Coord, Rain, Snow, Sys, WeatherCond, WeatherData, WeatherMain, Wind},
    weather_forecast::{CityEntry, ForecastEntry, ForecastMain, WeatherForecast},
    StringType,
};

use crate::model::WeatherDataDB;

#[derive(Into, From, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct CoordWrapper(Coord);

derive_rweb_schema!(CoordWrapper, _CoordWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "Coordinates")]
struct _CoordWrapper {
    #[schema(description = "Longitude")]
    lon: f64,
    #[schema(description = "Latitude")]
    lat: f64,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct WeatherDataDBWrapper(WeatherDataDB);

derive_rweb_schema!(WeatherDataDBWrapper, _WeatherDataDBWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "WeatherDataDB")]
struct _WeatherDataDBWrapper {
    #[schema(description = "ID")]
    id: UuidWrapper,
    #[schema(description = "Unix Timestamp")]
    dt: i32,
    #[schema(description = "Created At Datetime")]
    created_at: DateTimeType,
    #[schema(description = "Location Name")]
    location_name: StringType,
    #[schema(description = "Latitude")]
    latitude: f64,
    #[schema(description = "Longitude")]
    longitude: f64,
    #[schema(description = "Condition")]
    condition: StringType,
    #[schema(description = "Temperature (K)")]
    temperature: f64,
    #[schema(description = "Minimum Temperature (K)")]
    temperature_minimum: f64,
    #[schema(description = "Maximum Temperature (K)")]
    temperature_maximum: f64,
    #[schema(description = "Pressure (kPa)")]
    pressure: f64,
    #[schema(description = "Humidity (percent x 100)")]
    humidity: i32,
    #[schema(description = "Visibility (meters)")]
    visibility: Option<f64>,
    #[schema(description = "Rain (mm per hour)")]
    rain: Option<f64>,
    #[schema(description = "Snow (mm per hour)")]
    snow: Option<f64>,
    #[schema(description = "Wind Speed (m/s)")]
    wind_speed: f64,
    #[schema(description = "Wind Direction (degrees)")]
    wind_direction: Option<f64>,
    #[schema(description = "Country Code (ISO 3166-1 alpha-2)")]
    country: StringType,
    #[schema(description = "Sunrise Datetime")]
    sunrise: DateTimeType,
    #[schema(description = "Sunset Datetime")]
    sunset: DateTimeType,
    #[schema(description = "Timezone UTC Offset (seconds)")]
    timezone: i32,
    #[schema(description = "Server (dilepton-tower/dilepton-cloud)")]
    server: StringType,
}

// Weather Data
#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct WeatherDataWrapper(WeatherData);

derive_rweb_schema!(WeatherDataWrapper, _WeatherDataWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "WeatherData")]
struct _WeatherDataWrapper {
    #[schema(description = "Coordinates")]
    coord: CoordWrapper,
    #[schema(description = "Weather Conditions")]
    weather: Vec<WeatherCondWrapper>,
    base: StringType,
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
    name: StringType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WeatherCondWrapper(WeatherCond);

derive_rweb_schema!(WeatherCondWrapper, _WeatherCondWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "WeatherConditions")]
struct _WeatherCondWrapper {
    id: usize,
    main: StringType,
    description: StringType,
    icon: StringType,
}

#[derive(Into, From, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct WeatherMainWrapper(WeatherMain);

derive_rweb_schema!(WeatherMainWrapper, _WeatherMainWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "WeatherMain")]
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
#[schema(component = "Wind")]
struct _WindWrapper {
    #[schema(description = "Speed (m/s)")]
    speed: f64,
    #[schema(description = "Direction (degrees)")]
    deg: Option<f64>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Schema, Copy)]
#[schema(component = "Rain")]
pub struct RainWrapper {
    #[serde(alias = "3h", skip_serializing_if = "Option::is_none")]
    #[schema(description = "Rain (mm over previous 3 hours)")]
    pub three_hour: Option<f64>,
    #[serde(alias = "1h", skip_serializing_if = "Option::is_none")]
    #[schema(description = "Rain (mm over previous hour)")]
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

#[derive(Deserialize, Serialize, Debug, Clone, Schema, Copy)]
#[schema(component = "Snow")]
pub struct SnowWrapper {
    #[serde(alias = "3h", skip_serializing_if = "Option::is_none")]
    #[schema(description = "Snow (mm over previous 3 hours)")]
    pub three_hour: Option<f64>,
    #[serde(alias = "1h", skip_serializing_if = "Option::is_none")]
    #[schema(description = "Rain (mm over previous hour)")]
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

derive_rweb_schema!(SysWrapper, _SysWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "SystemData")]
struct _SysWrapper {
    country: Option<StringType>,
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
#[schema(component = "WeatherForecast")]
struct _WeatherForecastWrapper {
    #[schema(description = "Main Forecast Entries")]
    list: Vec<ForecastEntryWrapper>,
    #[schema(description = "City Information")]
    city: CityEntryWrapper,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct GeoLocationWrapper(GeoLocation);

derive_rweb_schema!(GeoLocationWrapper, _GeoLocationWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "GeoLocation")]
struct _GeoLocationWrapper {
    name: StringType,
    lat: f64,
    lon: f64,
    country: StringType,
    zip: Option<StringType>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ForecastEntryWrapper(ForecastEntry);

derive_rweb_schema!(ForecastEntryWrapper, _ForecastEntryWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "ForecastEntry")]
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
#[schema(component = "CityEntry")]
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
#[schema(component = "ForecastMain")]
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

derive_rweb_schema!(PlotPointWrapper, _PlotPointWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "PlotPoint")]
struct _PlotPointWrapper {
    #[schema(description = "Datetime")]
    datetime: DateTimeType,
    #[schema(description = "Value")]
    value: f64,
}

#[derive(Into, From, Deserialize, Serialize, Debug, Clone)]
pub struct PlotDataWrapper(PlotData);

derive_rweb_schema!(PlotDataWrapper, _PlotDataWrapper);

#[allow(dead_code)]
#[derive(Schema)]
#[schema(component = "PlotData")]
struct _PlotDataWrapper {
    #[schema(description = "Plot Data")]
    plot_data: Vec<PlotPointWrapper>,
    #[schema(description = "Plot Title")]
    title: String,
    #[schema(description = "Plot X-axis Label")]
    xaxis: String,
    #[schema(description = "Plot Y-axis Label")]
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
    let range = Uniform::from(0..1000);
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
        xaxis: "".into(),
        yaxis: "F".into(),
    });

    let plot_url = format!("/weather/forecast-plots/precipitation?{options}");

    plots.push(PlotData {
        plot_url,
        title: "Precipitation Forecast".into(),
        xaxis: "".into(),
        yaxis: "in".into(),
    });

    Ok(plots)
}

pub fn get_forecast_temp_plot(forecast: &WeatherForecast) -> Result<Vec<PlotPoint>, Error> {
    let fo: UtcOffset = forecast.city.timezone.into();
    forecast
        .list
        .iter()
        .map(|entry| {
            let temp = entry.main.temp.fahrenheit();
            Ok(PlotPoint {
                datetime: entry.dt.to_offset(fo),
                value: temp,
            })
        })
        .collect()
}

pub fn get_forecast_precip_plot(forecast: &WeatherForecast) -> Result<Vec<PlotPoint>, Error> {
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
            Ok(PlotPoint {
                datetime: entry.dt.to_offset(fo),
                value: (rain + snow).inches(),
            })
        })
        .collect()
}

pub fn get_history_plots(query: &str, weather: &WeatherData) -> Result<Vec<PlotData>, Error> {
    let mut plots = Vec::new();

    let plot_url = format!("/weather/history-plots/temperature?{query}");

    plots.push(PlotData {
        plot_url,
        title: format!(
            "Temperature Forecast {:0.1} F / {:0.1} C",
            weather.main.temp.fahrenheit(),
            weather.main.temp.celcius()
        ),
        xaxis: "".into(),
        yaxis: "F".into(),
    });

    let plot_url = format!("/weather/history-plots/precipitation?{query}");

    plots.push(PlotData {
        plot_url,
        title: "Precipitation Forecast".into(),
        xaxis: "".into(),
        yaxis: "in".into(),
    });

    Ok(plots)
}

pub fn get_history_temperature_plot(history: &[WeatherData]) -> Result<Vec<PlotPoint>, Error> {
    if history.is_empty() {
        return Ok(Vec::new());
    }
    let weather = history.last().unwrap();
    let fo: UtcOffset = weather.timezone.into();
    history
        .iter()
        .map(|w| {
            let temp = w.main.temp.fahrenheit();
            Ok(PlotPoint {
                datetime: w.dt.to_offset(fo),
                value: temp,
            })
        })
        .collect()
}

pub fn get_history_precip_plot(history: &[WeatherData]) -> Result<Vec<PlotPoint>, Error> {
    if history.is_empty() {
        return Ok(Vec::new());
    }
    let weather = history.last().unwrap();
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
            Ok(PlotPoint {
                datetime: w.dt.to_offset(fo),
                value: (rain + snow).inches(),
            })
        })
        .collect()
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
