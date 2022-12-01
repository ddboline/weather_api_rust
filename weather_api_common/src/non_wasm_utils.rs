use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use url::Url;

use weather_util_rust::{latitude::Latitude, longitude::Longitude, weather_api::WeatherLocation};

pub async fn get_ip_address() -> Result<Ipv4Addr, Error> {
    let url: Url = "https://ipinfo.io/ip".parse()?;
    let text = reqwest::get(url).await?.text().await?;
    text.trim().parse().map_err(Into::into)
}

pub async fn get_location_from_ip(ip: Ipv4Addr) -> Result<WeatherLocation, Error> {
    #[derive(Default, Serialize, Deserialize)]
    struct Location {
        latitude: Latitude,
        longitude: Longitude,
    }

    let ipaddr = ip.to_string();
    let url = Url::parse("https://ipwhois.app/json/")?.join(&ipaddr)?;
    let location: Location = reqwest::get(url).await?.json().await?;
    Ok(WeatherLocation::from_lat_lon(
        location.latitude,
        location.longitude,
    ))
}
