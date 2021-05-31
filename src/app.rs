use anyhow::Error;
use handlebars::Handlebars;
use rweb::Filter;
use rweb::{
    filters::BoxedFilter,
    Reply, openapi::{self, Spec},
    http::{StatusCode, header::CONTENT_TYPE},
};
use maplit::hashmap;
use std::borrow::Cow;
use std::{net::SocketAddr, sync::Arc};

use weather_util_rust::weather_api::WeatherApi;

use super::{
    config::Config,
    errors::error_response,
    routes::{forecast, forecast_plot, frontpage, get_templates, statistics, weather},
};

#[derive(Clone)]
pub struct AppState {
    pub api: Arc<WeatherApi>,
    pub config: Config,
    pub hbr: Arc<Handlebars<'static>>,
}

pub async fn start_app() -> Result<(), Error> {
    let config = Config::init_config()?;

    let port = config.port;
    run_app(&config, port).await
}

fn get_api_scope(app: &AppState) -> BoxedFilter<(impl Reply,)> {
    let frontpage_path = frontpage(app.clone());
    let forecast_plot_path = forecast_plot(app.clone());
    let weather_path = weather(app.clone());
    let forecast_path = forecast(app.clone());
    let statistics_path = statistics();

    frontpage_path.or(forecast_plot_path).or(weather_path).or(forecast_path).or(statistics_path).boxed()
}

fn modify_spec(spec: &mut Spec) {
    spec.info.title = "Weather App".into();
    spec.info.description = "Web App to disply weather from openweatherapi"
        .into();
    spec.info.version = env!("CARGO_PKG_VERSION").into();

    let response_descriptions = hashmap! {
        ("/weather/index.html", "get", StatusCode::OK) => "Display Current Weather and Forecast",
        ("/weather/plot.html", "get", StatusCode::OK) => "Show Plot of Current Weather and Forecast",
        ("/weather/statistics", "get", StatusCode::OK) => "Get Cache Statistics",
        ("/weather/weather", "get", StatusCode::OK) => "Get WeatherData Api Json",
        ("/weather/forecast", "get", StatusCode::OK) => "Get WeatherForecast Api Json",
    };

    for ((path, method, code), description) in response_descriptions {
        let code: Cow<'static, str> = code.as_u16().to_string().into();
        if let Some(path) = spec.paths.get_mut(path) {
            if let Some(method) = match method {
                "get" => path.get.as_mut(),
                "patch" => path.patch.as_mut(),
                "post" => path.post.as_mut(),
                "delete" => path.delete.as_mut(),
                _ => panic!("Unsupported"),
            } {
                if let Some(resp) = method.responses.get_mut(&code) {
                    resp.description = description.into();
                }
            }
        }
    }
}

async fn run_app(config: &Config, port: u32) -> Result<(), Error> {
    let app = AppState {
        api: Arc::new(WeatherApi::new(
            &config.api_key,
            &config.api_endpoint,
            &config.api_path,
        )),
        config: config.clone(),
        hbr: Arc::new(get_templates()?),
    };

    let (mut spec, api_scope) = openapi::spec().build(|| get_api_scope(&app));
    modify_spec(&mut spec);
    let spec = Arc::new(spec);
    let spec_json_path = rweb::path!("weather" / "openapi" / "json")
        .and(rweb::path::end())
        .map({
            let spec = spec.clone();
            move || rweb::reply::json(spec.as_ref())
        });
    let spec_yaml = serde_yaml::to_string(spec.as_ref())?;
    let spec_yaml_path = rweb::path!("weather" / "openapi" / "yaml")
        .and(rweb::path::end())
        .map(move || {
            let reply = rweb::reply::html(spec_yaml.clone());
            rweb::reply::with_header(reply, CONTENT_TYPE, "text/yaml")
        });

    let cors = rweb::cors()
        .allow_methods(vec!["GET"])
        .allow_header("content-type")
        .allow_any_origin()
        .build();

    let routes = api_scope.or(spec_json_path).or(spec_yaml_path)
        .recover(error_response)
        .with(cors);

    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    rweb::serve(routes).bind(addr).await;

    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use chrono::{offset::FixedOffset, Offset};
    use chrono_tz::US::Central;
    use std::convert::TryInto;

    use weather_util_rust::{weather_data::WeatherData, weather_forecast::WeatherForecast};

    use crate::{app::run_app, config::Config};

    #[tokio::test]
    async fn test_run_app() -> Result<(), Error> {
        let config = Config::init_config()?;
        let test_port = 12345;
        tokio::task::spawn(async move {
            env_logger::init();
            run_app(&config, test_port).await.unwrap()
        });
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let url = format!("http://localhost:{}/weather/weather?zip=55427", test_port);
        let weather: WeatherData = reqwest::get(&url).await?.error_for_status()?.json().await?;
        assert_eq!(weather.name.as_str(), "Minneapolis");

        let url = format!("http://localhost:{}/weather/forecast?zip=55427", test_port);
        let forecast: WeatherForecast =
            reqwest::get(&url).await?.error_for_status()?.json().await?;
        println!("{:?}", forecast);
        assert_eq!(forecast.list.len(), 40);
        let city_offset: FixedOffset = forecast.city.timezone.into();

        let local = weather.dt.with_timezone(&Central);
        let expected_offset: FixedOffset = local.offset().fix();
        assert_eq!(city_offset, expected_offset);

        let url = format!(
            "http://localhost:{}/weather/index.html?zip=55427",
            test_port
        );
        let text = reqwest::get(&url).await?.error_for_status()?.text().await?;
        println!("{}", text);
        assert!(text.len() > 0);

        let url = format!("http://localhost:{}/weather/plot.html?zip=55427", test_port);
        let text = reqwest::get(&url).await?.error_for_status()?.text().await?;
        println!("{}", text);
        assert!(text.len() > 0);

        let url = format!("http://localhost:{}/weather/statistics", test_port);
        let text = reqwest::get(&url).await?.error_for_status()?.text().await?;
        assert!(text.len() > 0);
        assert!(text.contains("data hits"));
        assert!(text.contains("misses"));
        assert!(text.contains("forecast hits"));

        let url = format!(
            "http://localhost:{}/weather/weather?q=Minneapolis",
            test_port
        );
        let weather: WeatherData = reqwest::get(&url).await?.error_for_status()?.json().await?;
        assert_eq!(weather.name.as_str(), "Minneapolis");

        let url = format!("http://localhost:{}/weather/weather?lat=0&lon=0", test_port);
        let weather: WeatherData = reqwest::get(&url).await?.error_for_status()?.json().await?;
        assert_eq!(weather.coord.lat, 0.0.try_into()?);
        assert_eq!(weather.coord.lon, 0.0.try_into()?);

        Ok(())
    }
}
