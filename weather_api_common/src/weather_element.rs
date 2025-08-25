use dioxus::prelude::{
    Element, GlobalSignal, IntoDynNode, Key, Props, Readable, Resource, Signal, Writable,
    component, dioxus_elements, rsx, use_resource, use_signal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};
use time::{
    Date, OffsetDateTime, UtcOffset, format_description::FormatItem, macros::format_description,
};
use url::Url;

#[cfg(not(target_arch = "wasm32"))]
use futures_util::{StreamExt, sink::SinkExt};

#[cfg(not(target_arch = "wasm32"))]
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};

#[cfg(not(target_arch = "wasm32"))]
use futures_util::lock::Mutex;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use crate::wasm_utils::{
    get_ip_address, get_location_from_ip, get_weather_data_forecast, set_history,
};

use weather_util_rust::{
    format_string, weather_api::WeatherLocation, weather_data::WeatherData,
    weather_forecast::WeatherForecast,
};

use crate::{DEFAULT_LOCATION, DEFAULT_STR, WeatherEntry, WeatherPage, get_parameters};

#[cfg(debug_assertions)]
use crate::DEFAULT_HOST;

#[cfg(debug_assertions)]
static BASE_HOST: Option<&str> = Some(DEFAULT_HOST);

#[cfg(not(debug_assertions))]
static BASE_HOST: Option<&str> = None;

static DATE_FORMAT: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");

#[derive(PartialEq, Deserialize, Serialize, Debug, Clone, Copy)]
pub struct PlotPoint {
    pub datetime: OffsetDateTime,
    pub value: f64,
}
#[derive(PartialEq, Deserialize, Serialize, Debug, Clone)]
pub struct PlotData {
    pub plot_url: String,
    pub title: String,
    pub xaxis: String,
    pub yaxis: String,
}

fn update_search_history(sh: &Vec<String>, s: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::with_capacity(sh.len());
    v.push(s.into());
    for x in sh {
        if x.as_str() == v[0] {
            continue;
        }
        v.push(x.clone())
    }

    #[cfg(target_arch = "wasm32")]
    set_history(&v).expect("Failed to set history");

    v
}

#[component]
pub fn WeatherComponent(weather: WeatherData, forecast: WeatherForecast) -> Element {
    weather_element(&weather, &forecast)
}

pub fn weather_element(weather: &WeatherData, forecast: &WeatherForecast) -> Element {
    let weather_data = weather.get_current_conditions();
    let weather_lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let weather_cols = weather_lines.iter().map(|x| x.len()).max().unwrap_or(0) + 2;
    let weather_rows = weather_lines.len() + 2;
    let weather_lines = weather_lines.join("\n");

    let name = &weather.name;
    let lat = weather.coord.lat;
    let lon = weather.coord.lon;
    let mut title = format_string!("{name}");
    if let Some(country) = &weather.sys.country {
        write!(&mut title, " {country}").unwrap();
    }
    write!(&mut title, " {lat:0.5}N {lon:0.5}E").unwrap();
    let url = format_string!("https://www.google.com/maps?ll={lat},{lon}&q={lat},{lon}");

    let location_element = rsx! {
        div {
            style: "text-anchor: middle; font-size: 16px;",
            a {
                href: "{url}",
                target: "_blank",
                "{title}",
            }
        }
    };

    let weather_element = rsx! {
        textarea {
            readonly: "true",
            rows: "{weather_rows}",
            cols: "{weather_cols}",
            "{weather_lines}"
        },
    };

    let forecast_element = {
        let weather_forecast = forecast.get_forecast();
        let forecast_lines: Vec<_> = weather_forecast.iter().map(|s| s.trim_end()).collect();
        let forecast_cols = forecast_lines.iter().map(|x| x.len()).max().unwrap_or(0) + 10;
        let forecast_rows = forecast_lines.len() + 2;
        let forecast_lines = forecast_lines.join("\n");

        rsx! {
            textarea {
                readonly: "true",
                rows: "{forecast_rows}",
                cols: "{forecast_cols}",
                "{forecast_lines}"
            }
        }
    };

    rsx! {
        head {
            title: "Weather Plots",
            style {
                {include_str!("../../templates/style.css")}
            }
        },
        body {
            {location_element},
            div {
                {weather_element},
                {forecast_element},
            },
        }
    }
}

