use anyhow::Error;
use dioxus::prelude::{
    dioxus_elements, inline_props, rsx, use_future, use_state, Element, GlobalAttributes,
    LazyNodes, Props, Scope, SvgAttributes, UseFuture, UseState,
};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures_util::lock::Mutex;
use keyboard_types::Key;
use log::debug;
use std::{collections::HashMap, fmt::Write, sync::Arc};
use time::{macros::format_description, UtcOffset};
use url::Url;

#[cfg(not(target_arch = "wasm32"))]
use futures_util::{sink::SinkExt, StreamExt};

use weather_util_rust::{
    format_string, precipitation::Precipitation, weather_api::WeatherLocation,
    weather_data::WeatherData, weather_forecast::WeatherForecast,
};

use crate::WeatherEntry;

#[cfg(target_arch = "wasm32")]
use crate::wasm_utils::{
    get_ip_address, get_location_from_ip, get_weather_data_forecast, set_history,
};

pub static DEFAULT_STR: &str = "11106";
pub static DEFAULT_URL: &str = "https://www.ddboline.net";

pub static DEFAULT_LOCATION: &str = "10001";

#[cfg(debug_assertions)]
static BASE_URL: Option<&str> = Some(DEFAULT_URL);

#[cfg(not(debug_assertions))]
static BASE_URL: Option<&str> = None;

#[derive(PartialEq, Eq)]
pub struct PlotData {
    forecast_data: String,
    title: String,
    xaxis: String,
    yaxis: String,
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

#[inline_props]
pub fn weather_component(
    cx: Scope,
    weather: WeatherData,
    forecast: Option<WeatherForecast>,
    plot: Option<Vec<PlotData>>,
) -> Element {
    cx.render(weather_element(weather, forecast, plot))
}

pub fn weather_element<'a>(
    weather: &'a WeatherData,
    forecast: &'a Option<WeatherForecast>,
    plot: &'a Option<Vec<PlotData>>,
) -> LazyNodes<'a, 'a> {
    let weather_data = weather.get_current_conditions();
    let weather_lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let weather_cols = weather_lines.iter().map(|x| x.len()).max().unwrap_or(0) + 2;
    let weather_rows = weather_lines.len() + 2;
    let weather_lines = weather_lines.join("\n");

    let weather_element = if plot.is_none() {
        rsx! {
            textarea {
                readonly: "true",
                rows: "{weather_rows}",
                cols: "{weather_cols}",
                "{weather_lines}"
            },
        }
    } else {
        let name = &weather.name;
        let mut title = format_string!("{name}");
        if let Some(country) = &weather.sys.country {
            write!(&mut title, " {country}").unwrap();
        }
        let lat = weather.coord.lat;
        let lon = weather.coord.lon;
        write!(&mut title, " {lat:0.5}N {lon:0.5}E").unwrap();
        rsx! {
            div {
                style: "text-anchor: middle; font-size: 16px;",
                "{title}"
            }
        }
    };

    let forecast_lines = forecast.as_ref().map(|forecast| {
        let weather_forecast = forecast.get_forecast();
        let forecast_lines: Vec<_> = weather_forecast.iter().map(|s| s.trim_end()).collect();
        let forecast_cols = forecast_lines.iter().map(|x| x.len()).max().unwrap_or(0) + 2;
        let forecast_rows = forecast_lines.len() + 2;
        (forecast_rows, forecast_cols, forecast_lines.join("\n"))
    });

    rsx! {
        head {
            title: "Weather Plots",
            style {
                include_str!("../../templates/style.css")
            }
        },
        body {
            div {
                weather_element,
                {
                    forecast_lines.map(|(forecast_rows, forecast_cols, forecast_lines)| rsx! {
                        textarea {
                            readonly: "true",
                            rows: "{forecast_rows}",
                            cols: "{forecast_cols}",
                            "{forecast_lines}"
                        }
                    })
                }
            },
            plot.as_ref().map(|plots| plot_element(plots)),
        }
    }
}

