use anyhow::Error;

use weather_api_rust::app::start_app;

#[actix_web::main]
async fn main() -> Result<(), Error> {
    start_app().await
}
