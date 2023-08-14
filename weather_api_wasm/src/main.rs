#![allow(clippy::unused_peekable)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]

use dioxus::prelude::{use_future, use_state, Element, Scope};
use log::debug;
use url::Url;
use web_sys::window;

const DEFAULT_HISTORY_DAYS: usize = 7;

#[cfg(target_arch = "wasm32")]
use js_sys::Date as JsDate;

#[cfg(target_arch = "wasm32")]
use time::{Date, Duration, Month, PrimitiveDateTime, Time};

use weather_api_common::weather_element::{
    get_parameters, index_element, WeatherPage, DEFAULT_LOCATION, DEFAULT_URL,
};

#[cfg(target_arch = "wasm32")]
use weather_api_common::wasm_utils::{
    get_history, get_ip_address, get_location_from_ip, get_locations, set_history,
};

fn main() {
    // init debug tool for WebAssembly
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();

    dioxus_web::launch(index_component);
}

pub fn index_component(cx: Scope) -> Element {
    let (url_path, set_url_path) = use_state(cx, || WeatherPage::Index).split();
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
    let (history_location, set_history_location) =
        use_state(cx, || String::from("Astoria")).split();
    let (start_date, set_start_date) = use_state(cx, || {
        #[cfg(target_arch = "wasm32")]
        {
            let js_date = JsDate::new_0();
            let month: Month = (js_date.get_utc_month() as u8 + 1).try_into().ok()?;
            let date = Date::from_calendar_date(
                js_date.get_utc_full_year() as i32,
                month,
                js_date.get_utc_date() as u8,
            )
            .ok()?;
            let time = Time::from_hms(
                js_date.get_utc_hours() as u8,
                js_date.get_utc_minutes() as u8,
                js_date.get_utc_minutes() as u8,
            )
            .ok()?;
            let date = PrimitiveDateTime::new(date, time).assume_utc();
            return Some((date - Duration::days(DEFAULT_HISTORY_DAYS)).date());
        }
        #[cfg(not(target_arch = "wasm32"))]
        None
    })
    .split();
    let (end_date, set_end_date) = use_state(cx, || {
        #[cfg(target_arch = "wasm32")]
        {
            let js_date = JsDate::new_0();
            let month: Month = (js_date.get_utc_month() as u8 + 1).try_into().ok()?;
            let date = Date::from_calendar_date(
                js_date.get_utc_full_year() as i32,
                month,
                js_date.get_utc_date() as u8,
            )
            .ok()?;
            return Some(date);
        }
        #[cfg(not(target_arch = "wasm32"))]
        None
    })
    .split();

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
            if let Ok(loc) = get_location_from_ip(ip).await {
                return Some(loc);
            }
        }
        None
    });

    let history_locaton_future = use_future(cx, (), |_| async move {
        #[cfg(target_arch = "wasm32")]
        if let Ok(locations) = get_locations().await {
            return Some(locations);
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
        history_location,
        set_history_location,
        history_locaton_future,
        set_current_loc,
        start_date,
        set_start_date,
        end_date,
        set_end_date,
    ))
}
