use rweb::Schema;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
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
    pub appid: Option<StackString>,
}

impl ApiOptions {
    pub fn get_weather_api<'a>(&self, api: &'a WeatherApi) -> Result<Cow<'a, WeatherApi>, Error> {
        if let Some(appid) = &self.appid {
            Ok(Cow::Owned(api.clone().with_key(&appid)))
        } else {
            Ok(Cow::Borrowed(api))
        }
    }

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
            let lat = self.lat.unwrap().into();
            let lon = self.lon.unwrap().into();
            WeatherLocation::from_lat_lon(lat, lon)
        } else if let Some(zipcode) = config.zipcode {
            if let Some(country_code) = &config.country_code {
                WeatherLocation::from_zipcode_country_code(zipcode, *country_code)
            } else {
                WeatherLocation::from_zipcode(zipcode)
            }
        } else if let Some(city_name) = &config.city_name {
            WeatherLocation::from_city_name(city_name)
        } else if config.lat.is_some() && config.lon.is_some() {
            let lat = config.lat.unwrap();
            let lon = config.lon.unwrap();
            WeatherLocation::from_lat_lon(lat, lon)
        } else {
            return Err(Error::BadRequest(
                "\n\nERROR: You must specify at least one option".into(),
            ));
        };
        Ok(loc)
    }
}
