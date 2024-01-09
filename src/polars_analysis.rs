use anyhow::{format_err, Error};
use chrono::NaiveDateTime;
use futures::TryStreamExt;
use log::{debug, info};
use polars::{
    df as dataframe,
    io::SerReader,
    prelude::{
        col, lit, DataFrame, LazyFrame, NamedFrom, ParquetReader, ParquetWriter, ScanArgsParquet,
        SortOptions, TimeUnit, UniqueKeepStrategy,
    },
};
use postgres_query::{query, FromSqlRow};
use stack_string::{format_sstr, StackString};
use std::{fs::File, path::Path};
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};
use uuid::Uuid;

use crate::{model::WeatherDataDB, pgpool::PgPool};

fn convert_offset_naive(input: OffsetDateTime) -> NaiveDateTime {
    let d: OffsetDateTime = input.to_offset(UtcOffset::UTC);
    NaiveDateTime::from_timestamp_opt(d.unix_timestamp(), d.nanosecond())
        .expect("Invalid timestamp")
}

fn convert_naive_offset(input: NaiveDateTime) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(input.timestamp()).expect("Invalid timestamp")
}

fn stackstring_to_series(col: &[StackString]) -> Vec<&str> {
    col.iter().map(StackString::as_str).collect()
}

struct WeatherDataColumns {
    id: Vec<StackString>,
    dt: Vec<i32>,
    created_at: Vec<NaiveDateTime>,
    location_name: Vec<StackString>,
    latitude: Vec<f64>,
    longitude: Vec<f64>,
    condition: Vec<StackString>,
    temperature: Vec<f64>,
    temperature_minimum: Vec<f64>,
    temperature_maximum: Vec<f64>,
    pressure: Vec<f64>,
    humidity: Vec<i32>,
    visibility: Vec<Option<f64>>,
    rain: Vec<Option<f64>>,
    snow: Vec<Option<f64>>,
    wind_speed: Vec<f64>,
    wind_direction: Vec<Option<f64>>,
    country: Vec<StackString>,
    sunrise: Vec<NaiveDateTime>,
    sunset: Vec<NaiveDateTime>,
    timezone: Vec<i32>,
    server: Vec<StackString>,
}

impl WeatherDataColumns {
    fn new(cap: usize) -> Self {
        Self {
            id: Vec::with_capacity(cap),
            dt: Vec::with_capacity(cap),
            created_at: Vec::with_capacity(cap),
            location_name: Vec::with_capacity(cap),
            latitude: Vec::with_capacity(cap),
            longitude: Vec::with_capacity(cap),
            condition: Vec::with_capacity(cap),
            temperature: Vec::with_capacity(cap),
            temperature_minimum: Vec::with_capacity(cap),
            temperature_maximum: Vec::with_capacity(cap),
            pressure: Vec::with_capacity(cap),
            humidity: Vec::with_capacity(cap),
            visibility: Vec::with_capacity(cap),
            rain: Vec::with_capacity(cap),
            snow: Vec::with_capacity(cap),
            wind_speed: Vec::with_capacity(cap),
            wind_direction: Vec::with_capacity(cap),
            country: Vec::with_capacity(cap),
            sunrise: Vec::with_capacity(cap),
            sunset: Vec::with_capacity(cap),
            timezone: Vec::with_capacity(cap),
            server: Vec::with_capacity(cap),
        }
    }

