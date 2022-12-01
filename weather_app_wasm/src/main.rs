use std::sync::Arc;
use futures_util::lock::Mutex;
use futures_channel::mpsc::unbounded;

use weather_util_rust::weather_api::WeatherLocation;

use weather_api_common::weather_element::weather_app_component;
use weather_api_common::weather_element::AppProps;
use weather_api_common::WeatherEntry;

fn main() {
    let (send_loc, _) = unbounded::<WeatherLocation>();
    let (_, recv_result) = unbounded::<(WeatherLocation, WeatherEntry)>();

    wasm_logger::init(wasm_logger::Config::default());
    dioxus::web::launch_with_props(
        weather_app_component,
        AppProps {
            send: Arc::new(Mutex::new(send_loc)),
            recv: Arc::new(Mutex::new(recv_result)),
        },
        |c| c
    );
}