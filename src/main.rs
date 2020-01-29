use weather_api_rust::app::start_app;

#[actix_rt::main]
async fn main() {
    start_app().await
}
