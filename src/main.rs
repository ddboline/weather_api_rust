use weather_api_rust::app::start_app;

#[actix_web::main]
async fn main() {
    start_app().await
}
