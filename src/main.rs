use anyhow::Error;

use weather_api_rust::app::start_app;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    tokio::spawn(async move { start_app().await })
        .await
        .unwrap()
}
