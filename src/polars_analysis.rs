use anyhow::{format_err, Error};
use chrono::NaiveDateTime;
use futures::TryStreamExt;
use log::debug;
use polars::{
    datatypes::{DatetimeChunked, TimeUnit},
    io::SerReader,
    lazy::frame::IntoLazy,
    prelude::{
        BooleanChunked, DataFrame, Float64Chunked, Int32Chunked, IntoSeries, NewChunkedArray,
        ParquetReader, ParquetWriter, SortOptions, UniqueKeepStrategy, Utf8Chunked,
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
    pub fn into_weather_data(self) -> Vec<WeatherDataDB> {
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
                WeatherDataColumns {
                    id: Vec::with_capacity(count as usize),
                    dt: Vec::with_capacity(count as usize),
                    created_at: Vec::with_capacity(count as usize),
                    location_name: Vec::with_capacity(count as usize),
                    latitude: Vec::with_capacity(count as usize),
                    longitude: Vec::with_capacity(count as usize),
                    condition: Vec::with_capacity(count as usize),
                    temperature: Vec::with_capacity(count as usize),
                    temperature_minimum: Vec::with_capacity(count as usize),
                    temperature_maximum: Vec::with_capacity(count as usize),
                    pressure: Vec::with_capacity(count as usize),
                    humidity: Vec::with_capacity(count as usize),
                    visibility: Vec::with_capacity(count as usize),
                    rain: Vec::with_capacity(count as usize),
                    snow: Vec::with_capacity(count as usize),
                    wind_speed: Vec::with_capacity(count as usize),
                    wind_direction: Vec::with_capacity(count as usize),
                    country: Vec::with_capacity(count as usize),
                    sunrise: Vec::with_capacity(count as usize),
                    sunset: Vec::with_capacity(count as usize),
                    timezone: Vec::with_capacity(count as usize),
                    server: Vec::with_capacity(count as usize),
                },
                |mut acc, row| async move {
                    acc.id.push(format_sstr!("{}", row.id));
                    acc.dt.push(row.dt);
                    acc.created_at
                        .push(convert_offset_naive(row.created_at.into()));
                    acc.location_name.push(row.location_name);
                    acc.latitude.push(row.latitude);
                    acc.longitude.push(row.longitude);
                    acc.condition.push(row.condition);
                    acc.temperature.push(row.temperature);
                    acc.temperature_minimum.push(row.temperature_minimum);
                    acc.temperature_maximum.push(row.temperature_maximum);
                    acc.pressure.push(row.pressure);
                    acc.humidity.push(row.humidity);
                    acc.visibility.push(row.visibility);
                    acc.rain.push(row.rain);
                    acc.snow.push(row.snow);
                    acc.wind_speed.push(row.wind_speed);
                    acc.wind_direction.push(row.wind_direction);
                    acc.country.push(row.country);
                    acc.sunrise.push(convert_offset_naive(row.sunrise.into()));
                    acc.sunset.push(convert_offset_naive(row.sunset.into()));
                    acc.timezone.push(row.timezone);
                    acc.server.push(row.server);
                    Ok(acc)
                },
            )
            .await?;

        let columns = vec![
            Utf8Chunked::from_slice("id", &weather_rows.id).into_series(),
            Int32Chunked::from_slice("dt", &weather_rows.dt).into_series(),
            DatetimeChunked::from_naive_datetime(
                "created_at",
                weather_rows.created_at,
                TimeUnit::Milliseconds,
            )
            .into_series(),
            Utf8Chunked::from_slice("location_name", &weather_rows.location_name).into_series(),
            Float64Chunked::from_slice("latitude", &weather_rows.latitude).into_series(),
            Float64Chunked::from_slice("longitude", &weather_rows.longitude).into_series(),
            Utf8Chunked::from_slice("condition", &weather_rows.condition).into_series(),
            Float64Chunked::from_slice("temperature", &weather_rows.temperature).into_series(),
            Float64Chunked::from_slice("temperature_minimum", &weather_rows.temperature_minimum)
                .into_series(),
            Float64Chunked::from_slice("temperature_maximum", &weather_rows.temperature_maximum)
                .into_series(),
            Float64Chunked::from_slice("pressure", &weather_rows.pressure).into_series(),
            Int32Chunked::from_slice("humidity", &weather_rows.humidity).into_series(),
            Float64Chunked::from_slice_options("visibility", &weather_rows.visibility)
                .into_series(),
            Float64Chunked::from_slice_options("rain", &weather_rows.rain).into_series(),
            Float64Chunked::from_slice_options("snow", &weather_rows.snow).into_series(),
            Float64Chunked::from_slice("wind_speed", &weather_rows.wind_speed).into_series(),
            Float64Chunked::from_slice_options("wind_direction", &weather_rows.wind_direction)
                .into_series(),
            Utf8Chunked::from_slice("country", &weather_rows.country).into_series(),
            DatetimeChunked::from_naive_datetime(
                "sunrise",
                weather_rows.sunrise,
                TimeUnit::Milliseconds,
            )
            .into_series(),
            DatetimeChunked::from_naive_datetime(
                "sunset",
                weather_rows.sunset,
                TimeUnit::Milliseconds,
            )
            .into_series(),
            Int32Chunked::from_slice("timezone", &weather_rows.timezone).into_series(),
            Utf8Chunked::from_slice("server", &weather_rows.server).into_series(),
        ];

        let new_df = DataFrame::new(columns)?;
        output.push(format_sstr!("{:?}", new_df.shape()));

        let filename = format_sstr!("weather_data_{year:04}_{month:02}.parquet");
        let file = outdir.join(&filename);
        let mut df = if file.exists() {
            let df = ParquetReader::new(File::open(&file)?).finish()?;
            output.push(format_sstr!("{:?}", df.shape()));
            df.vstack(&new_df)?
                .unique(None, UniqueKeepStrategy::First, None)?
        } else {
            new_df
        };
        ParquetWriter::new(File::create(&file)?).finish(&mut df)?;
        output.push(format_sstr!("wrote {filename} {:?}", df.shape()));
    }

    Ok(output)
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
                .utf8()?
                .into_iter()
                .filter_map(|i| i.map(Into::into))
                .collect(),
            dt: df
                .column("dt")?
                .i32()?
                .into_iter()
                .flatten()
                .collect(),
            created_at: df
                .column("created_at")?
                .datetime()?
                .into_iter()
                .filter_map(|t| t.and_then(NaiveDateTime::from_timestamp_millis))
                .collect(),
            location_name: df
                .column("location_name")?
                .utf8()?
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
                .utf8()?
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
                .utf8()?
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
                .utf8()?
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
    let mut df = ParquetReader::new(File::open(input)?).finish()?;
    if let Some(name) = name {
        let mask: Vec<_> = df
            .column("location_name")?
            .utf8()?
            .into_iter()
            .map(|x| x == Some(name))
            .collect();
        let mask = BooleanChunked::from_slice("name_mask", &mask);
        df = df.filter(&mask)?;
    }
    if let Some(server) = server {
        let mask: Vec<_> = df
            .column("server")?
            .utf8()?
            .into_iter()
            .map(|x| x == Some(server))
            .collect();
        let mask = BooleanChunked::from_slice("server_mask", &mask);
        df = df.filter(&mask)?;
    }
    if let Some(start_date) = start_date {
        let timestamp = PrimitiveDateTime::new(start_date, Time::from_hms(0, 0, 0)?)
            .assume_utc()
            .unix_timestamp()
            * 1000;
        let mask: Vec<_> = df
            .column("created_at")?
            .datetime()?
            .into_iter()
            .map(|t| {
                if let Some(t) = t {
                    t >= timestamp
                } else {
                    true
                }
            })
            .collect();
        let mask = BooleanChunked::from_slice("created_at", &mask);
        df = df.filter(&mask)?;
    }
    if let Some(end_date) = end_date {
        let timestamp = PrimitiveDateTime::new(end_date, Time::from_hms(0, 0, 0)?)
            .assume_utc()
            .unix_timestamp()
            * 1000;
        let mask: Vec<_> = df
            .column("created_at")?
            .datetime()?
            .into_iter()
            .map(|t| {
                if let Some(t) = t {
                    t <= timestamp
                } else {
                    true
                }
            })
            .collect();
        let mask = BooleanChunked::from_slice("created_at", &mask);
        df = df.filter(&mask)?;
    }
    let df = df
        .lazy()
        .sort(
            "created_at",
            SortOptions {
                descending: true,
                ..SortOptions::default()
            },
        )
        .collect()?;
    Ok(df)
}
