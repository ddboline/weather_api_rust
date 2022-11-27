use dioxus::prelude::{
    dioxus_elements, format_args_f, rsx, Element, LazyNodes, NodeFactory, Scope, VNode,
    inline_props, Props, use_state, use_future,
};
use futures_channel::oneshot::{channel, Sender};
use stack_string::StackString;
use time::{macros::format_description, UtcOffset};
use weather_util_rust::{
    precipitation::Precipitation, weather_data::WeatherData, weather_forecast::WeatherForecast,
    weather_api::WeatherLocation, latitude::Latitude, longitude::Longitude,
};
use std::{fmt::Write, net::Ipv4Addr};
use anyhow::Error;
use std::collections::HashMap;
use url::Url;
use serde::{Serialize, Deserialize};
use log::debug;

static DEFAULT_STR: &str = "11106";
static API_ENDPOINT: &str = "https://cloud.ddboline.net/weather/";

#[derive(PartialEq)]
pub struct PlotData {
    forecast_data: StackString,
    title: StackString,
    xaxis: StackString,
    yaxis: StackString,
}

#[inline_props]
pub fn weather_component(
    cx: Scope,
    weather: WeatherData,
    forecast: Option<WeatherForecast>,
    plot: Option<Vec<PlotData>>,
) -> Element {
    cx.render(
        weather_element(weather, forecast, plot)
    )
}

