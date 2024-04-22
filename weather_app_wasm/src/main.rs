use dioxus::dioxus_core::VirtualDom;
use weather_api_common::weather_element::{AppProps, WeatherAppComponent};

#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    let app = VirtualDom::new_with_props(WeatherAppComponent, AppProps);
    dioxus_web::launch::launch_virtual_dom(app, dioxus_web::Config::default());
}
