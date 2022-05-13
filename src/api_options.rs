use rweb::Schema;
use serde::{Deserialize, Serialize};
use stack_string::{SmallString, StackString};
use std::borrow::Cow;

use weather_util_rust::weather_api::{WeatherApi, WeatherLocation};

use crate::{
    config::Config, country_code_wrapper::CountryCodeWrapper, errors::ServiceError as Error,
    latitude_wrapper::LatitudeWrapper, longitude_wrapper::LongitudeWrapper,
};

#[derive(Serialize, Deserialize, Schema)]
pub struct ApiOptions {
    pub zip: Option<u64>,
    pub country_code: Option<CountryCodeWrapper>,
    pub q: Option<StackString>,
    pub lat: Option<LatitudeWrapper>,
    pub lon: Option<LongitudeWrapper>,
    #[serde(rename = "APPID")]
    pub appid: Option<SmallString<32>>,
}

impl ApiOptions {
    #[must_use]
    pub fn get_weather_api<'a>(&self, api: &'a WeatherApi) -> Cow<'a, WeatherApi> {
        if let Some(appid) = &self.appid {
            Cow::Owned(api.clone().with_key(appid))
        } else {
            Cow::Borrowed(api)
        }
    }

    /// # Errors
    /// Returns error if unable to determine location
    pub fn get_weather_location(&self, config: &Config) -> Result<WeatherLocation, Error> {
        let loc = if let Some(zipcode) = self.zip {
            if let Some(country_code) = &self.country_code {
                WeatherLocation::from_zipcode_country_code(zipcode, (*country_code).into())
            } else {
                WeatherLocation::from_zipcode(zipcode)
            }
        } else if let Some(city_name) = &self.q {
            WeatherLocation::from_city_name(city_name)
        } else if self.lat.is_some() && self.lon.is_some() {
            if let Some(lat) = self.lat {
                if let Some(lon) = self.lon {
                    WeatherLocation::from_lat_lon(lat.into(), lon.into())
                } else {
                    unreachable!()
                }
            } else {
                unreachable!()
            }
        } else if let Some(zipcode) = config.zipcode {
            if let Some(country_code) = &config.country_code {
                WeatherLocation::from_zipcode_country_code(zipcode, *country_code)
            } else {
                WeatherLocation::from_zipcode(zipcode)
            }
        } else if let Some(city_name) = &config.city_name {
            WeatherLocation::from_city_name(city_name)
        } else if config.lat.is_some() && config.lon.is_some() {
            if let Some(lat) = config.lat {
                if let Some(lon) = config.lon {
                    WeatherLocation::from_lat_lon(lat, lon)
                } else {
                    unreachable!()
                }
            } else {
                unreachable!()
            }
        } else {
            return Err(Error::BadRequest(
                "\n\nERROR: You must specify at least one option".into(),
            ));
        };
        Ok(loc)
    }
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use weather_util_rust::weather_api::{WeatherApi, WeatherLocation};

    use crate::{api_options::ApiOptions, config::Config};

    #[test]
    fn test_api_options() -> Result<(), Error> {
        let api = WeatherApi::default();
        let opt: ApiOptions = serde_json::from_str(r#"{"zip":55427}"#)?;
        let api2 = opt.get_weather_api(&api);
        assert_eq!(api, *api2);

        let config = Config::default();

        let loc = opt.get_weather_location(&config)?;
        if let WeatherLocation::ZipCode { zipcode, .. } = loc {
            assert_eq!(zipcode, 55427);
        } else {
            assert!(false);
        }
        Ok(())
    }
}