#[component]
pub fn ForecastComponent(weather: WeatherData, plots: Vec<PlotData>) -> Element {
    let name = &weather.name;
    let lat = weather.coord.lat;
    let lon = weather.coord.lon;
    let mut title = format_string!("{name}");
    if let Some(country) = &weather.sys.country {
        write!(&mut title, " {country}").unwrap();
    }
    write!(&mut title, " {lat:0.5}N {lon:0.5}E").unwrap();
    let url = format_string!("https://www.google.com/maps?ll={lat},{lon}&q={lat},{lon}");

    let location_element = rsx! {
        div {
            style: "text-anchor: middle; font-size: 16px;",
            a {
                href: "{url}",
                target: "_blank",
                "{title}",
            }
        }
    };

    rsx! {
        head {
            title: "Weather Plots",
            style {
                {include_str!("../../templates/style.css")}
            }
        },
        body {
            {location_element},
            {plot_element(&plots)},
        }
    }
}

fn plot_element(plots: &[PlotData]) -> Element {
    let timeseries_url = if let Some(base_host) = BASE_HOST {
        format!("https://{base_host}/weather/timeseries.js")
    } else {
        "/weather/timeseries.js".into()
    };
    let mut script_body = String::new();
    writeln!(&mut script_body, "\n async function forecast_plots(){{\n").unwrap();
    for pd in plots {
        let plot_url = &pd.plot_url;
        let title = &pd.title;
        let xaxis = &pd.xaxis;
        let yaxis = &pd.yaxis;
        writeln!(
            &mut script_body,
            "\t await create_plot('{plot_url}', '{title}', '{xaxis}', '{yaxis}');"
        )
        .unwrap();
    }
    script_body.push_str("};\n");
    writeln!(&mut script_body, "forecast_plots();").unwrap();
    rsx! {
        script {
            src: "https://d3js.org/d3.v4.min.js",
        },
        script {
            "src": "{timeseries_url}",
        },
        br {},
        script {
            dangerous_inner_html: "{script_body}",
        }
    }
}

