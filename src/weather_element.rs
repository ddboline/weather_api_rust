use dioxus::prelude::{
    dioxus_elements, format_args_f, rsx, Element, LazyNodes, NodeFactory, Scope, VNode,
};
use maplit::hashmap;
use weather_util_rust::{
    precipitation::Precipitation,
    weather_data::WeatherData,
    weather_forecast::WeatherForecast,
};
use time::{macros::format_description, UtcOffset};
use stack_string::StackString;

use crate::{app::AppState, errors::ServiceError as Error};


pub struct AppProps {
    weather: WeatherData,
    forecast: Option<WeatherForecast>,
    plot: Option<Vec<String>>,
}

impl AppProps {
    #[must_use]
    pub fn new(weather: WeatherData, forecast: WeatherForecast) -> Self {
        Self {
            weather,
            forecast: Some(forecast),
            plot: None,
        }
    }

    #[must_use]
    pub fn new_plot(weather: WeatherData, plot: Vec<String>) -> Self {
        Self {
            weather,
            forecast: None,
            plot: Some(plot),
        }
    }
}

#[must_use]
pub fn weather_element(cx: Scope<AppProps>) -> Element {
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

/// # Errors
/// Returns error if there is a syntax or parsing error
pub fn get_forecast_plots(forecast: &WeatherForecast, data: &AppState) -> Result<Vec<String>, Error> {
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
