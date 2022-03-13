use anyhow::{format_err, Error};
use arc_swap::ArcSwap;
use chrono::FixedOffset;
use dioxus::prelude::{
    dioxus_elements, fc_to_builder, format_args_f, rsx, use_state, Element, LazyNodes, NodeFactory,
    Props, Scope, VNode,
};
use im::HashMap;
use lazy_static::lazy_static;
use log::debug;
use stack_string::{format_sstr, StackString};
use std::{fmt::Write, sync::Arc};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use weather_util_rust::{
    config::Config,
    weather_api::{WeatherApi, WeatherLocation},
    weather_data::WeatherData,
    weather_forecast::WeatherForecast,
};

lazy_static! {
    static ref WEATHER_CACHE: WeatherCache = WeatherCache::new();
}

static DEFAULT_STR: &str = "11106";

#[derive(Clone, Debug)]
struct WeatherEntry {
    weather: Option<WeatherData>,
    forecast: Option<WeatherForecast>,
}

struct WeatherCache(ArcSwap<HashMap<StackString, WeatherEntry>>);

impl WeatherCache {
    fn new() -> Self {
        Self(ArcSwap::new(Arc::new(HashMap::new())))
    }

    fn update(&self, msg: &str, weather: Option<WeatherData>, forecast: Option<WeatherForecast>) {
        let new_cache = Arc::new(
            self.0
                .load()
                .update(msg.into(), WeatherEntry { weather, forecast }),
        );
        self.0.store(new_cache);
    }

    fn contains_key(&self, key: &str) -> bool {
        self.0.load().contains_key(key)
    }

    fn get_map(&self) -> Arc<HashMap<StackString, WeatherEntry>> {
        self.0.load().clone()
    }
}

fn main() -> Result<(), Error> {
    env_logger::init();
    let (send, mut recv) = unbounded_channel::<StackString>();
    let config = Config::init_config()?;
    let api_key = config
        .api_key
        .as_ref()
        .ok_or_else(|| format_err!("No api key given"))?;
    let api = WeatherApi::new(api_key.as_str(), &config.api_endpoint, &config.api_path);

    let handle: std::thread::JoinHandle<Result<(), Error>> = std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(async move {
                while let Some(msg) = recv.recv().await {
                    debug!("grab {msg} weather");
                    let loc = get_parameters(&msg);
                    let weather = api.get_weather_data(&loc).await.ok();
                    let forecast = api.get_weather_forecast(&loc).await.ok();
                    WEATHER_CACHE.update(&msg, weather, forecast);
                }
            });
        Ok(())
    });

    send.send(DEFAULT_STR.into())?;
    loop {
        if WEATHER_CACHE.contains_key(DEFAULT_STR) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    dioxus::desktop::launch_with_props(app, AppProps { send }, |c| c);
    handle.join().unwrap()?;
    Ok(())
}

struct AppProps {
    send: UnboundedSender<StackString>,
}