fn plot_element(plots: &[PlotData]) -> LazyNodes {
    rsx! {
        script {
            src: "https://d3js.org/d3.v4.min.js",
        },
        script {
            "src": "/weather/timeseries.js",
        },
        br {},
        plots.iter().enumerate().map(|(idx, pd)| {
            let forecast_data = &pd.forecast_data;
            let title = &pd.title;
            let xaxis = &pd.xaxis;
            let yaxis = &pd.yaxis;
            let mut script_body = String::new();
            script_body.push_str("\n!function(){\n");
            writeln!(&mut script_body, "\tlet forecast_data = {forecast_data};").unwrap();
            writeln!(&mut script_body, "\tcreate_plot(forecast_data, '{title}', '{xaxis}', '{yaxis}');").unwrap();
            script_body.push_str("}();\n");

            rsx! {
                script {
                    key: "forecast-plot-key-{idx}",
                    "{script_body}",
                }
            }
        }),
    }
}

/// # Errors
/// Returns error if there is a syntax or parsing error
pub fn get_forecast_plots(
    weather: &WeatherData,
    forecast: &WeatherForecast,
) -> Result<Vec<PlotData>, Error> {
    let mut plots = Vec::new();

    let fo: UtcOffset = forecast.city.timezone.into();
    let forecast_data = forecast
        .list
        .iter()
        .map(|entry| {
            let date_str = entry.dt.to_offset(fo).format(format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second]"
            ))?;
            let temp = entry.main.temp.fahrenheit();
            Ok((date_str, temp))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let forecast_data = serde_json::to_string(&forecast_data).map_err(Into::<Error>::into)?;

    plots.push(PlotData {
        forecast_data,
        title: format!(
            "Temperature Forecast {:0.1} F / {:0.1} C",
            weather.main.temp.fahrenheit(),
            weather.main.temp.celcius()
        ),
        xaxis: "".into(),
        yaxis: "F".into(),
    });

    let forecast_data = forecast
        .list
        .iter()
        .map(|entry| {
            let rain = if let Some(rain) = &entry.rain {
                rain.three_hour.unwrap_or_default()
            } else {
                Precipitation::default()
            };
            let snow = if let Some(snow) = &entry.snow {
                snow.three_hour.unwrap_or_default()
            } else {
                Precipitation::default()
            };
            let dt_str = entry.dt.to_offset(fo).format(format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second]"
            ))?;
            Ok((dt_str, (rain + snow).inches()))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let forecast_data = serde_json::to_string(&forecast_data).map_err(Into::<Error>::into)?;

    plots.push(PlotData {
        forecast_data,
        title: "Precipitation Forecast".into(),
        xaxis: "".into(),
        yaxis: "in".into(),
    });

    Ok(plots)
}

