use cached::{proc_macro::cached, Cached, TimedSizedCache};
use dioxus::prelude::{
    dioxus_elements, format_args_f, rsx, Element, LazyNodes, NodeFactory, Scope, VNode, VirtualDom,
};
use handlebars::Handlebars;
use lazy_static::lazy_static;
use maplit::hashmap;
use rweb::{get, Query, Rejection, Schema};
use serde::{Deserialize, Serialize};
use stack_string::{format_sstr, StackString};
use std::collections::HashMap;
use time::{macros::format_description, UtcOffset};
use tokio::sync::RwLock;

use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, RwebResponse,
};
use weather_util_rust::{
    precipitation::Precipitation,
    weather_api::{WeatherApi, WeatherLocation},
    weather_data::WeatherData,
    weather_forecast::WeatherForecast,
};

use crate::{
    api_options::ApiOptions, app::AppState, errors::ServiceError as Error, WeatherDataWrapper,
    WeatherForecastWrapper,
};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

lazy_static! {
    static ref WEATHER_STRING_LENGTH: StringLengthMap = StringLengthMap::new();
}

struct StringLengthMap(RwLock<HashMap<StackString, usize>>);

impl StringLengthMap {
    fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }

    async fn insert_lenth(&self, key: &str, length: usize) {
        let current_max = self.0.read().await.get(key).map_or(0, |x| *x);
        if length > current_max {
            self.0.write().await.insert(key.into(), length);
        }
    }

    async fn get_map(&self) -> HashMap<String, usize> {
        self.0
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.into(), *v))
            .collect()
    }
}

/// # Errors
/// Returns error if there is a syntax or parsing error
pub fn get_templates() -> Result<Handlebars<'static>, Error> {
    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string("ts", include_str!("../templates/TIMESERIESTEMPLATE.js.hbr"))?;
    Ok(handlebars)
}

#[cached(
    type = "TimedSizedCache<StackString, WeatherData>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format_sstr!("{:?}", loc) }"#,
    result = true
)]
async fn get_weather_data(api: &WeatherApi, loc: &WeatherLocation) -> Result<WeatherData, Error> {
    api.get_weather_data(loc).await.map_err(Into::into)
}

#[cached(
    type = "TimedSizedCache<StackString, WeatherForecast>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format_sstr!("{:?}", loc) }"#,
    result = true
)]
async fn get_weather_forecast(
    api: &WeatherApi,
    loc: &WeatherLocation,
) -> Result<WeatherForecast, Error> {
    api.get_weather_forecast(loc).await.map_err(Into::into)
}

#[derive(RwebResponse)]
#[response(description = "Display Current Weather and Forecast", content = "html")]
struct IndexResponse(HtmlBase<StackString, Error>);

#[get("/weather/index.html")]
pub async fn frontpage(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<IndexResponse> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let weather = get_weather_data(&api, &loc).await?;
    let forecast = get_weather_forecast(&api, &loc).await?;

    let body = {
        let mut app = VirtualDom::new_with_props(
            weather_element,
            AppProps {
                weather,
                forecast: Some(forecast),
                plot: None,
            },
        );
        app.rebuild();
        dioxus::ssr::render_vdom(&app)
    };
    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/index.html", body.len())
        .await;
    Ok(HtmlBase::new(body.into()).into())
}

#[derive(RwebResponse)]
#[response(
    description = "Show Plot of Current Weather and Forecast",
    content = "html"
)]
struct WeatherPlotResponse(HtmlBase<String, Error>);

