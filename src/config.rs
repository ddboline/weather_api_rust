use anyhow::{format_err, Error};
use serde::Deserialize;
use std::{ops::Deref, path::Path, sync::Arc};

use weather_util_rust::{latitude::Latitude, longitude::Longitude};

/// Configuration data
#[derive(Default, Debug, Deserialize, PartialEq)]
pub struct ConfigInner {
    /// openweathermap.org api key
    pub api_key: String,
    /// openweathermap.org api endpoint
    #[serde(default = "default_api_endpoint")]
    pub api_endpoint: String,
    /// api path (default `data/2.5/`)
    #[serde(default = "default_api_path")]
    pub api_path: String,
    /// optional default zipcode
    pub zipcode: Option<u64>,
    /// optional default country code
    pub country_code: Option<String>,
    /// optional default city name
    pub city_name: Option<String>,
    /// optional default latitude
    pub lat: Option<Latitude>,
    /// optional default longitude
    pub lon: Option<Longitude>,
    #[serde(default = "default_port")]
    pub port: u32,
}

fn default_port() -> u32 {
    3097
}
fn default_api_endpoint() -> String {
    "api.openweathermap.org".to_string()
}
fn default_api_path() -> String {
    "data/2.5/".to_string()
}

/// Configuration struct
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Config(Arc<ConfigInner>);

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pull in configuration data using `[dotenv](https://crates.io/dotenv)`.
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
    /// let config = Config::init_config()?;
    /// assert_eq!(config.api_key, Some("api_key_value".into()));
    /// assert_eq!(config.api_endpoint, Some("api.openweathermap.org".into()));
    /// # Ok(())
    /// # }
    /// ```
    pub fn init_config() -> Result<Self, Error> {
        let fname = Path::new("config.env");
        let config_dir = dirs::config_dir().ok_or_else(|| format_err!("No CONFIG directory"))?;
        let default_fname = config_dir.join("weather_api_rust").join("config.env");

        let env_file = if fname.exists() {
            fname
        } else {
            &default_fname
        };

        dotenv::dotenv().ok();

        if env_file.exists() {
            dotenv::from_path(env_file).ok();
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

#[cfg(test)]
mod test {
    use anyhow::Error;

    use crate::config::{Config, default_api_endpoint};

    #[test]
    fn test_config() -> Result<(), Error> {
        let config = Config::default();
        assert_eq!(&config.api_endpoint, "");

        let config = Config::init_config()?;
        if let Some(api_key) = std::env::var_os("API_KEY") {
            assert_eq!(api_key.to_string_lossy().as_ref(), config.api_key.as_str());
        }

        assert_eq!(Config::default(), Config::new());
        assert_eq!(&default_api_endpoint(), "api.openweathermap.org");
        Ok(())
    }
}
