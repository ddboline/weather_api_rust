use anyhow::Error;
use clap::Parser;
use refinery::embed_migrations;

use crate::{app::start_app, config::Config, pgpool::PgPool};

embed_migrations!("migrations");

#[derive(Parser, Debug)]
pub enum ParseOpts {
    /// Run migrations
    RunMigrations,
    /// Run daemon
    Daemon,
}

impl ParseOpts {
    /// # Errors
    /// Return error if db query fails
    /// # Panics
    /// Panics if no db url when calling run migrations
    pub async fn process_args() -> Result<(), Error> {
        let opts = ParseOpts::parse();
        let config = Config::init_config(None)?;

        match opts {
            Self::RunMigrations => {
                let db_url = config.database_url.as_ref().unwrap();
                let pool = PgPool::new(db_url);
                let mut client = pool.get().await?;
                migrations::runner().run_async(&mut **client).await?;
            }
            Self::Daemon => {
                tokio::spawn(async move { start_app().await }).await??;
            }
        }

        Ok(())
    }
}
