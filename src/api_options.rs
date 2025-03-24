use serde::{Deserialize, Serialize};
use stack_string::{SmallString, StackString};
use std::borrow::Cow;
use utoipa::ToSchema;

use weather_util_rust::weather_api::{WeatherApi, WeatherLocation};

use crate::{
    config::Config, country_code_wrapper::CountryCodeWrapper, errors::ServiceError as Error,
    latitude_wrapper::LatitudeWrapper, longitude_wrapper::LongitudeWrapper,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ApiOptions {
    pub zip: Option<u64>,
    pub country_code: Option<CountryCodeWrapper>,
    pub q: Option<StackString>,
    pub lat: Option<LatitudeWrapper>,
    pub lon: Option<LongitudeWrapper>,
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
    use log::info;
    use std::{
        convert::TryInto,
        env::{remove_var, set_var},
        path::Path,
    };
    use weather_util_rust::{
        latitude::Latitude,
        longitude::Longitude,
        weather_api::{WeatherApi, WeatherLocation},
    };

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

        let opt: ApiOptions = serde_json::from_str(r#"{"appid":"TEST"}"#)?;

        unsafe {
            set_var("ZIPCODE", "49934");
        }

        let config = Config::init_config(None)?;

        let loc = opt.get_weather_location(&config)?;
        if let WeatherLocation::ZipCode { zipcode, .. } = loc {
            assert_eq!(zipcode, 49934);
        } else {
            assert!(false);
        }

        unsafe {
            remove_var("ZIPCODE");
            set_var("CITY_NAME", "TEST CITY");
        }

        let opt: ApiOptions = serde_json::from_str(r#"{"appid":"TEST"}"#)?;

        let conf_path = Path::new("tests/config.env");
        let config = Config::init_config(Some(conf_path))?;

        let loc = opt.get_weather_location(&config)?;
        info!("{loc:?}");
        if let WeatherLocation::CityName(name) = loc {
            assert_eq!(&name, "TEST CITY");
        } else {
            assert!(false);
        }

        unsafe {
            remove_var("CITY_NAME");
            set_var("LAT", "40.7518359");
            set_var("LON", "-74.0529922");
        }

        let opt: ApiOptions = serde_json::from_str(r#"{"appid":"TEST"}"#)?;

        let conf_path = Path::new("tests/config.env");
        let config = Config::init_config(Some(conf_path))?;

        let loc = opt.get_weather_location(&config)?;
        info!("{loc:?}");
        if let WeatherLocation::LatLon {
            latitude,
            longitude,
        } = loc
        {
            let lat: Latitude = 40.7518359f64.try_into()?;
            let lon: Longitude = (-74.0529922f64).try_into()?;
            info!("lat {lat} lon {lon}");
            assert_eq!(latitude, lat);
            assert_eq!(longitude, lon);
        } else {
            assert!(false);
        }

        Ok(())
    }
}
