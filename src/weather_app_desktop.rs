#![allow(clippy::used_underscore_binding)]
#![allow(clippy::too_many_lines)]

use anyhow::{Error, format_err};
use dioxus::dioxus_core::VirtualDom;
use futures_channel::mpsc::unbounded;
use futures_util::{SinkExt, lock::Mutex, stream::StreamExt};
use log::debug;
use std::sync::Arc;

use weather_api_common::{
    WeatherEntry,
    weather_element::{AppProps, WeatherAppComponent},
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
    let _handle: std::thread::JoinHandle<Result<(), Error>> = std::thread::spawn(move || {
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

    let weather_app = VirtualDom::new_with_props(
        WeatherAppComponent,
        AppProps {
            send: Arc::new(Mutex::new(send_loc)),
            recv: Arc::new(Mutex::new(recv_result)),
        },
    );
    dioxus_desktop::launch::launch_virtual_dom(weather_app, dioxus_desktop::Config::default())
}
