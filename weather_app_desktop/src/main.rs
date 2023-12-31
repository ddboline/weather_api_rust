#![allow(clippy::used_underscore_binding)]
#![allow(clippy::too_many_lines)]

use anyhow::{format_err, Error};
use futures_channel::mpsc::unbounded;
use futures_util::{lock::Mutex, stream::StreamExt, SinkExt};
use log::debug;
use std::sync::Arc;

use weather_api_common::{
    weather_element::{AppProps, WeatherAppComponent},
    WeatherEntry,
};
use weather_util_rust::{
    config::Config,
    weather_api::{WeatherApi, WeatherLocation},
};

fn main() -> Result<(), Error> {
    env_logger::init();
    let (send_loc, mut recv_loc) = unbounded::<WeatherLocation>();
    let (mut send_result, recv_result) = unbounded::<(WeatherLocation, WeatherEntry)>();
    let config = Config::init_config(None)?;
    let api_key = config
        .api_key
        .as_ref()
        .ok_or_else(|| format_err!("No api key given"))?;
    let api = WeatherApi::new(
        api_key.as_str(),
        &config.api_endpoint,
        &config.api_path,
        &config.geo_path,
    );
    let handle: std::thread::JoinHandle<Result<(), Error>> = std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(async move {
                while let Some(loc) = recv_loc.next().await {
                    debug!("get loc {loc:?}");
                    let weather = api.get_weather_data(&loc).await.ok();
                    let forecast = api.get_weather_forecast(&loc).await.ok();
                    let entry = WeatherEntry { weather, forecast };
                    send_result.send((loc, entry)).await.unwrap();
                }
            });
        Ok(())
    });

    dioxus_desktop::launch_with_props(
        WeatherAppComponent,
        AppProps {
            send: Arc::new(Mutex::new(send_loc)),
            recv: Arc::new(Mutex::new(recv_result)),
        },
        dioxus_desktop::Config::default(),
    );
    handle.join().unwrap()?;
    Ok(())
}
