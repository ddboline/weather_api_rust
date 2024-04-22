#![allow(clippy::unused_peekable)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]

use dioxus::dioxus_core::VirtualDom;
use weather_api_common::wasm_components::IndexComponent;

fn main() {
    // init debug tool for WebAssembly
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();

    let app = VirtualDom::new(IndexComponent);
    dioxus_web::launch::launch_virtual_dom(app, dioxus_web::Config::default());
}