fn weather_app_element<'a>(
    draft: &'a str,
    set_draft: &'a UseState<String>,
    location_cache: &'a HashMap<String, WeatherLocation>,
    set_location_cache: &'a UseState<HashMap<String, WeatherLocation>>,
    cache: &'a HashMap<WeatherLocation, WeatherEntry>,
    set_cache: &'a UseState<HashMap<WeatherLocation, WeatherEntry>>,
    location: &'a WeatherLocation,
    set_location: &'a UseState<WeatherLocation>,
    weather: &'a WeatherData,
    set_weather: &'a UseState<WeatherData>,
    forecast: &'a WeatherForecast,
    set_forecast: &'a UseState<WeatherForecast>,
    search_history: &'a [String],
    set_search_history: &'a UseState<Vec<String>>,
) -> LazyNodes<'a, 'a> {
    let country_info_element = country_info(weather);
    let country_data_element = country_data(weather);
    let week_weather_element = week_weather(forecast);

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
                                    let msg = evt.value.as_str();
                                    set_draft.modify(|_| msg.into());
                                    set_draft.needs_update();
                                    let new_location = location_cache.get(msg).map_or_else(
                                        || {
                                            let l = get_parameters(msg);
                                            set_location_cache.modify(|lc| {
                                                let mut lc = lc.clone();
                                                lc.insert(msg.into(), l.clone());
                                                lc
                                            });
                                            set_location_cache.needs_update();
                                            l
                                        }, Clone::clone
                                    );
                                    if let Some(WeatherEntry{weather, forecast}) = cache.get(&new_location) {
                                        if let Some(weather) = weather {
                                            debug!("weather_oninput {location:?}");
                                            set_weather.modify(|_| weather.clone());
                                            set_weather.needs_update();
                                        }
                                        if let Some(forecast) = forecast {
                                            debug!("forecast_oninput {location:?}");
                                            set_forecast.modify(|_| forecast.clone());
                                            set_forecast.needs_update();
                                        }
                                        set_location.modify(|_| new_location);
                                        set_location.needs_update();
                                    }
                                },
                                onkeydown: move |evt| {
                                    let new_location = location_cache.get(draft).map_or_else(
                                        || {
                                            let l = get_parameters(draft);
                                            set_location_cache.modify(|lc| {
                                                let mut lc = lc.clone();
                                                lc.insert(draft.into(), l.clone());
                                                lc
                                            });
                                            set_location_cache.needs_update();
                                            l
                                        }, Clone::clone
                                    );
                                    if let Some(WeatherEntry{weather, forecast}) = cache.get(&new_location) {
                                        if let Some(weather) = weather {
                                            debug!("weather_onkeydown {location:?}");
                                            set_weather.modify(|_| weather.clone());
                                            set_weather.needs_update();
                                        }
                                        if let Some(forecast) = forecast {
                                            debug!("forecast_onkeydown {location:?}");
                                            set_forecast.modify(|_| forecast.clone());
                                            set_forecast.needs_update();
                                        }
                                    }
                                    if evt.key() == Key::Enter {
                                        set_draft.modify(|_| String::new());
                                        set_draft.needs_update();
                                        set_search_history.modify(|sh| {
                                            let mut v: Vec<String> = sh.iter().filter(|s| s.as_str() != draft).cloned().collect();
                                            v.push(draft.into());
                                            v
                                        });
                                        set_location.modify(|_| new_location);
                                        set_location.needs_update();
                                        set_cache.needs_update();
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
                            let s = x.data.value.as_str();
                            debug!("s {s:?}");
                            let new_location = location_cache.get(s).map_or_else(|| {
                                let l = get_parameters(s);
                                set_location_cache.modify(|lc| {
                                    let mut lc = lc.clone();
                                    lc.insert(s.into(), l.clone());
                                    lc
                                });
                                set_location_cache.needs_update();
                                set_search_history.modify(|sh| {
                                    let mut v: Vec<String> = sh.iter().filter(|x| x.as_str() != s).cloned().collect();
                                    v.push(s.into());
                                    v
                                });
                                set_search_history.needs_update();
                                l
                            }, Clone::clone);
                            debug!("new_location {new_location:?}");
                            if let Some(WeatherEntry{weather, forecast}) = cache.get(&new_location) {
                                if let Some(weather) = weather {
                                    debug!("weather {new_location:?}");
                                    set_weather.modify(|_| weather.clone());
                                    set_weather.needs_update();
                                }
                                if let Some(forecast) = forecast {
                                    debug!("forecast {new_location:?}");
                                    set_forecast.modify(|_| forecast.clone());
                                    set_forecast.needs_update();
                                }
                            }
                            set_location.modify(|_| new_location);
                            set_location.needs_update();
                        },
                        {
                            search_history.iter().rev().map(|s| {
                                let selected = &get_parameters(s) == location;
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
                            country_info_element,
                            country_data_element,
                        }
                        week_weather_element,
                    }
                }
            }
        }
    }
}

pub struct AppProps {
    pub send: Arc<Mutex<UnboundedSender<WeatherLocation>>>,
    pub recv: Arc<Mutex<UnboundedReceiver<(WeatherLocation, WeatherEntry)>>>,
}

