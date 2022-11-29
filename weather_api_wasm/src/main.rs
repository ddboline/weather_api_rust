#![allow(clippy::unused_variables)]
#![allow(unused_variables)]

use dioxus::prelude::{use_future, use_state, Element, Scope};
use fermi::{use_read, use_set};
use log::debug;
use url::Url;
use web_sys::window;

use weather_api_common::weather_element::{
    get_parameters, index_element, DEFAULT_LOCATION, DEFAULT_URL, LOCATION,
};

use weather_api_common::wasm_utils::{
    get_history, get_ip_address, get_location_from_ip, set_history,
};

fn main() {
    // init debug tool for WebAssembly
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();

    dioxus::web::launch(index_component);
}

pub fn index_component(cx: Scope) -> Element {
    let (url_path, set_url_path) = use_state(&cx, || "weather/plot.html").split();
    let (draft, set_draft) = use_state(&cx, String::new).split();
    let (current_loc, set_current_loc) = use_state(&cx, || None).split();
    let (search_history, set_search_history) = use_state(&cx, || {
        let history = vec![String::from("zip=10001")];

        let history = get_history().unwrap_or_else(|_| vec![String::from("zip=10001")]);

        history
    })
    .split();
    let (ip_location, set_ip_location) =
        use_state(&cx, || get_parameters(DEFAULT_LOCATION)).split();

    let location = use_read(&cx, LOCATION);
    let set_location = use_set(&cx, LOCATION);

    let window = window().unwrap();
    let origin = window
        .location()
        .origin()
        .unwrap_or_else(|_| DEFAULT_URL.to_string());
    let href = window.location().href().unwrap();
    let url: Option<Url> = href.parse().ok();

    let height = window
        .inner_height()
        .ok()
        .and_then(|s| s.as_f64())
        .unwrap_or(100.0);
    let width = window
        .inner_width()
        .ok()
        .and_then(|s| s.as_f64())
        .unwrap_or(100.0);

    let height = (height * 750. / 856.) as u64;
    let width = (width * 850. / 1105.) as u64;

    if url.is_some() && current_loc.is_none() {
        if let Some((_, s)) = url.as_ref().and_then(|u| u.query_pairs().next()) {
            debug!("href {s}");
            let s = s.to_string();
            let loc = get_parameters(&s);
            set_current_loc.set(Some(s.to_string()));
            set_current_loc.needs_update();
            if !search_history.contains(&s.to_string()) {
                set_search_history.modify(|sh| {
                    let mut v: Vec<String> =
                        sh.iter().filter(|x| x.as_str() != &s).cloned().collect();
                    v.push(s.to_string());

                    set_history(&v).expect("Failed to set history");

                    v
                });
                set_search_history.needs_update();
                set_location(loc);
            }
        }
    }

    let location_future = use_future(&cx, (), |_| async move {
        if let Ok(ip) = get_ip_address().await {
            debug!("ip {ip}");
            if let Ok(location) = get_location_from_ip(ip).await {
                debug!("get location {location:?}");
                return Some(location);
            }
        }
        None
    });

    cx.render(index_element(
        height,
        width,
        origin,
        url_path,
        set_url_path,
        draft,
        set_draft,
        location,
        set_location.as_ref(),
        ip_location,
        set_ip_location,
        search_history,
        set_search_history,
        location_future,
        set_current_loc,
    ))
}
