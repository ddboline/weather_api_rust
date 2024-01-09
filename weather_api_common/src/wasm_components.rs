use dioxus::prelude::{
    component, use_future, use_state, Element,
    Scope, UseFuture, UseFutureState,
};
use log::debug;
use std::collections::HashMap;
use time::{Date, Duration, Month, PrimitiveDateTime, Time};

use js_sys::Date as JsDate;
use web_sys::window;

use weather_util_rust::weather_api::WeatherLocation;

use crate::{
    get_parameters, LocationCount, WeatherEntry, WeatherPage, DEFAULT_LOCATION,
    DEFAULT_URL,
};

use crate::{
    wasm_utils::{
        get_history, get_ip_address, get_location_from_ip, get_locations,
        get_weather_data_forecast,
    },
    weather_element::index_element,
};

const DEFAULT_HISTORY_DAYS: i64 = 7;

#[component]
pub fn IndexComponent(cx: Scope) -> Element {
    let default_cache: HashMap<WeatherLocation, WeatherEntry> = HashMap::new();

    let (page_type, set_page_type) = use_state(cx, || WeatherPage::Index).split();
    let (draft, set_draft) = use_state(cx, String::new).split();
    let (search_history, set_search_history) = use_state(cx, || {
        let history = get_history().unwrap_or_else(|_| vec![String::from("zip=10001")]);

        history
    })
    .split();
    let (ip_location, set_ip_location) = use_state(cx, || get_parameters(DEFAULT_LOCATION)).split();
    let (location, set_location) = use_state(cx, || get_parameters(DEFAULT_LOCATION)).split();
    let (history_location, set_history_location) =
        use_state(cx, || String::from("Astoria")).split();
    let (history_location_cache, set_history_location_cache) = use_state(cx, || Vec::new()).split();
    let (start_date, set_start_date) = use_state(cx, || {
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
        Some((date - Duration::days(DEFAULT_HISTORY_DAYS)).date())
    })
    .split();
    let (end_date, set_end_date) = use_state(cx, || {
        let js_date = JsDate::new_0();
        let month: Month = (js_date.get_utc_month() as u8 + 1).try_into().ok()?;
        let date = Date::from_calendar_date(
            js_date.get_utc_full_year() as i32,
            month,
            js_date.get_utc_date() as u8,
        )
        .ok()?;
        Some(date)
    })
    .split();
    let (cache, set_cache) = use_state(cx, || default_cache).split();
    let (weather, set_weather) = use_state(cx, || None).split();
    let (forecast, set_forecast) = use_state(cx, || None).split();

    let mut origin: String = DEFAULT_URL.to_string();
    let mut height = 100.0f64;
    let mut width = 100.0f64;

    if let Some(window) = window() {
        if let Ok(o) = window.location().origin() {
            origin = o;
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

    let location_future: &UseFuture<Option<WeatherLocation>> =
        use_future(cx, (), |()| async move {
            if let Ok(ip) = get_ip_address().await {
                if let Ok(loc) = get_location_from_ip(ip).await {
                    return Some(loc);
                }
            }
            None
        });

    let history_location_future: &UseFuture<Option<Vec<LocationCount>>> =
        use_future(cx, (), |()| async move {
            if let Ok(locations) = get_locations().await {
                return Some(locations);
            }
            None
        });

    let weather_future = use_future(cx, location, |l| {
        let entry_opt = cache.get(&l).cloned();
        async move {
            if let Some(entry) = entry_opt {
                (l, entry)
            } else {
                let entry = get_weather_data_forecast(&l).await;
                (l, entry)
            }
        }
    });

    cx.render({
        let mut is_complete = false;
        if let UseFutureState::Complete(Some(loc)) = location_future.state() {
            debug!("enter location future");
            if loc != ip_location {
                set_ip_location.set(loc.clone());
                set_ip_location.needs_update();
                is_complete = true;
            }
        }
        if is_complete {
            location_future.set(None);
        }

        if let UseFutureState::Complete((loc, entry)) = weather_future.state() {
            debug!("enter future");
            if !cache.contains_key(loc) || cache.is_empty() {
                set_cache.modify(|c| {
                    let mut new_cache = c.clone();
                    new_cache.insert(location.clone(), entry.clone());
                    if let Some(WeatherEntry { weather, forecast }) = new_cache.get(location) {
                        if let Some(weather) = weather {
                            set_weather.modify(|_| Some(weather.clone()));
                            set_weather.needs_update();
                        }
                        if let Some(forecast) = forecast {
                            set_forecast.modify(|_| Some(forecast.clone()));
                            set_forecast.needs_update();
                        }
                    }
                    new_cache
                });
                set_cache.needs_update();
            }
        }

        is_complete = false;
        if let UseFutureState::Complete(Some(locations)) = history_location_future.state() {
            debug!("enter history location future");
            if history_location_cache.is_empty() {
                set_history_location_cache.set(locations.clone());
                set_history_location_cache.needs_update();
            }
            is_complete = true;
        }
        if is_complete {
            history_location_future.set(None);
        }

        debug!("{location:?}");

        index_element(
            height,
            width,
            origin,
            page_type,
            set_page_type,
            draft,
            set_draft,
            location,
            set_location,
            ip_location,
            search_history,
            set_search_history,
            history_location,
            set_history_location,
            history_location_cache,
            location_future,
            weather,
            forecast,
            start_date,
            set_start_date,
            end_date,
            set_end_date,
        )
    })
}