    fn add_row(&mut self, row: WeatherDataDB) {
        self.id.push(format_sstr!("{}", row.id));
        self.dt.push(row.dt);
        self.created_at
            .push(convert_offset_naive(row.created_at.into()));
        self.location_name.push(row.location_name);
        self.latitude.push(row.latitude);
        self.longitude.push(row.longitude);
        self.condition.push(row.condition);
        self.temperature.push(row.temperature);
        self.temperature_minimum.push(row.temperature_minimum);
        self.temperature_maximum.push(row.temperature_maximum);
        self.pressure.push(row.pressure);
        self.humidity.push(row.humidity);
        self.visibility.push(row.visibility);
        self.rain.push(row.rain);
        self.snow.push(row.snow);
        self.wind_speed.push(row.wind_speed);
        self.wind_direction.push(row.wind_direction);
        self.country.push(row.country);
        self.sunrise.push(convert_offset_naive(row.sunrise.into()));
        self.sunset.push(convert_offset_naive(row.sunset.into()));
        self.timezone.push(row.timezone);
        self.server.push(row.server);
    }

    fn get_dataframe(&self) -> Result<DataFrame, Error> {
        dataframe!(
            "id" => stackstring_to_series(&self.id),
            "dt" => &self.dt,
            "created_at" => &self.created_at,
            "location_name" => stackstring_to_series(&self.location_name),
            "latitude" => &self.latitude,
            "longitude" => &self.longitude,
            "condition" => stackstring_to_series(&self.condition),
            "temperature" => &self.temperature,
            "temperature_minimum" => &self.temperature_minimum,
            "temperature_maximum" => &self.temperature_maximum,
            "pressure" => &self.pressure,
            "humidity" => &self.humidity,
            "visibility" => &self.visibility,
            "rain" => &self.rain,
            "snow" => &self.snow,
            "wind_speed" => &self.wind_speed,
            "wind_direction" => &self.wind_direction,
            "country" => stackstring_to_series(&self.country),
            "sunrise" => &self.sunrise,
            "sunset" => &self.sunset,
            "timezone" => &self.timezone,
            "server" => stackstring_to_series(&self.server),
        )
        .map_err(Into::into)
    }

    fn into_weather_data(self) -> Vec<WeatherDataDB> {
        debug!("cap {}", self.id.len());
        let mut output = Vec::with_capacity(self.id.len());
        for i in 0..self.id.len() {
            output.push(WeatherDataDB {
                id: Uuid::parse_str(&self.id[i]).expect("Invalid uuid"),
                dt: self.dt[i],
                created_at: convert_naive_offset(self.created_at[i]).into(),
                location_name: self.location_name[i].clone(),
                latitude: self.latitude[i],
                longitude: self.longitude[i],
                condition: self.condition[i].clone(),
                temperature: self.temperature[i],
                temperature_minimum: self.temperature_minimum[i],
                temperature_maximum: self.temperature_maximum[i],
                pressure: self.pressure[i],
                humidity: self.humidity[i],
                visibility: self.visibility[i],
                rain: self.rain[i],
                snow: self.snow[i],
                wind_speed: self.wind_speed[i],
                wind_direction: self.wind_direction[i],
                country: self.country[i].clone(),
                sunrise: convert_naive_offset(self.sunrise[i]).into(),
                sunset: convert_naive_offset(self.sunset[i]).into(),
                timezone: self.timezone[i],
                server: self.server[i].clone(),
            });
        }
        debug!("output {}", output.len());
        output
    }
}

