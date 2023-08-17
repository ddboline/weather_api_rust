use anyhow::Error;
use clap::Parser;
use futures::{future::try_join_all, TryStreamExt};
use refinery::embed_migrations;
use rweb_helper::DateType;
use stack_string::StackString;
use std::path::PathBuf;
use time::{macros::format_description, Date};
use tokio::{
    fs::{read, File},
    io::{stdin, stdout, AsyncReadExt, AsyncWrite, AsyncWriteExt},
};

use crate::{
    app::start_app, config::Config, pgpool::PgPool, polars_analysis::insert_db_into_parquet,
    WeatherDataDB,
};

embed_migrations!("migrations");

fn parse_date_from_str(s: &str) -> Result<DateType, String> {
    Date::parse(s, format_description!("[year]-[month]-[day]"))
        .map(Into::into)
        .map_err(|e| format!("{e}"))
}

#[derive(Parser, Debug)]
pub enum ParseOpts {
    /// Run migrations
    RunMigrations,
    /// Run daemon
    Daemon,
    /// Import into history
    Import {
        #[clap(short, long)]
        /// Input file (if missinge will read from stdin)
        filepath: Option<PathBuf>,
        #[clap(short, long)]
        table: Option<StackString>,
    },
    /// Export history
    Export {
        #[clap(short, long)]
        server: Option<StackString>,
        #[clap(short='b', long, value_parser=parse_date_from_str)]
        /// Start date
        start_time: Option<DateType>,
        #[clap(short, long, value_parser=parse_date_from_str)]
        /// End date
        end_time: Option<DateType>,
        #[clap(short, long)]
        /// Output file (if missinge will read from stdin)
        filepath: Option<PathBuf>,
        #[clap(short, long)]
        table: Option<StackString>,
    },
    /// Export DB data into parquet files
    Db {
        #[clap(short = 'd', long = "directory")]
        directory: Option<PathBuf>,
    },
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
            Self::Import { filepath, table: _ } => {
                let db_url = config.database_url.as_ref().unwrap();
                let pool = PgPool::new(db_url);

                let data = if let Some(filepath) = filepath {
                    read(&filepath).await?
                } else {
                    let mut stdin = stdin();
                    let mut buf = Vec::new();
                    stdin.read_to_end(&mut buf).await?;
                    buf
                };
                let history: Vec<WeatherDataDB> = serde_json::from_slice(&data)?;
                let futures = history.into_iter().map(|entry| {
                    let pool = pool.clone();
                    async move { entry.insert(&pool).await.map_err(Into::<Error>::into) }
                });
                let results: Result<Vec<u64>, Error> = try_join_all(futures).await;
                let written: u64 = results?.into_iter().sum();
                println!("written {written}");
            }
            Self::Export {
                server,
                start_time,
                end_time,
                filepath,
                table: _,
            } => {
                let db_url = config.database_url.as_ref().unwrap();
                let pool = PgPool::new(db_url);
                let results: Vec<_> = WeatherDataDB::get_by_name_dates(
                    &pool,
                    None,
                    server.as_ref().map(StackString::as_str),
                    start_time.map(Into::into),
                    end_time.map(Into::into),
                )
                .await?
                .try_collect()
                .await?;

                let mut file: Box<dyn AsyncWrite + Unpin + Send + Sync> =
                    if let Some(filepath) = filepath {
                        Box::new(File::create(&filepath).await?)
                    } else {
                        Box::new(stdout())
                    };

                file.write_all(&serde_json::to_vec(&results)?).await?;
            }
            Self::Db { directory } => {
                let directory = directory.unwrap_or_else(|| config.cache_dir.clone());
                let db_url = config.database_url.as_ref().unwrap();
                let pool = PgPool::new(db_url);
                stdout()
                    .write(
                        insert_db_into_parquet(&pool, &directory)
                            .await?
                            .join("\n")
                            .as_bytes(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}