fn app(cx: Scope<AppProps>) -> Element {
    let (search_str, set_search_str) = use_state(&cx, StackString::new).split();
    let (weather_default, forecast_default) = {
        let weather_cache = WEATHER_CACHE.get_map();
        let WeatherEntry { weather, forecast } = weather_cache.get(DEFAULT_STR).unwrap();
        (
            weather.as_ref().unwrap().clone(),
            forecast.as_ref().unwrap().clone(),
        )
    };
    let (weather, set_weather) = use_state(&cx, || weather_default).split();
    let (forecast, set_forecast) = use_state(&cx, || forecast_default).split();
    let (draft, set_draft) = use_state(&cx, || search_str.clone()).split();

    cx.render(rsx!(
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
                                    let weather_cache = WEATHER_CACHE.get_map();
                                    set_draft.modify(|_| evt.value.as_str().into());
                                    set_draft.needs_update();
                                    if let Some(WeatherEntry{weather, forecast}) = weather_cache.get(msg) {
                                        if let Some(weather) = weather {
                                            set_weather.modify(|_| weather.clone());
                                            set_weather.needs_update();
                                        }
                                        if let Some(forecast) = forecast {
                                            set_forecast.modify(|_| forecast.clone());
                                            set_forecast.needs_update();
                                        }
                                    }
                                },
                                onkeydown: move |evt| {
                                    let weather_cache = WEATHER_CACHE.get_map();
                                    if let Some(WeatherEntry{weather, forecast}) = weather_cache.get(draft) {
                                        if let Some(weather) = weather {
                                            set_weather.modify(|_| weather.clone());
                                            set_weather.needs_update();
                                        }
                                        if let Some(forecast) = forecast {
                                            set_forecast.modify(|_| forecast.clone());
                                            set_forecast.needs_update();
                                        }
                                    }
                                    if evt.key == "Enter" {
                                        set_search_str.modify(|_| draft.clone());
                                        set_search_str.needs_update();
                                        cx.props.send.send(draft.clone()).unwrap();
                                        loop {
                                            let weather_cache = WEATHER_CACHE.get_map();
                                            if let Some(WeatherEntry{weather, forecast}) = weather_cache.get(draft) {
                                                if let Some(weather) = weather {
                                                    set_weather.modify(|_| weather.clone());
                                                    set_weather.needs_update();
                                                }
                                                if let Some(forecast) = forecast {
                                                    set_forecast.modify(|_| forecast.clone());
                                                    set_forecast.needs_update();
                                                }
                                                break;
                                            }
                                            std::thread::sleep(std::time::Duration::from_millis(10));
                                        }
                                    }
                                },
                            }
                            svg { class: "w-4 h-4 absolute left-2.5 top-3.5",
                                "viewBox": "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                xmlns: "http://www.w3.org/2000/svg",
                                path {
                                    d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z",
                                    "stroke-linejoin": "round",
                                    "stroke-linecap": "round",
                                    "stroke-width": "2",
                                }
                            }
                        }
                    }
                }
                div { class: "flex flex-wrap w-full px-2",
                    div { class: "bg-gray-900 text-white relative min-w-0 break-words rounded-lg overflow-hidden shadow-sm mb-4 w-full bg-white dark:bg-gray-600",
                        div { class: "px-6 py-6 relative",
                            country_info( weather: weather, forecast: forecast )
                            country_data( weather: weather, forecast: forecast )
                        }
                        week_weather( weather: weather, forecast: forecast )
                    }
                }
            }
        }
    ))
}

#[allow(clippy::used_underscore_binding)]
#[derive(Props)]
struct WeatherForecastProp<'a> {
    weather: &'a WeatherData,
    forecast: &'a WeatherForecast,
}

fn country_data<'a>(cx: Scope<'a, WeatherForecastProp<'a>>) -> Element {
    let weather = cx.props.weather;
    let temp = weather.main.temp.fahrenheit();
    let feels = weather.main.feels_like.fahrenheit();
    let min = weather.main.temp_min.fahrenheit();
    let max = weather.main.temp_max.fahrenheit();

    cx.render(rsx!(
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
    ))
}

fn country_info<'a>(cx: Scope<'a, WeatherForecastProp<'a>>) -> Element {
    let weather = cx.props.weather;
    let name = &weather.name;
    let country = weather.sys.country.as_ref().map_or("", StackString::as_str);
    let mut main = StackString::new();
    let mut desc = StackString::new();
    let mut icon = StackString::new();
    if let Some(weather) = weather.weather.get(0) {
        main.push_str(&weather.main);
        desc.push_str(&weather.description);
        icon.push_str(&weather.icon);
    }
    let temp = weather.main.temp.fahrenheit();
    let fo: FixedOffset = weather.timezone.into();
    let date = weather.dt.with_timezone(&fo);

    cx.render(rsx!(
        div { class: "flex mb-4 justify-between items-center",
            div {
                h5 { class: "mb-0 font-medium text-xl",
                    "{name} {country}"
                }
                small {
                    img { class: "block w-8 h-8",
                        src: "http://openweathermap.org/img/wn/{icon}@2x.png",
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
    ))
}

fn week_weather<'a>(cx: Scope<'a, WeatherForecastProp<'a>>) -> Element {
    let forecast = cx.props.forecast;
    let high_low = forecast.get_high_low();
    cx.render(rsx!(
        div { class: "divider table mx-2 text-center bg-transparent whitespace-nowrap",
            span { class: "inline-block px-3", small { "Forecast" } }
        }
        div { class: "px-6 py-6 relative",
            div { class: "text-center justify-between items-center flex",
                style: "flex-flow: initial;",
                high_low.iter().map(|(d, (h, l, r, s, i))| {
                    let weekday = StackString::from_display(d.format("%a"));
                    let low = l.fahrenheit();
                    let high = h.fahrenheit();
                    let mut rain = StackString::new();
                    let mut snow = StackString::new();
                    if r.millimeters() > 0.0 {
                        rain = format_sstr!("R {:0.1}\"", r.inches());
                    }
                    if s.millimeters() > 0.0 {
                        snow = format_sstr!("S {:0.1}\"", s.inches());
                    }
                    let mut icon = StackString::new();
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
                                src: "http://openweathermap.org/img/wn/{icon}@2x.png",
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
    ))
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