fn weather_app_element(
    mut draft: Signal<String>,
    mut location_cache: Signal<HashMap<String, WeatherLocation>>,
    cache: Signal<HashMap<WeatherLocation, WeatherEntry>>,
    mut location: Signal<WeatherLocation>,
    mut weather: Signal<WeatherData>,
    mut forecast: Signal<WeatherForecast>,
    mut search_history: Signal<Vec<String>>,
) -> Element {
    let country_info_element = country_info(&weather.read());
    let country_data_element = country_data(&weather.read());
    let week_weather_element = week_weather(&forecast.read());

    rsx! {
        link { rel: "stylesheet", href: "https://unpkg.com/tailwindcss@^2.0/dist/tailwind.min.css" },
        div { class: "mx-auto p-4 bg-gray-100 h-screen flex justify-center",
            div { class: "flex items-center justify-center flex-col",
                div {
                    div { class: "inline-flex flex-col justify-center relative text-gray-500",
                        div { class: "relative",
                            input { class: "p-2 pl-8 rounded border border-gray-200 bg-gray-200 focus:bg-white focus:outline-none focus:ring-2 focus:ring-yellow-600 focus:border-transparent",
                                placeholder: "search...",
                                "type": "text",
                                value: "{draft}",
                                oninput: move |evt| {
                                    let msg = (*evt.map(|data| data.value())).to_string();
                                    draft.set(msg.clone());
                                    let lc = location_cache.read().clone();
                                    let new_location = lc.get(&msg).map_or_else(
                                        || {
                                            let l = get_parameters(&msg);
                                            location_cache.set({
                                                let mut lc = location_cache.read().clone();
                                                lc.insert(msg, l.clone());
                                                lc
                                            });
                                            l
                                        }, Clone::clone
                                    );
                                    if let Some(we) = cache.read().get(&new_location) {
                                        if let Some(w) = &we.weather {
                                            weather.set(w.clone());
                                        }
                                        if let Some(f) = &we.forecast {
                                            forecast.set(f.clone());
                                        }
                                        location.set(new_location);
                                    }
                                },
                                onkeydown: move |evt| {
                                    let d = draft.read().clone();
                                    let lc = location_cache.read().clone();
                                    let new_location = lc.get(&d).map_or_else(
                                        || {
                                            let l = get_parameters(&d);
                                            location_cache.set({
                                                let mut lc = location_cache.read().clone();
                                                lc.insert(d, l.clone());
                                                lc
                                            });
                                            l
                                        }, Clone::clone
                                    );
                                    if let Some(we) = cache.read().get(&new_location) {
                                        if let Some(w) = &we.weather {
                                            weather.set(w.clone());
                                        }
                                        if let Some(f) = &we.forecast {
                                            forecast.set(f.clone());
                                        }
                                    }
                                    let key = evt.map(|data| data.key()).data();
                                    if *key == Key::Enter {
                                        let d = draft.read().clone();
                                        let sh = search_history.read().clone();
                                        draft.set(String::new());
                                        search_history.set({
                                            let mut v: Vec<String> = sh.iter().filter(|s| s.as_str() != d).cloned().collect();
                                            v.push(d);
                                            v
                                        });
                                        location.set(new_location);
                                    }
                                },
                            }
                            svg { class: "w-4 h-4 absolute left-2.5 top-3.5",
                                "viewBox": "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                xmlns: "https://www.w3.org/2000/svg",
                                path {
                                    d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z",
                                    "stroke-linejoin": "round",
                                    "stroke-linecap": "round",
                                    "stroke-width": "2",
                                }
                            }
                        }
                    }
                    select { class: "bg-white border border-gray-100 w-full mt-2",
                        id: "history-selector",
                        onchange: move |x| {
                            let s = x.map(|data| data.value()).data().to_string();
                            let lc = location_cache.read().clone();
                            let new_location = lc.get(&s).map_or_else(|| {
                                let l = get_parameters(&s);
                                location_cache.set({
                                    let mut lc = lc.clone();
                                    lc.insert(s.clone(), l.clone());
                                    lc
                                });
                                let sh = search_history.read().clone();
                                search_history.set({
                                    let mut v: Vec<String> = sh.iter().filter(|x| x.as_str() != s).cloned().collect();
                                    v.push(s);
                                    v
                                });
                                l
                            }, Clone::clone);
                            if let Some(we) = cache.read().get(&new_location) {
                                if let Some(w) = &we.weather {
                                    weather.set(w.clone());
                                }
                                if let Some(f) = &we.forecast {
                                    forecast.set(f.clone());
                                }
                            }
                            location.set(new_location);
                        },
                        {
                            let tmp = search_history.read().clone();
                            tmp.into_iter().rev().map(|s| {
                                let selected = get_parameters(&s) == *location.read();
                                rsx! {
                                    option { class: "pl-8 pr-2 py-1 border-b-2 border-gray-100 relative cursor-pointer hover:bg-yellow-50 hover:text-gray-900",
                                        key: "search-history-key-{s}",
                                        value: "{s}",
                                        selected: "{selected}",
                                        "{s}"
                                    }
                                }
                            })
                        }
                    }
                }
                div { class: "flex flex-wrap w-full px-2",
                    div { class: "bg-gray-900 text-white relative min-w-0 break-words rounded-lg overflow-hidden shadow-sm mb-4 w-full bg-white dark:bg-gray-600",
                        div { class: "px-6 py-6 relative",
                            {country_info_element},
                            {country_data_element},
                        }
                        {week_weather_element},
                    }
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct AppProps {
    pub send: Arc<Mutex<UnboundedSender<WeatherLocation>>>,
    pub recv: Arc<Mutex<UnboundedReceiver<(WeatherLocation, WeatherEntry)>>>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct AppProps;

#[component]
#[allow(unused_variables)]
pub fn WeatherAppComponent(props: AppProps) -> Element {
    let default_cache: HashMap<WeatherLocation, WeatherEntry> = HashMap::new();
    let mut default_location_cache: HashMap<String, WeatherLocation> = HashMap::new();
    default_location_cache.insert(DEFAULT_STR.into(), get_parameters(DEFAULT_STR));

    let mut cache = use_signal(|| default_cache);
    let location_cache = use_signal(|| default_location_cache);
    let mut weather = use_signal(WeatherData::default);
    let mut forecast = use_signal(WeatherForecast::default);
    let draft = use_signal(String::new);
    let search_history = use_signal(|| vec![String::from(DEFAULT_STR)]);

    let mut location = use_signal(|| get_parameters(DEFAULT_LOCATION));

    #[cfg(not(target_arch = "wasm32"))]
    let mut recv_future = use_resource(move || {
        let recv = props.recv.clone();
        async move {
            let mut recv = recv.lock().await;
            recv.next().await
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    let _send_future = use_resource(move || {
        let contains_key = cache().contains_key(&location());
        let send = props.send.clone();
        async move {
            if !contains_key {
                let mut send = send.lock().await;
                send.send(location()).await.unwrap();
            }
        }
    });

    #[cfg(target_arch = "wasm32")]
    let location_future = use_resource(|| async move {
        if let Ok(ip) = get_ip_address().await {
            if let Ok(location) = get_location_from_ip(ip).await {
                return Some(location);
            }
        }
        None
    });

    #[cfg(target_arch = "wasm32")]
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = (*recv_future.read()).clone();
            if let Some(Some((loc, entry))) = result
                && ((!cache.read().contains_key(&loc)) || cache.read().is_empty())
            {
                location.set(loc.clone());
                cache.set({
                    let mut new_cache = cache.read().clone();
                    new_cache.insert(location.read().clone(), entry.clone());
                    new_cache
                });
                recv_future.restart();
                if let Some(w) = &entry.weather {
                    weather.set(w.clone());
                }
                if let Some(f) = &entry.forecast {
                    forecast.set(f.clone());
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let result = (*location_future.read()).clone();
            if let Some(Some(loc)) = result {
                if loc != *location.read()
                    && (!cache.read().contains_key(&loc) || cache.read().is_empty())
                {
                    location.set(loc.clone());
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let result = (*weather_future.read()).clone();
            if let Some((loc, entry)) = result {
                if !cache.read().contains_key(&loc) || cache.read().is_empty() {
                    let location = location.read().clone();
                    cache.set({
                        let mut new_cache = cache.read().clone();
                        new_cache.insert(location.clone(), entry.clone());
                        if let Some(we) = new_cache.get(&location) {
                            if let Some(w) = &we.weather {
                                weather.set(w.clone());
                            }
                            if let Some(f) = &we.forecast {
                                forecast.set(f.clone());
                            }
                        }
                        new_cache
                    });
                }
            }
        }

        weather_app_element(
            draft,
            location_cache,
            cache,
            location,
            weather,
            forecast,
            search_history,
        )
    }
}

fn country_data(weather: &WeatherData) -> Element {
    let temp = weather.main.temp.fahrenheit();
    let feels = weather.main.feels_like.fahrenheit();
    let min = weather.main.temp_min.fahrenheit();
    let max = weather.main.temp_max.fahrenheit();

    rsx!(
        div { class: "block sm:flex justify-between items-center flex-wrap",
            div { class: "w-full sm:w-1/2",
                div { class: "flex mb-2 justify-between items-center",
                    span { "Temp" }
                    small { class: "px-2 inline-block", "{temp:0.2}°F" }
                }
            }
            div { class: "w-full sm:w-1/2",
                div { class: "flex mb-2 justify-between items-center",
                    span { "Feels like" }
                    small { class: "px-2 inline-block", "{feels:0.2}°F" }
                }
            }
            div { class: "w-full sm:w-1/2",
                div { class: "flex mb-2 justify-between items-center",
                    span { "Temp min" }
                    small { class: "px-2 inline-block", "{min:0.2}°F" }
                }
            }
            div { class: "w-full sm:w-1/2",
                div { class: "flex mb-2 justify-between items-center",
                    span { "Temp max" }
                    small { class: "px-2 inline-block", "{max:0.2}°F" }
                }
            }
        }
    )
}

fn country_info(weather: &WeatherData) -> Element {
    let name = &weather.name;
    let country = weather.sys.country.as_ref().map_or("", |s| s.as_str());
    let mut main = String::new();
    let mut desc = String::new();
    let mut icon = String::new();
    if let Some(weather) = weather.weather.first() {
        main.push_str(&weather.main);
        desc.push_str(&weather.description);
        icon.push_str(&weather.icon);
    }
    let temp = weather.main.temp.fahrenheit();
    let fo: UtcOffset = weather.timezone.into();
    let date = weather.dt.to_offset(fo);

    rsx!(
        div { class: "flex mb-4 justify-between items-center",
            div {
                h5 { class: "mb-0 font-medium text-xl",
                    "{name} {country}"
                }
                small {
                    img { class: "block w-8 h-8",
                        src: "https://openweathermap.org/img/wn/{icon}@2x.png",
                    }
                }
            }
            div { class: "text-right",
                h6 { class: "mb-0",
                    "{date}"
                }
                h3 { class: "font-bold text-4xl mb-0",
                    span {
                        "{temp:0.1}°F"
                    }
                }
            }
        }
    )
}

fn week_weather(forecast: &WeatherForecast) -> Element {
    let high_low = forecast.get_high_low();
    rsx!(
        div { class: "divider table mx-2 text-center bg-transparent whitespace-nowrap",
            span { class: "inline-block px-3", small { "Forecast" } }
        }
        div { class: "px-6 py-6 relative",
            div { class: "text-center justify-between items-center flex",
                style: "flex-flow: initial;",
                {high_low.iter().map(|(d, (h, l, r, s, i))| {
                    let weekday = d.weekday();
                    let low = l.fahrenheit();
                    let high = h.fahrenheit();
                    let mut rain = String::new();
                    let mut snow = String::new();
                    if r.millimeters() > 0.0 {
                        rain = format!("R {:0.1}\"", r.inches());
                    }
                    if s.millimeters() > 0.0 {
                        snow = format!("S {:0.1}\"", s.inches());
                    }
                    let mut icon = String::new();
                    if let Some(i) = i.iter().next() {
                        icon.push_str(i);
                    }

                    rsx!(div {
                            key: "weather-forecast-key-{d}",
                            class: "text-center mb-0 flex items-center justify-center flex-col",
                            span { class: "block my-1",
                                "{weekday}"
                            }
                            img { class: "block w-8 h-8",
                                src: "https://openweathermap.org/img/wn/{icon}@2x.png",
                            }
                            span { class: "block my-1",
                                "{low:0.1}F°"
                            }
                            span { class: "block my-1",
                                "{high:0.1}F°"
                            }
                            span { class: "block my-1",
                                "{rain}"
                            }
                            span { class: "block my-1",
                                "{snow}"
                            }
                        }
                    )
                })}
            }
        }
    )
}

pub fn index_element(
    height: u64,
    width: u64,
    host: String,
    mut page_type: Signal<WeatherPage>,
    mut draft: Signal<String>,
    mut location: Signal<WeatherLocation>,
    ip_location: Signal<WeatherLocation>,
    mut search_history: Signal<Vec<String>>,
    mut history_location: Signal<String>,
    history_location_cache: Signal<HashSet<String>>,
    mut location_future: Resource<Option<WeatherLocation>>,
    weather: Signal<Option<WeatherData>>,
    forecast: Signal<Option<WeatherForecast>>,
    mut start_date: Signal<Option<Date>>,
    mut end_date: Signal<Option<Date>>,
) -> Element {
    let base_host = BASE_HOST.unwrap_or(&host);
    let url: Url = format!("https://{base_host}/{page_type}")
        .parse()
        .expect("Failed to parse base url");
    let url = match *page_type.read() {
        WeatherPage::Index | WeatherPage::Plot => {
            Url::parse_with_params(url.as_str(), location.read().get_options()).unwrap_or(url)
        }
        WeatherPage::Wasm => url,
        WeatherPage::HistoryPlot => {
            let hl = (*history_location.read()).clone();
            let mut options = vec![("name", &hl)];
            let start_date = (*start_date.read()).map(|d| format!("{d}"));
            let end_date = (*end_date.read()).map(|d| format!("{d}"));
            if let Some(start_date) = &start_date {
                options.push(("start_time", start_date));
            }
            if let Some(end_date) = &end_date {
                options.push(("end_time", end_date));
            }
            Url::parse_with_params(url.as_str(), &options).unwrap_or(url)
        }
    };
    let location_selector = match *page_type.read() {
        WeatherPage::Index | WeatherPage::Plot => {
            let sh = (*search_history.read()).clone();
            let hlc = (*history_location_cache.read()).clone();
            let locations: HashSet<_> = sh.iter().chain(hlc.iter()).map(|l| l.as_str()).collect();
            let mut locations: Vec<_> = locations.into_iter().collect();
            locations.sort();
            Some(rsx! {
                button {
                    id: "current-value",
                    name: "{location}",
                    value: "{location}",
                    "{location}",
                }
                select {
                    id: "history-selector",
                    onchange: move |x| {
                        let v = (*x.map(|data| data.value())).to_string();
                        if v.is_empty() {
                            return;
                        }
                        let s = v.as_str().to_string();
                        let loc = get_parameters(&s);
                        let sh = (*search_history.read()).clone();
                        if !sh.contains(&s) {
                            search_history.set(update_search_history(&sh, &s));
                        }
                        let hlc = (*history_location_cache.read()).clone();
                        if hlc.contains(&s) {
                            history_location.set(s.clone());
                        }
                        location.set(loc);
                    },
                    option {
                        value: "",
                        "",
                    },
                    {locations.iter().enumerate().filter_map(|(idx, s)| {
                        let loc = get_parameters(s);
                        if loc == *location.read() {
                            None
                        } else {
                            Some(
                                rsx! {
                                    option {
                                        key: "search-history-key-{idx}",
                                        value: "{s}",
                                        "{s}"
                                    }
                                }
                            )
                        }
                    })}
                },
                input {
                    "type": "button",
                    name: "clear",
                    value: "Clear",
                    onclick: move |_| {
                        let history = vec![String::from("10001")];

                        #[cfg(target_arch = "wasm32")]
                        set_history(&history).unwrap();

                        search_history.set(history);
                    }
                },
            })
        }
        WeatherPage::HistoryPlot => {
            let hlc = (*history_location_cache.read()).clone();
            let mut locations: Vec<_> = hlc.iter().map(|l| l.as_str()).collect();
            locations.sort();
            if !locations.contains(&history_location.read().as_str())
                && let Some(loc) = locations.first()
            {
                history_location.set(loc.to_string());
            }
            let start_date_string = start_date
                .read()
                .map_or("2023-01-01".into(), |d| format!("{d}"));
            let end_date_string = end_date
                .read()
                .map_or("2024-01-01".into(), |d| format!("{d}"));
            Some(rsx! {
                button {
                    id: "current-value",
                    name: "{history_location}",
                    value: "{history_location}",
                    "{history_location}",
                }
                select {
                    id: "history-location-selector",
                    onchange: move |x| {
                        let v = (*x.map(|data| data.value())).to_string();
                        if v.is_empty() {
                            return;
                        }
                        let s = v.as_str().to_string();
                        history_location.set(s.clone());
                        location.set(get_parameters(&s));
                    },
                    option {
                        value: "",
                        "",
                    },
                    {locations.iter().enumerate().filter_map(|(idx, s)| {
                        if s == &(*history_location.read()) {
                            None
                        } else {
                            Some(
                                rsx! {
                                    option {
                                        key: "location-history-key-{idx}",
                                        value: "{s}",
                                        "{s}",
                                    }
                                }
                            )
                        }
                    })},
                }
                input {
                    "type": "date",
                    name: "start-date",
                    value: "{start_date_string}",
                    onchange: move |x| {
                        let v = (*x.map(|data| data.value())).to_string();
                        if let Ok(date) = Date::parse(&v, DATE_FORMAT) {
                            start_date.set(Some(date));
                        }
                    }
                }
                input {
                    "type": "date",
                    name: "end-date",
                    value: "{end_date_string}",
                    onchange: move |x| {
                        let v = (*x.map(|data| data.value())).to_string();
                        if let Ok(date) = Date::parse(&v, DATE_FORMAT) {
                            end_date.set(Some(date));
                        }
                    }
                }
            })
        }
        WeatherPage::Wasm => None,
    };

    let page_element = match *page_type.read() {
        WeatherPage::Index => {
            let w = weather.read().clone();
            let f = forecast.read().clone();
            if let Some((weather, forecast)) = w.as_ref().and_then(|w| f.as_ref().map(|f| (w, f))) {
                Some(weather_element(weather, forecast))
            } else {
                None
            }
        }
        _ => Some(rsx! {
            iframe {
                src: "{url}",
                id: "weather-frame",
                height: "{height}",
                width: "{width}",
                align: "center",
            }
        }),
    };

    rsx! {
        div {
            input {
                "type": "button",
                name: "update_location",
                value: "Update Location",
                onclick: move |_| {
                    if *location.read() != *ip_location.read() {
                        let s = format!("{ip_location}");
                        let sh = (*search_history.read()).clone();
                        if !sh.contains(&s) {
                            search_history.set(update_search_history(&sh, &s));
                        }
                        location.set(ip_location.read().clone());
                        location_future.restart();
                    }
                },
            },
            input {
                "type": "button",
                name: "text",
                value: "Text",
                onclick: move |_| {
                    page_type.set(WeatherPage::Index);
                },
            },
            input {
                "type": "button",
                name: "plot",
                value: "Plot",
                onclick: move |_| {
                    page_type.set(WeatherPage::Plot);
                },
            },
            input {
                "type": "button",
                name: "history",
                value: "History",
                onclick: move |_| {
                    page_type.set(WeatherPage::HistoryPlot);
                },
            }
            input {
                "type": "button",
                name: "wasm",
                value: "Wasm",
                onclick: move |_| {
                    page_type.set(WeatherPage::Wasm);
                },
            },
            form {
                input {
                    "type": "text",
                    name: "location",
                    value: "{draft}",
                    id: "locationForm",
                    oninput: move |evt| {
                        let msg = (*evt.map(|data| data.value())).to_string();
                        draft.set(msg);
                    },
                },
                input {
                    "type": "button",
                    name: "submitLocation",
                    value: "Location",
                    onclick: move |_| {
                        let d = (*draft.read()).to_string();
                        if !d.is_empty() {
                            let loc = get_parameters(&d);
                            let sh = (*search_history.read()).clone();
                            if !sh.contains(&d.to_string()) {
                                search_history.set(update_search_history(&sh, &d));
                            }
                            location.set(loc);
                            draft.set(String::new());
                        }
                    },
                },
            },
            {location_selector},
        },
        {page_element},
    }
}