/// # Errors
/// Returns error if db query fails
pub async fn insert_db_into_parquet(
    pool: &PgPool,
    outdir: &Path,
) -> Result<Vec<StackString>, Error> {
    #[derive(FromSqlRow)]
    struct Wrap {
        year: i32,
        month: i32,
        count: i64,
    }

    let mut output = Vec::new();

    let query = query!(
        r#"
            SELECT cast(extract(year from created_at at time zone 'utc') as int) as year,
                   cast(extract(month from created_at at time zone 'utc') as int) as month,
                   count(*) as count
            FROM weather_data
            GROUP BY 1,2
            ORDER BY 1,2
        "#
    );
    let conn = pool.get().await?;
    let rows: Vec<Wrap> = query.fetch(&conn).await?;
    if rows.is_empty() {
        return Ok(output);
    }

    for Wrap { year, month, count } in rows {
        let query = query!(
            r#"
                SELECT *
                FROM weather_data
                WHERE cast(extract(year from created_at at time zone 'utc') as int) = $year
                  AND cast(extract(month from created_at at time zone 'utc') as int) = $month
            "#,
            year = year,
            month = month,
        );

        let weather_rows: WeatherDataColumns = query
            .fetch_streaming::<WeatherDataDB, _>(&conn)
            .await?
            .try_fold(
                WeatherDataColumns::new(count as usize),
                |mut acc, row| async move {
                    acc.add_row(row);
                    Ok(acc)
                },
            )
            .await?;

        let new_df = weather_rows.get_dataframe()?;
        output.push(format_sstr!("{:?}", new_df.shape()));

        let filename = format_sstr!("weather_data_{year:04}_{month:02}.parquet");
        let file = outdir.join(&filename);
        let mut df = if file.exists() {
            let df = ParquetReader::new(File::open(&file)?).finish()?;
            output.push(format_sstr!("{:?}", df.shape()));
            let existing_entries = df.shape().0;
            let combined_df = df
                .vstack(&new_df)?
                .unique(None, UniqueKeepStrategy::First, None)?;
            if combined_df.shape().0 == existing_entries {
                continue;
            }
            combined_df
        } else {
            new_df
        };
        ParquetWriter::new(File::create(&file)?).finish(&mut df)?;
        output.push(format_sstr!("wrote {filename} {:?}", df.shape()));
    }

    Ok(output)
}

/// # Errors
/// Returns error if input/output doesn't exist or cannot be read
pub fn merge_parquet_files(input: &Path, output: &Path) -> Result<(), Error> {
    info!("input {:?} output {:?}", input, output);
    if !input.exists() {
        return Err(format_err!("input {input:?} does not exist"));
    }
    if !output.exists() {
        return Err(format_err!("output {output:?} does not exist"));
    }
    let df0 = ParquetReader::new(File::open(input)?).finish()?;
    let entries0 = df0.shape().0;
    info!("input {entries0}");
    let df1 = ParquetReader::new(File::open(output)?).finish()?;
    let entries1 = df1.shape().0;
    info!("output {entries1}");

    if entries0 == 0 {
        return Ok(());
    }

    let mut df = df1
        .vstack(&df0)?
        .unique(None, UniqueKeepStrategy::First, None)?;
    info!("final {:?}", df.shape());
    ParquetWriter::new(File::create(output)?).finish(&mut df)?;
    info!("wrote {:?} {:?}", output, df.shape());
    Ok(())
}