pub fn weather_element<'a>(
    weather: &'a WeatherData,
    forecast: &'a Option<WeatherForecast>,
    plot: &'a Option<Vec<PlotData>>,
) -> LazyNodes<'a, 'a> {
    let weather_data = weather.get_current_conditions();
    let weather_lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let weather_cols = weather_lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let weather_rows = weather_lines.len() + 5;
    let weather_lines = weather_lines.join("\n");

    let forecast_lines = forecast.as_ref().map(|forecast| {
        let weather_forecast = forecast.get_forecast();
        let forecast_lines: Vec<_> = weather_forecast.iter().map(|s| s.trim_end()).collect();
        let forecast_cols = forecast_lines.iter().map(|x| x.len()).max().unwrap_or(0) + 10;
        (forecast_cols, forecast_lines.join("\n"))
    });

    rsx! {
        head {
            title: "Weather Plots",
            style {
                [include_str!("../../templates/style.css")]
            }
        },
        body {
            div {
                textarea {
                    readonly: "true",
                    rows: "{weather_rows}",
                    cols: "{weather_cols}",
                    "{weather_lines}"
                },
                {
                    forecast_lines.map(|(forecast_cols, forecast_lines)| rsx! {
                        textarea {
                            readonly: "true",
                            rows: "{weather_rows}",
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

fn plot_element<'a>(
    plots: &'a [PlotData],
) -> LazyNodes<'a, 'a> {
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
pub fn get_forecast_plots(forecast: &WeatherForecast) -> Result<Vec<PlotData>, Error> {
    let mut plots = Vec::new();

    let fo: UtcOffset = forecast.city.timezone.into();
    let forecast_data = forecast
        .list
        .iter()
        .map(|entry| {
            let date_str: StackString = entry
                .dt
                .to_offset(fo)
                .format(format_description!(
                    "[year]-[month]-[day]T[hour]:[minute]:[second]"
                ))?
                .into();
            let temp = entry.main.temp.fahrenheit();
            Ok((date_str, temp))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let forecast_data = serde_json::to_string(&forecast_data)
        .map_err(Into::<Error>::into)?
        .into();

    plots.push(PlotData {
        forecast_data,
        title: "Temperature Forecast".into(),
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
            let dt_str: StackString = entry
                .dt
                .to_offset(fo)
                .format(format_description!(
                    "[year]-[month]-[day]T[hour]:[minute]:[second]"
                ))?
                .into();
            Ok((dt_str, (rain + snow).inches()))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let forecast_data = serde_json::to_string(&forecast_data)
        .map_err(Into::<Error>::into)?
        .into();

    plots.push(PlotData {
        forecast_data,
        title: "Precipitation Forecast".into(),
        xaxis: "".into(),
        yaxis: "in".into(),
    });

    Ok(plots)
}

#[derive(Clone, Debug)]
struct WeatherEntry {
    weather: Option<WeatherData>,
    forecast: Option<WeatherForecast>,
}

pub fn weather_app_element(cx: Scope<()>) -> Element {
    let (send, recv) = channel();

    let default_cache: HashMap<WeatherLocation, WeatherEntry> = HashMap::new();
    let mut default_location_cache: HashMap<String, WeatherLocation> = HashMap::new();
    default_location_cache.insert(DEFAULT_STR.into(), get_parameters(DEFAULT_STR));

    let (cache, set_cache) = use_state(&cx, || default_cache).split();
    let (location_cache, set_location_cache) = use_state(&cx, || default_location_cache).split();
    let (location, set_location) = use_state(&cx, || get_parameters(DEFAULT_STR)).split();
    let (weather, set_weather) = use_state(&cx, WeatherData::default).split();
    let (forecast, set_forecast) = use_state(&cx, WeatherForecast::default).split();
    let (draft, set_draft) = use_state(&cx, String::new).split();
    let (search_history, set_search_history) =
        use_state(&cx, || vec![StackString::from(DEFAULT_STR)]).split();

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

    let weather_future = use_future(&cx, location, |l| {
        let entry_opt = cache.get(&l).cloned();
        async move {
            if let Some(entry) = entry_opt {
                entry
            } else {
                get_weather_data_forecast(&l).await
            }
        }
    });

    cx.render({
        if let Some(Some(location)) = location_future.value() {
            set_location.modify(|_| get_parameters(&format!("{},{}", location.latitude, location.longitude)));
            set_location.needs_update();
        }
        if let Some(entry) = weather_future.value() {
            set_cache.modify(|c| {
                let new_cache = c.update(location.clone(), entry.clone());
                if let Some(WeatherEntry{weather, forecast}) = new_cache.get(location) {
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
        let country_info_element = country_info(weather);
        let country_data_element = country_data(weather);
        let week_weather_element = week_weather(forecast);

        rsx!(
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
                                                set_location_cache.modify(|lc| lc.update(msg.into(), l.clone()));
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
                                                set_location_cache.modify(|lc| lc.update(draft.into(), l.clone()));
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
                                        if evt.key == "Enter" {
                                            set_draft.modify(|_| String::new());
                                            set_draft.needs_update();
                                            set_search_history.modify(|sh| {
                                                let mut v: Vec<StackString> = sh.iter().filter(|s| s.as_str() != draft.as_str()).cloned().collect();
                                                v.push(draft.into());
                                                v
                                            });
                                            set_location.modify(|_| new_location);
                                            set_location.needs_update();
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
                                let new_location = location_cache.get(s).map_or_else(|| {
                                    let l = get_parameters(s);
                                    set_location_cache.modify(|lc| lc.update(s.into(), l.clone()));
                                    set_location_cache.needs_update();
                                    set_search_history.modify(|sh| {
                                        let mut v: Vec<StackString> = sh.iter().filter(|x| x.as_str() != s).cloned().collect();
                                        v.push(s.into());
                                        v
                                    });
                                    set_search_history.needs_update();
                                    l
                                }, Clone::clone);
                                debug!("{new_location:?}");
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
                                    let selected = location_cache.contains_key(s.as_str());
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
        )
    })
}

fn get_parameters(search_str: &str) -> WeatherLocation {
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

#[cfg(target_arch = "wasm32")]
async fn get_ip_address() -> Result<Ipv4Addr, JsValue> {
    let url: Url = "https://ipinfo.io/ip".parse().map_err(|e| {
        error!("error {e}");
        let e: JsValue = format!("{e}").into();
        e
    })?;
    let resp = text_fetch(&url, Method::GET).await?;
    let resp = resp
        .as_string()
        .ok_or_else(|| JsValue::from_str("Failed to get ip"))?
        .trim()
        .to_string();
    debug!("got resp {resp}");
    resp.parse().map_err(|e| {
        let e: JsValue = format!("{e}").into();
        e
    })
}

#[cfg(target_arch = "wasm32")]
async fn get_location_from_ip(ip: Ipv4Addr) -> Result<WeatherLocation, JsValue> {
    #[derive(Default, Serialize, Deserialize)]
    struct Location {
        latitude: Latitude,
        longitude: Longitude,
    }

    let ipaddr = ip.to_string();
    let url = Url::parse("https://ipwhois.app/json/")
        .map_err(|e| {
            error!("error {e}");
            let e: JsValue = format!("{e}").into();
            e
        })?
        .join(&ipaddr)
        .map_err(|e| {
            error!("error {e}");
            let e: JsValue = format!("{e}").into();
            e
        })?;
    debug!("url {url}");
    let json = js_fetch(&url, Method::GET).await?;
    let location: Location = serde_wasm_bindgen::from_value(json)?;
    Ok(WeatherLocation::from_lat_lon(
        location.latitude,
        location.longitude,
    ))
}


#[cfg(not(target_args = "wasm32"))]
async fn get_ip_address() -> Result<Ipv4Addr, Error> {
    let url: Url = "https://ipinfo.io/ip".parse()?;
    let text = reqwest::get(url).await?.text().await?;
    text.trim().parse().map_err(Into::into)
}

#[cfg(not(target_args = "wasm32"))]
async fn get_location_from_ip(ip: Ipv4Addr) -> Result<WeatherLocation, Error> {
    #[derive(Default, Serialize, Deserialize)]
    struct Location {
        latitude: Latitude,
        longitude: Longitude,
    }

    let ipaddr = ip.to_string();
    let url = Url::parse("https://ipwhois.app/json/")?.join(&ipaddr)?;
    let location: Location = reqwest::get(url).await?.json().await?;
    Ok(WeatherLocation::from_lat_lon(
        location.latitude,
        location.longitude,
    ))   
}