#[cfg(target_arch = "wasm32")]
use weather_api_common::weather_element::index_component;

fn main() {
    // init debug tool for WebAssembly
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();

    #[cfg(target_arch = "wasm32")]
    dioxus::web::launch(index_component);
}