/// # Errors
/// Returns error if path does not exist
pub async fn get_by_name_dates(
    input: &Path,
    name: Option<&str>,
    server: Option<&str>,
    start_date: Option<Date>,
    end_date: Option<Date>,
) -> Result<Vec<WeatherDataDB>, Error> {
    if !input.exists() {
        return Err(format_err!("Path does not exist"));
    }
    let input_files = if input.is_dir() {
        let v: Result<Vec<_>, Error> = input
            .read_dir()?
            .map(|p| p.map(|p| p.path()).map_err(Into::into))
            .collect();
        let mut v = v?;
        v.sort();
        v
    } else {
        vec![input.to_path_buf()]
    };
    debug!("{input_files:?}");
    let mut output = Vec::new();
    for input_file in input_files {
        let df = get_by_name_dates_file(&input_file, name, server, start_date, end_date).await?;
        debug!("df {input_file:?} {:?}", df.shape());
        let columns = WeatherDataColumns {
            id: df
                .column("id")?
                .str()?
                .into_iter()
                .filter_map(|i| i.map(Into::into))
                .collect(),
            dt: df.column("dt")?.i32()?.into_iter().flatten().collect(),
            created_at: df
                .column("created_at")?
                .datetime()?
                .into_iter()
                .filter_map(|t| t.and_then(NaiveDateTime::from_timestamp_millis))
                .collect(),
            location_name: df
                .column("location_name")?
                .str()?
                .into_iter()
                .filter_map(|i| i.map(Into::into))
                .collect(),
            latitude: df
                .column("latitude")?
                .f64()?
                .into_iter()
                .flatten()
                .collect(),
            longitude: df
                .column("longitude")?
                .f64()?
                .into_iter()
                .flatten()
                .collect(),
            condition: df
                .column("condition")?
                .str()?
                .into_iter()
                .filter_map(|i| i.map(Into::into))
                .collect(),
            temperature: df
                .column("temperature")?
                .f64()?
                .into_iter()
                .flatten()
                .collect(),
            temperature_minimum: df
                .column("temperature_minimum")?
                .f64()?
                .into_iter()
                .flatten()
                .collect(),
            temperature_maximum: df
                .column("temperature_maximum")?
                .f64()?
                .into_iter()
                .flatten()
                .collect(),
            pressure: df
                .column("pressure")?
                .f64()?
                .into_iter()
                .flatten()
                .collect(),
            humidity: df
                .column("humidity")?
                .i32()?
                .into_iter()
                .flatten()
                .collect(),
            visibility: df.column("visibility")?.f64()?.into_iter().collect(),
            rain: df.column("rain")?.f64()?.into_iter().collect(),
            snow: df.column("snow")?.f64()?.into_iter().collect(),
            wind_speed: df
                .column("wind_speed")?
                .f64()?
                .into_iter()
                .flatten()
                .collect(),
            wind_direction: df.column("wind_direction")?.f64()?.into_iter().collect(),
            country: df
                .column("country")?
                .str()?
                .into_iter()
                .filter_map(|i| i.map(Into::into))
                .collect(),
            sunrise: df
                .column("sunrise")?
                .datetime()?
                .into_iter()
                .filter_map(|t| t.and_then(NaiveDateTime::from_timestamp_millis))
                .collect(),
            sunset: df
                .column("sunset")?
                .datetime()?
                .into_iter()
                .filter_map(|t| t.and_then(NaiveDateTime::from_timestamp_millis))
                .collect(),
            timezone: df
                .column("timezone")?
                .i32()?
                .into_iter()
                .flatten()
                .collect(),
            server: df
                .column("server")?
                .str()?
                .into_iter()
                .filter_map(|i| i.map(Into::into))
                .collect(),
        };
        let rows = columns.into_weather_data();
        debug!("rows {}", rows.len());
        output.extend(rows);
    }
    Ok(output)
}

async fn get_by_name_dates_file(
    input: &Path,
    name: Option<&str>,
    server: Option<&str>,
    start_date: Option<Date>,
    end_date: Option<Date>,
) -> Result<DataFrame, Error> {
    let args = ScanArgsParquet::default();
    let mut df = LazyFrame::scan_parquet(input, args)?;
    if let Some(name) = name {
        df = df.filter(col("location_name").eq(lit(name)));
    }
    if let Some(server) = server {
        df = df.filter(col("server").eq(lit(server)));
    }
    if let Some(start_date) = start_date {
        let timestamp = PrimitiveDateTime::new(start_date, Time::from_hms(0, 0, 0)?)
            .assume_utc()
            .unix_timestamp()
            * 1000;
        df = df.filter(
            col("created_at")
                .dt()
                .timestamp(TimeUnit::Milliseconds)
                .gt_eq(timestamp),
        );
    }
    if let Some(end_date) = end_date {
        let timestamp = PrimitiveDateTime::new(end_date, Time::from_hms(0, 0, 0)?)
            .assume_utc()
            .unix_timestamp()
            * 1000;
        df = df.filter(
            col("created_at")
                .dt()
                .timestamp(TimeUnit::Milliseconds)
                .lt_eq(timestamp),
        );
    }
    let df = df
        .sort(
            "created_at",
            SortOptions {
                descending: false,
                ..SortOptions::default()
            },
        )
        .collect()?;
    Ok(df)
}
