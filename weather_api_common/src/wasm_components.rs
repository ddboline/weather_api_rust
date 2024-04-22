use dioxus::prelude::{
    component, use_resource, use_signal, Element, Readable, Resource, UseResourceState, Writable,
};
use std::collections::{HashMap, HashSet};
use time::{Date, Duration, Month, PrimitiveDateTime, Time};

use js_sys::Date as JsDate;
use web_sys::window;

use weather_util_rust::weather_api::WeatherLocation;

use crate::{
    get_parameters, LocationCount, WeatherEntry, WeatherPage, DEFAULT_LOCATION, DEFAULT_URL,
};

use crate::{
    wasm_utils::{
        get_history, get_ip_address, get_location_from_ip, get_locations, get_weather_data_forecast,
    },
    weather_element::index_element,
};

const DEFAULT_HISTORY_DAYS: i64 = 7;

#[component]
pub fn IndexComponent() -> Element {
    let default_cache: HashMap<WeatherLocation, WeatherEntry> = HashMap::new();

    let page_type = use_signal(|| WeatherPage::Index);
    let draft = use_signal(String::new);
    let search_history = use_signal(|| {
        let history = get_history().unwrap_or_else(|_| vec![String::from("zip=10001")]);
        history
    });
    let mut ip_location = use_signal(|| get_parameters(DEFAULT_LOCATION));
    let location = use_signal(|| get_parameters(DEFAULT_LOCATION));
    let history_location = use_signal(|| String::from("11106"));
    let mut history_location_cache = use_signal(|| HashSet::new());
    let start_date = use_signal(|| {
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
    });
    let end_date = use_signal(|| {
        let js_date = JsDate::new_0();
        let month: Month = (js_date.get_utc_month() as u8 + 1).try_into().ok()?;
        let date = Date::from_calendar_date(
            js_date.get_utc_full_year() as i32,
            month,
            js_date.get_utc_date() as u8,
        )
        .ok()?;
        Some(date)
    });
    let mut cache = use_signal(|| default_cache);
    let mut weather = use_signal(|| None);
    let mut forecast = use_signal(|| None);

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

    let mut location_future: Resource<Option<WeatherLocation>> = use_resource(|| async move {
        if let Ok(ip) = get_ip_address().await {
            if let Ok(loc) = get_location_from_ip(ip).await {
                return Some(loc);
            }
        }
        None
    });

    let mut history_location_future: Resource<Option<Vec<LocationCount>>> =
        use_resource(|| async move {
            if let Ok(locations) = get_locations().await {
                return Some(locations);
            }
            None
        });

    let weather_future = use_resource(move || {
        let l = location();
        let entry_opt = cache().get(&l).cloned();
        async move {
            if let Some(entry) = entry_opt {
                (l, entry)
            } else {
                let entry = get_weather_data_forecast(&l).await;
                (l, entry)
            }
        }
    });

    {
        let mut is_complete = false;
        if *location_future.state().read() == UseResourceState::Ready {
            let result = (*location_future.read()).clone();
            if let Some(Some(loc)) = result {
                if loc != *ip_location.read() {
                    ip_location.set(loc.clone());
                    is_complete = true;
                }
            }
        }
        if is_complete {
            location_future.cancel();
        }

        let result = (*weather_future.read()).clone();
        if let Some((loc, entry)) = result {
            if !cache.read().contains_key(&loc) || cache.read().is_empty() {
                let mut new_cache = (*cache.read()).clone();
                cache.set({
                    let l = (*location.read()).clone();
                    new_cache.insert(l.clone(), entry.clone());
                    if let Some(we) = new_cache.get(&l) {
                        if let Some(w) = &we.weather {
                            weather.set(Some(w.clone()));
                        }
                        if let Some(f) = &we.forecast {
                            forecast.set(Some(f.clone()));
                        }
                    }
                    new_cache
                });
            }
        }

        is_complete = false;
        let result = (*history_location_future.read()).clone();
        if let Some(Some(locations)) = result {
            if history_location_cache.read().is_empty() {
                let cache: HashSet<String> = locations
                    .iter()
                    .filter_map(|lc| {
                        if lc.count > 100 {
                            Some(lc.location.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                history_location_cache.set(cache);
            }
            is_complete = true;
        }
        if is_complete {
            history_location_future.cancel();
        }

        index_element(
            height,
            width,
            origin,
            page_type,
            draft,
            location,
            ip_location,
            search_history,
            history_location,
            history_location_cache,
            location_future,
            weather,
            forecast,
            start_date,
            end_date,
        )
    }
}
