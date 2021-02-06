use anyhow::Error;

use weather_api_rust::app::start_app;

#[tokio::main]
async fn main() -> Result<(), Error> {
    start_app().await
}
