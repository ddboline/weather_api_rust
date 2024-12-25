use anyhow::Error;
use isocountry::CountryCode;
use serde::{Deserialize, Deserializer};
use stack_string::{format_sstr, SmallString, StackString};
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use weather_api_common::get_parameters;
use weather_util_rust::{latitude::Latitude, longitude::Longitude, weather_api::WeatherLocation};

/// Configuration data
#[derive(Default, Debug, Deserialize, PartialEq, Eq)]
pub struct ConfigInner {
    /// openweathermap.org api key
    pub api_key: SmallString<32>,
    /// openweathermap.org api endpoint
    #[serde(default = "default_api_endpoint")]
    pub api_endpoint: StackString,
    /// api path (default `data/2.5/`)
    #[serde(default = "default_api_path")]
    pub api_path: StackString,
    /// Geo Api path (default is `geo/1.0/`)
    #[serde(default = "default_geo_path")]
    pub geo_path: StackString,
    /// optional default zipcode
    pub zipcode: Option<u64>,
    /// optional default country code
    pub country_code: Option<CountryCode>,
    /// optional default city name
    pub city_name: Option<StackString>,
    /// optional default latitude
    pub lat: Option<Latitude>,
    /// optional default longitude
    pub lon: Option<Longitude>,
    #[serde(default = "default_host")]
    pub host: StackString,
    #[serde(default = "default_port")]
    pub port: u32,
    #[serde(deserialize_with = "deserialize_semi_colon_delimited_locations", default = "Vec::new")]
    pub locations_to_record: Vec<WeatherLocation>,
    pub database_url: StackString,
    #[serde(default = "default_server")]
    pub server: StackString,
    #[serde(default = "default_secret_path")]
    pub secret_path: PathBuf,
    #[serde(default = "default_secret_path")]
    pub jwt_secret_path: PathBuf,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
    #[serde(default = "default_s3_bucket")]
    pub s3_bucket: StackString,
}
fn default_host() -> StackString {
    "0.0.0.0".into()
}
fn default_port() -> u32 {
    3097
}
fn default_api_endpoint() -> StackString {
    "api.openweathermap.org".into()
}
fn default_api_path() -> StackString {
    "data/2.5/".into()
}
fn default_geo_path() -> StackString {
    "geo/1.0/".into()
}
fn default_server() -> StackString {
    "N/A".into()
}
fn default_secret_path() -> PathBuf {
    dirs::config_dir()
        .unwrap()
        .join("aws_app_rust")
        .join("secret.bin")
}
fn default_cache_dir() -> PathBuf {
    dirs::home_dir()
        .expect("No home directory")
        .join(".weather-data-cache")
}
fn default_s3_bucket() -> StackString {
    format_sstr!("weather-data-backup-ddboline")
}

/// Configuration struct
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Config(Arc<ConfigInner>);

impl Config {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pull in configuration data using `[dotenvy](https://crates.io/dotenvy)`.
    ///
    /// If a .env file exists in the current directory, pull in any ENV
    /// variables in it.
    ///
    /// Next, if a config file exists in the current directory named config.env,
    /// or if a config file exists at `${HOME}/.config/weather_util/config.env`,
    /// set ENV variables using it.
    ///
    /// Config files should have lines of the following form:
    /// `API_KEY=api_key_value`
    ///
    /// # Example
    ///
    /// ```
    /// # use std::env::set_var;
    /// use weather_util_rust::config::Config;
    /// use anyhow::Error;
    ///
    /// # fn main() -> Result<(), Error> {
    /// # set_var("API_KEY", "api_key_value");
    /// # set_var("API_ENDPOINT", "api.openweathermap.org");
    /// let config = Config::init_config(None)?;
    /// assert_eq!(config.api_key, Some("api_key_value".into()));
    /// assert_eq!(&config.api_endpoint, "api.openweathermap.org");
    /// # Ok(())
    /// # }
    /// ```
    /// # Errors
    /// Return error if deserializing environment variables fails
    pub fn init_config(config_path: Option<&Path>) -> Result<Self, Error> {
        let fname = config_path.unwrap_or_else(|| Path::new("config.env"));
        let config_dir = dirs::config_dir().unwrap_or_else(|| "./".into());
        let default_fname = config_dir.join("weather_api_rust").join("config.env");

        let env_file = if fname.exists() {
            fname
        } else {
            &default_fname
        };

        dotenvy::dotenv().ok();

        if env_file.exists() {
            dotenvy::from_path(env_file).ok();
        }

        let conf: ConfigInner = envy::from_env()?;

        Ok(Self(Arc::new(conf)))
    }
}

impl Deref for Config {
    type Target = ConfigInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn deserialize_semi_colon_delimited_locations<'de, D>(
    deserializer: D,
) -> Result<Vec<WeatherLocation>, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)
        .map(|s| s.split(';').map(get_parameters).collect())
        .map_err(Into::into)
}

#[cfg(test)]
mod test {
    use anyhow::Error;

    use crate::config::{default_api_endpoint, Config};

    #[test]
    fn test_config() -> Result<(), Error> {
        let config = Config::default();
        assert_eq!(&config.api_endpoint, "");

        let config = Config::init_config(None)?;
        if let Some(api_key) = std::env::var_os("API_KEY") {
            assert_eq!(api_key.to_string_lossy().as_ref(), config.api_key.as_str());
        }

        assert_eq!(Config::default(), Config::new());
        assert_eq!(&default_api_endpoint(), "api.openweathermap.org");
        Ok(())
    }
}
