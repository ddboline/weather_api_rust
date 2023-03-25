use anyhow::Error;

use weather_api_rust::parse_opts::ParseOpts;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    ParseOpts::process_args().await
}
