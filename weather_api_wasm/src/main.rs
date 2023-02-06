#![allow(clippy::unused_peekable)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use dioxus::prelude::{use_future, use_state, Element, Scope};
use log::debug;
use url::Url;
use web_sys::window;

use weather_api_common::weather_element::{
    get_parameters, index_element, DEFAULT_LOCATION, DEFAULT_URL,
};

#[cfg(target_arch = "wasm32")]
use weather_api_common::wasm_utils::{
    get_history, get_ip_address, get_location_from_ip, set_history,
};

fn main() {
    // init debug tool for WebAssembly
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();

    dioxus_web::launch(index_component);
}

pub fn index_component(cx: Scope) -> Element {
    let (url_path, set_url_path) = use_state(cx, || "weather/plot.html").split();
    let (draft, set_draft) = use_state(cx, String::new).split();
    let (current_loc, set_current_loc) = use_state(cx, || None).split();
    let (search_history, set_search_history) = use_state(cx, || {
        #[cfg(not(target_arch = "wasm32"))]
        let history = vec![String::from("zip=10001")];

        #[cfg(target_arch = "wasm32")]
        let history = get_history().unwrap_or_else(|_| vec![String::from("zip=10001")]);

        history
    })
    .split();
    let (ip_location, set_ip_location) = use_state(cx, || get_parameters(DEFAULT_LOCATION)).split();
    let (location, set_location) = use_state(cx, || get_parameters(DEFAULT_LOCATION)).split();

    let mut origin = DEFAULT_URL.to_string();
    let mut url: Option<Url> = None;
    let mut height = 100.0;
    let mut width = 100.0;
    if let Some(window) = window() {
        if let Ok(o) = window.location().origin() {
            origin = o;
        }
        if let Ok(href) = window.location().href() {
            if let Ok(u) = href.parse() {
                url.replace(u);
            }
        }
        if let Some(h) = window.inner_height().ok().and_then(|s| s.as_f64()) {
            height = h;
        }
        if let Some(w) = window.inner_width().ok().and_then(|s| s.as_f64()) {
            width = w;
        }
    }

    let height = (height * 750. / 856.).abs() as u64;
    let width = (width * 850. / 1105.).abs() as u64;

    if url.is_some() && current_loc.is_none() {
        if let Some((_, s)) = url.as_ref().and_then(|u| u.query_pairs().next()) {
            debug!("href {s}");
            let s = s.to_string();
            let loc = get_parameters(&s);
            set_current_loc.set(Some(s.to_string()));
            set_current_loc.needs_update();
            if !search_history.contains(&s) {
                set_search_history.modify(|sh| {
                    let mut v: Vec<String> =
                        sh.iter().filter(|x| x.as_str() != s).cloned().collect();
                    v.push(s.to_string());

                    #[cfg(target_arch = "wasm32")]
                    set_history(&v).expect("Failed to set history");

                    v
                });
                set_search_history.needs_update();
                set_location.modify(|_| loc);
                set_location.needs_update();
            }
        }
    }

    let location_future = use_future(cx, (), |_| async move {
        #[cfg(target_arch = "wasm32")]
        if let Ok(ip) = get_ip_address().await {
            debug!("ip {ip}");
            if let Ok(loc) = get_location_from_ip(ip).await {
                debug!("get location {loc:?}");
                return Some(loc);
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
        set_location,
        ip_location,
        set_ip_location,
        search_history,
        set_search_history,
        location_future,
        set_current_loc,
    ))
}
