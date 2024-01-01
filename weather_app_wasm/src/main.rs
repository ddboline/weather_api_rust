use weather_api_common::weather_element::WeatherAppComponent;
use weather_api_common::weather_element::AppProps;

#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    dioxus_web::launch_with_props(
        WeatherAppComponent,
        AppProps,
        dioxus_web::Config::default()
    );
}