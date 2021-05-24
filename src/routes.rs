use cached::{proc_macro::cached, Cached, TimedSizedCache};
use chrono::FixedOffset;
use handlebars::Handlebars;
use lazy_static::lazy_static;
use maplit::hashmap;
use rweb::{get, Query, Rejection, Reply};
use stack_string::StackString;

use weather_util_rust::{
    precipitation::Precipitation,
    weather_api::{WeatherApi, WeatherLocation},
    weather_data::WeatherData,
    weather_forecast::WeatherForecast,
};

use crate::{api_options::ApiOptions, app::AppState, errors::ServiceError as Error};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

lazy_static! {
    static ref HBR: Handlebars<'static> = get_templates().expect("Failed to register templates");
}

fn get_templates() -> Result<Handlebars<'static>, Error> {
    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string("ts", include_str!("../templates/TIMESERIESTEMPLATE.js.hbr"))?;
    handlebars
        .register_template_string("ht", include_str!("../templates/PLOT_TEMPLATE.html.hbr"))?;
    Ok(handlebars)
}

#[cached(
    type = "TimedSizedCache<StackString, WeatherData>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format!("{:?}", loc).into() }"#,
    result = true
)]
async fn get_weather_data(api: &WeatherApi, loc: &WeatherLocation) -> Result<WeatherData, Error> {
    api.get_weather_data(loc).await.map_err(Into::into)
}

#[cached(
    type = "TimedSizedCache<StackString, WeatherForecast>",
    create = "{ TimedSizedCache::with_size_and_lifespan(100, 3600) }",
    convert = r#"{ format!("{:?}", loc).into() }"#,
    result = true
)]
async fn get_weather_forecast(
    api: &WeatherApi,
    loc: &WeatherLocation,
) -> Result<WeatherForecast, Error> {
    api.get_weather_forecast(loc).await.map_err(Into::into)
}

#[get("/index.html")]
pub async fn frontpage(#[data] data: AppState, query: Query<ApiOptions>) -> WarpResult<impl Reply> {
    let body = frontpage_body(data, query.into_inner()).await?;
    Ok(rweb::reply::html(body))
}

async fn frontpage_body(data: AppState, query: ApiOptions) -> HttpResult<String> {
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location(&data.config)?;

    let weather_data = get_weather_data(&api, &loc).await?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;

    let weather_data = weather_data.get_current_conditions()?;
    let weather_forecast = weather_forecast.get_forecast()?;

    let lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let rows = lines.len() + 5;
    let body = format!(
        "<textarea rows={} cols={}>{}</textarea>",
        rows,
        cols,
        lines.join("\n")
    );

    let cols = weather_forecast.iter().map(|x| x.len()).max().unwrap_or(0) + 10;
    let body = format!(
        "{}<textarea rows={} cols={}>{}</textarea>",
        body,
        rows,
        cols,
        lines.join("\n")
    );
    Ok(body)
}

#[get("/plot.html")]
pub async fn forecast_plot(
    #[data] data: AppState,
    query: Query<ApiOptions>,
) -> WarpResult<impl Reply> {
    let body = forecast_plot_body(data, query.into_inner()).await?;
    Ok(rweb::reply::html(body))
}

async fn forecast_plot_body(data: AppState, query: ApiOptions) -> HttpResult<String> {
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location(&data.config)?;

    let weather_data = get_weather_data(&api, &loc).await?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;

    let weather_data = weather_data.get_current_conditions()?;

    let lines: Vec<_> = weather_data.split('\n').map(str::trim_end).collect();
    let cols = lines.iter().map(|x| x.len()).max().unwrap_or(0) + 5;
    let rows = lines.len() + 5;
    let body = format!(
        "<textarea rows={} cols={}>{}</textarea>",
        rows,
        cols,
        lines.join("\n")
    );

    let fo: FixedOffset = weather_forecast.city.timezone.into();
    let data: Vec<_> = weather_forecast
        .list
        .iter()
        .map(|entry| {
            (
                entry
                    .dt
                    .with_timezone(&fo)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
                entry.main.temp.fahrenheit(),
            )
        })
        .collect();

    let js_str = serde_json::to_string(&data).unwrap_or_else(|_| "".to_string());

    let params = hashmap! {
        "DATA" => js_str.as_str(),
        "YAXIS" => "F",
        "XAXIS" => "",
        "EXAMPLETITLE" => "Temperature Forecast",
        "NAME" => "temperature_forecast",
    };

    let body = format!("{}<br>{}", body, HBR.render("ts", &params)?);

    let data: Vec<_> = weather_forecast
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
            (
                entry
                    .dt
                    .with_timezone(&fo)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
                (rain + snow).inches(),
            )
        })
        .collect();

    let js_str = serde_json::to_string(&data).unwrap_or_else(|_| "".to_string());

    let params = hashmap! {
        "DATA"=> js_str.as_str(),
        "YAXIS"=> "in",
        "XAXIS"=> "",
        "EXAMPLETITLE"=> "Precipitation Forecast",
        "NAME"=> "precipitation_forecast",
    };

    let body = format!("{}<br>{}", body, HBR.render("ts", &params)?);

    Ok(HBR.render("ht", &hashmap! {"INSERTOTHERIMAGESHERE" => &body})?)
}

#[get("/statistics")]
pub async fn statistics() -> WarpResult<impl Reply> {
    let body = statistics_body().await?;
    Ok(rweb::reply::html(body))
}

async fn statistics_body() -> HttpResult<String> {
    let data_cache = GET_WEATHER_DATA.lock().await;
    let forecast_cache = GET_WEATHER_FORECAST.lock().await;
    let body = format!(
        "data hits {}, misses {} : forecast hits {}, misses {}",
        data_cache.cache_hits().unwrap_or(0),
        data_cache.cache_misses().unwrap_or(0),
        forecast_cache.cache_hits().unwrap_or(0),
        forecast_cache.cache_misses().unwrap_or(0)
    );
    Ok(body)
}

#[get("/weather")]
pub async fn weather(#[data] data: AppState, query: Query<ApiOptions>) -> WarpResult<impl Reply> {
    let weather_data = weather_json(data, query.into_inner()).await?;
    Ok(rweb::reply::json(&weather_data))
}

async fn weather_json(data: AppState, query: ApiOptions) -> HttpResult<WeatherData> {
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location(&data.config)?;
    let weather_data = get_weather_data(&api, &loc).await?;
    Ok(weather_data)
}

#[get("/forecast")]
pub async fn forecast(#[data] data: AppState, query: Query<ApiOptions>) -> WarpResult<impl Reply> {
    let weather_forecast = forecast_body(data, query.into_inner()).await?;
    Ok(rweb::reply::json(&weather_forecast))
}

async fn forecast_body(data: AppState, query: ApiOptions) -> HttpResult<WeatherForecast> {
    let api = query.get_weather_api(&data.api)?;
    let loc = query.get_weather_location(&data.config)?;
    let weather_forecast = get_weather_forecast(&api, &loc).await?;
    Ok(weather_forecast)
}