#[get("/weather/plot.html")]
pub async fn forecast_plot(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<WeatherPlotResponse> {
    let query = query.into_inner();
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;

    let weather = get_weather_data(&api, &loc).await?;
    let forecast = get_weather_forecast(&api, &loc).await?;

    let plots = get_forecast_plots(&forecast, &data)?;

    let body = {
        let mut app = VirtualDom::new_with_props(
            weather_element,
            AppProps {
                weather,
                forecast: None,
                plot: Some(plots),
            },
        );
        app.rebuild();
        dioxus::ssr::render_vdom(&app)
    };

    WEATHER_STRING_LENGTH
        .insert_lenth("/weather/plot.html", body.len())
        .await;
    Ok(HtmlBase::new(body).into())
}

#[derive(Serialize, Deserialize, Schema, Clone)]
pub struct StatisticsObject {
    pub data_cache_hits: u64,
    pub data_cache_misses: u64,
    pub forecast_cache_hits: u64,
    pub forecast_cache_misses: u64,
    pub weather_string_length_map: HashMap<String, usize>,
}

#[derive(RwebResponse)]
#[response(description = "Get Cache Statistics")]
struct StatisticsResponse(JsonBase<StatisticsObject, Error>);

#[get("/weather/statistics")]
pub async fn statistics() -> WarpResult<StatisticsResponse> {
    let data_cache = GET_WEATHER_DATA.lock().await;
    let forecast_cache = GET_WEATHER_FORECAST.lock().await;
    let weather_string_length_map = WEATHER_STRING_LENGTH.get_map().await;

    let stat = StatisticsObject {
        data_cache_hits: data_cache.cache_hits().unwrap_or(0),
        data_cache_misses: data_cache.cache_misses().unwrap_or(0),
        forecast_cache_hits: forecast_cache.cache_hits().unwrap_or(0),
        forecast_cache_misses: forecast_cache.cache_misses().unwrap_or(0),
        weather_string_length_map,
    };

    Ok(JsonBase::new(stat).into())
}

#[derive(RwebResponse)]
#[response(description = "Get WeatherData Api Json")]
struct WeatherResponse(JsonBase<WeatherDataWrapper, Error>);

#[get("/weather/weather")]
pub async fn weather(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<WeatherResponse> {
    let weather_data = weather_json(data, query.into_inner()).await?.into();
    Ok(JsonBase::new(weather_data).into())
}

async fn weather_json(data: AppState, query: ApiOptions) -> HttpResult<WeatherData> {
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;
    let weather_data = get_weather_data(&api, &loc).await?;
    Ok(weather_data)
}

#[derive(RwebResponse)]
#[response(description = "Get WeatherForecast Api Json")]
struct ForecastResponse(JsonBase<WeatherForecastWrapper, Error>);

#[get("/weather/forecast")]
pub async fn forecast(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<ForecastResponse> {
    let weather_forecast = forecast_body(data, query.into_inner()).await?.into();
    Ok(JsonBase::new(weather_forecast).into())
}

async fn forecast_body(data: AppState, query: ApiOptions) -> HttpResult<WeatherForecast> {
    let api = query.get_weather_api(&data.api);
    let loc = query.get_weather_location(&data.config)?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;
    Ok(weather_forecast)
}

struct AppProps {
    weather: WeatherData,
    forecast: Option<WeatherForecast>,
    plot: Option<Vec<String>>,
}

fn weather_element(cx: Scope<AppProps>) -> Element {
    let weather_data = cx.props.weather.get_current_conditions();
    let weather_lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let weather_cols = weather_lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let weather_rows = weather_lines.len() + 5;
    let weather_lines = weather_lines.join("\n");

    let forecast_lines = cx.props.forecast.as_ref().map(|forecast| {
        let weather_forecast = forecast.get_forecast();
        let forecast_lines: Vec<_> = weather_forecast.iter().map(|s| s.trim_end()).collect();
        let forecast_cols = forecast_lines.iter().map(|x| x.len()).max().unwrap_or(0) + 10;
        (forecast_cols, forecast_lines.join("\n"))
    });

    cx.render(rsx!(
        head {
            title: "Scale Measurement Plots",
            style {
                [include_str!("../templates/style.css")],
            }
        },
        body {
            cx.props.plot.as_ref().map(|_| {
                rsx! {
                    script {
                        src: "https://d3js.org/d3.v4.min.js",
                    }
                }
            })
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
            }
            cx.props.plot.as_ref().map(|plots| {
                rsx! {
                    br {},
                    plots.iter().enumerate().map(|(idx, ts)| {
                        rsx! {
                            script {
                                key: "forecast-plot-key-{idx}",
                                "{ts}"
                            }
                        }
                    })
                }
            })
        }
    ))
}

fn get_forecast_plots(forecast: &WeatherForecast, data: &AppState) -> Result<Vec<String>, Error> {
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

    let js_str = serde_json::to_string(&forecast_data).map_err(Into::<Error>::into)?;

    let params = hashmap! {
        "DATA" => js_str.as_str(),
        "YAXIS" => "F",
        "XAXIS" => "",
        "EXAMPLETITLE" => "Temperature Forecast",
        "NAME" => "temperature_forecast",
    };
    let ts = data
        .hbr
        .render("ts", &params)
        .map_err(Into::<Error>::into)?;
    plots.push(ts);

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

    let js_str = serde_json::to_string(&forecast_data).map_err(Into::<Error>::into)?;

    let params = hashmap! {
        "DATA"=> js_str.as_str(),
        "YAXIS"=> "in",
        "XAXIS"=> "",
        "EXAMPLETITLE"=> "Precipitation Forecast",
        "NAME"=> "precipitation_forecast",
    };
    let ts = data
        .hbr
        .render("ts", &params)
        .map_err(Into::<Error>::into)?;
    plots.push(ts);

    Ok(plots)
}