pub fn weather_app_component(cx: Scope<AppProps>) -> Element {
    let default_cache: HashMap<WeatherLocation, WeatherEntry> = HashMap::new();
    let mut default_location_cache: HashMap<String, WeatherLocation> = HashMap::new();
    default_location_cache.insert(DEFAULT_STR.into(), get_parameters(DEFAULT_STR));

    let (cache, set_cache) = use_state(cx, || default_cache).split();
    let (location_cache, set_location_cache) = use_state(cx, || default_location_cache).split();
    let (weather, set_weather) = use_state(cx, WeatherData::default).split();
    let (forecast, set_forecast) = use_state(cx, WeatherForecast::default).split();
    let (draft, set_draft) = use_state(cx, String::new).split();
    let (search_history, set_search_history) =
        use_state(cx, || vec![String::from(DEFAULT_STR)]).split();

    let (location, set_location) = use_state(cx, || get_parameters(DEFAULT_LOCATION)).split();

    #[cfg(not(target_arch = "wasm32"))]
    let recv_future = use_future(cx, (), |_| {
        let recv = cx.props.recv.clone();
        async move {
            let mut recv = recv.lock().await;
            debug!("recv");
            recv.next().await
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    let _send_future = use_future(cx, location, |l| {
        let contains_key = cache.contains_key(&l);
        let send = cx.props.send.clone();
        async move {
            if !contains_key {
                debug!("send {l}");
                let mut send = send.lock().await;
                send.send(l.clone()).await.unwrap();
            }
        }
    });

    #[cfg(target_arch = "wasm32")]
    let location_future = use_future(cx, (), |_| async move {
        if let Ok(ip) = get_ip_address().await {
            debug!("ip {ip}");
            if let Ok(location) = get_location_from_ip(ip).await {
                debug!("get location {location:?}");
                return Some(location);
            }
        }
        None
    });

    #[cfg(target_arch = "wasm32")]
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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(Some((loc, entry))) = recv_future.value() {
            if (!cache.contains_key(loc)) || cache.is_empty() {
                debug!(
                    "set location {location} {} {}",
                    entry.weather.is_none(),
                    entry.forecast.is_none()
                );
                set_location.modify(|_| loc.clone());
                set_location.needs_update();
                set_cache.modify(|c| {
                    let mut new_cache = c.clone();
                    new_cache.insert(location.clone(), entry.clone());
                    new_cache
                });
                set_cache.needs_update();
                recv_future.restart();
                if let Some(weather) = &entry.weather {
                    debug!("weather_oninput {location:?}");
                    set_weather.modify(|_| weather.clone());
                    set_weather.needs_update();
                }
                if let Some(forecast) = &entry.forecast {
                    debug!("forecast_oninput {location:?}");
                    set_forecast.modify(|_| forecast.clone());
                    set_forecast.needs_update();
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        if let Some(Some(loc)) = location_future.value() {
            if loc != location && (!cache.contains_key(loc) || cache.is_empty()) {
                debug!("set location {location}");
                set_location.modify(|_| loc.clone());
                set_location.needs_update();
            }
        }

        #[cfg(target_arch = "wasm32")]
        if let Some((loc, entry)) = weather_future.value() {
            if !cache.contains_key(loc) || cache.is_empty() {
                set_cache.modify(|c| {
                    let mut new_cache = c.clone();
                    new_cache.insert(location.clone(), entry.clone());
                    if let Some(WeatherEntry { weather, forecast }) = new_cache.get(location) {
                        if let Some(weather) = weather {
                            debug!("weather_future {location:?}");
                            set_weather.modify(|_| weather.clone());
                            set_weather.needs_update();
                        }
                        if let Some(forecast) = forecast {
                            debug!("forecast_future {location:?}");
                            set_forecast.modify(|_| forecast.clone());
                            set_forecast.needs_update();
                        }
                    }
                    new_cache
                });
                set_cache.needs_update();
            }
        }

        weather_app_element(
            draft,
            set_draft,
            location_cache,
            set_location_cache,
            cache,
            set_cache,
            location,
            set_location,
            weather,
            set_weather,
            forecast,
            set_forecast,
            search_history,
            set_search_history,
        )
    })
}

pub fn get_parameters(search_str: &str) -> WeatherLocation {
    let mut opts = WeatherLocation::from_city_name(search_str);
    if let Ok(zip) = search_str.parse::<u64>() {
        opts = WeatherLocation::from_zipcode(zip);
    } else if search_str.contains(',') {
        let mut iter = search_str.split(',');
        if let Some(lat) = iter.next() {
            if let Ok(lat) = lat.parse() {
                if let Some(lon) = iter.next() {
                    if let Ok(lon) = lon.parse() {
                        opts = WeatherLocation::from_lat_lon(lat, lon);
                    }
                }
            }
        }
    }
    opts
}

fn country_data(weather: &WeatherData) -> LazyNodes {
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

fn country_info(weather: &WeatherData) -> LazyNodes {
    let name = &weather.name;
    let country = weather.sys.country.as_ref().map_or("", |s| s.as_str());
    let mut main = String::new();
    let mut desc = String::new();
    let mut icon = String::new();
    if let Some(weather) = weather.weather.get(0) {
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

fn week_weather(forecast: &WeatherForecast) -> LazyNodes {
    let high_low = forecast.get_high_low();
    rsx!(
        div { class: "divider table mx-2 text-center bg-transparent whitespace-nowrap",
            span { class: "inline-block px-3", small { "Forecast" } }
        }
        div { class: "px-6 py-6 relative",
            div { class: "text-center justify-between items-center flex",
                style: "flex-flow: initial;",
                high_low.iter().map(|(d, (h, l, r, s, i))| {
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
                })
            }
        }
    )
}

pub fn index_element<'a>(
    height: u64,
    width: u64,
    origin: String,
    url_path: &'a str,
    set_url_path: &'a UseState<&'static str>,
    draft: &'a str,
    set_draft: &'a UseState<String>,
    location: &'a WeatherLocation,
    set_location: &'a UseState<WeatherLocation>,
    ip_location: &'a WeatherLocation,
    set_ip_location: &'a UseState<WeatherLocation>,
    search_history: &'a [String],
    set_search_history: &'a UseState<Vec<String>>,
    location_future: &'a UseFuture<Option<WeatherLocation>>,
    set_current_loc: &'a UseState<Option<String>>,
) -> LazyNodes<'a, 'a> {
    let base_url = BASE_URL.unwrap_or(&origin);
    let url: Url = format!("{base_url}/{url_path}")
        .parse()
        .expect("Failed to parse base url");
    let url = Url::parse_with_params(url.as_str(), location.get_options()).unwrap_or(url);
    if let Some(Some(loc)) = location_future.value() {
        if loc != ip_location {
            debug!("set location {loc:?}");
            set_ip_location.set(loc.clone());
        }
    }

    rsx! {
        div {
            input {
                "type": "button",
                name: "update_location",
                value: "Update Location",
                onclick: move |_| {
                    if location != ip_location {
                        let s = format!("{ip_location}");
                        if !search_history.contains(&s) {
                            set_search_history.modify(|sh| update_search_history(sh, &s));
                            set_search_history.needs_update();
                        }
                        set_location.modify(|_| ip_location.clone());
                        set_location.needs_update();
                        location_future.restart();
                    }
                },
            },
            input {
                "type": "button",
                name: "text",
                value: "Text",
                onclick: move |_| {
                    set_url_path.modify(|_| "weather/index.html");
                },
            },
            input {
                "type": "button",
                name: "plot",
                value: "Plot",
                onclick: move |_| {
                    set_url_path.modify(|_| "weather/plot.html");
                },
            },
            input {
                "type": "button",
                name: "wasm",
                value: "Wasm",
                onclick: move |_| {
                    set_url_path.modify(|_| "wasm_weather/index.html");
                },
            },
            form {
                input {
                    "type": "text",
                    name: "location",
                    value: "{draft}",
                    id: "locationForm",
                    oninput: move |evt| {
                        let msg = evt.value.as_str();
                        set_draft.modify(|_| {msg.into()});
                        set_draft.needs_update();
                    },
                },
                input {
                    "type": "button",
                    name: "submitLocation",
                    value: "Location",
                    onclick: move |_| {
                        if !draft.is_empty() {
                            let loc = get_parameters(draft);
                            if !search_history.contains(&draft.to_string()) {
                                set_search_history.modify(|sh| update_search_history(sh, draft));
                                set_search_history.needs_update();
                            }
                            set_location.modify(|_| loc);
                            set_location.needs_update();
                            set_current_loc.set(Some(draft.to_string()));
                            set_current_loc.needs_update();
                            set_draft.set(String::new());
                            set_draft.needs_update();
                        }
                    },
                },
            },
            button {
                id: "current-value",
                name: "{location}",
                value: "{location}",
                "{location}",
            }
            select {
                id: "history-selector",
                onchange: move |x| {
                    if x.data.value.is_empty() {
                        return;
                    }
                    let s = x.data.value.as_str().to_string();
                    let loc = get_parameters(&s);
                    if !search_history.contains(&s) {
                        set_search_history.modify(|sh| update_search_history(sh, &s));
                        set_search_history.needs_update();
                    }
                    set_location.modify(|_| loc);
                    set_location.needs_update();
                },
                option {
                    value: "",
                    "",
                },
                search_history.iter().rev().enumerate().filter_map(|(idx, s)| {
                    let loc = get_parameters(&s);
                    if &loc == location {
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
                })
            },
            input {
                "type": "button",
                name: "clear",
                value: "Clear",
                onclick: move |_| {
                    let history = vec![String::from("10001")];

                    #[cfg(target_arch = "wasm32")]
                    set_history(&history).unwrap();

                    set_search_history.set(history);
                    set_search_history.needs_update();
                }
            },
        },
        iframe {
            src: "{url}",
            id: "weather-frame",
            height: "{height}",
            width: "{width}",
            align: "center",
        },
    }
}
