use anyhow::Error;
use futures::Stream;
use postgres_query::{client::GenericClient, query, query_dyn, Error as PqError, FromSqlRow};
use stack_string::{format_sstr, StackString};
use std::convert::TryInto;
use time::OffsetDateTime;
use uuid::Uuid;

use weather_util_rust::{
    distance::Distance,
    precipitation::Precipitation,
    direction::Direction,
    weather_data::{Coord, Rain, Snow, Sys, WeatherCond, WeatherData, WeatherMain, Wind},
};

use crate::pgpool::PgPool;

#[derive(FromSqlRow)]
pub struct WeatherDataDB {
    pub id: Uuid,
    dt: i32,
    created_at: OffsetDateTime,
    location_name: StackString,
    latitude: f64,
    longitude: f64,
    condition: StackString,
    temperature: f64,
    temperature_minimum: f64,
    temperature_maximum: f64,
    pressure: f64,
    humidity: i32,
    visibility: Option<f64>,
    rain: Option<f64>,
    snow: Option<f64>,
    wind_speed: f64,
    wind_direction: Option<f64>,
    country: StackString,
    sunrise: OffsetDateTime,
    sunset: OffsetDateTime,
    timezone: i32,
}

impl From<WeatherData> for WeatherDataDB {
    fn from(value: WeatherData) -> Self {
        let conditions: Vec<_> = value
            .weather
            .iter()
            .map(|w| format_sstr!("{} {} ", w.main, w.description))
            .collect();
        let tz: i32 = value.timezone.into();
        let humidity: i64 = value.main.humidity.into();
        Self {
            id: Uuid::new_v4(),
            dt: value.dt.unix_timestamp() as i32,
            created_at: value.dt,
            location_name: value.name.into(),
            latitude: value.coord.lat.into(),
            longitude: value.coord.lon.into(),
            condition: conditions.join(", ").into(),
            temperature: value.main.temp.kelvin(),
            temperature_minimum: value.main.temp_min.kelvin(),
            temperature_maximum: value.main.temp_max.kelvin(),
            pressure: value.main.pressure.kpa(),
            humidity: humidity as i32,
            visibility: value.visibility.map(Distance::meters),
            rain: value
                .rain
                .and_then(|r| r.three_hour.map(Precipitation::millimeters)),
            snow: value
                .snow
                .and_then(|s| s.three_hour.map(Precipitation::millimeters)),
            wind_speed: value.wind.speed.mps(),
            wind_direction: value.wind.deg.map(|d| d.deg()),
            country: value.sys.country.map_or("".into(), Into::into),
            sunrise: value.sys.sunrise,
            sunset: value.sys.sunset,
            timezone: tz,
        }
    }
}

impl From<WeatherDataDB> for WeatherData {
    fn from(value: WeatherDataDB) -> Self {
        Self {
            coord: Coord {
                lon: value.longitude.try_into().unwrap(),
                lat: value.latitude.try_into().unwrap(),
            },
            weather: vec![WeatherCond {
                id: 0,
                main: value.condition.into(),
                description: String::new(),
                icon: String::new(),
            }],
            base: String::new(),
            main: WeatherMain {
                temp: value.temperature.try_into().unwrap(),
                feels_like: value.temperature.try_into().unwrap(),
                temp_min: value.temperature_minimum.try_into().unwrap(),
                temp_max: value.temperature_maximum.try_into().unwrap(),
                pressure: value.pressure.try_into().unwrap(),
                humidity: i64::from(value.humidity).try_into().unwrap(),
            },
            visibility: value.visibility.and_then(|v| v.try_into().ok()),
            rain: value.rain.map(|r| Rain {
                three_hour: r.try_into().ok(),
            }),
            snow: value.snow.map(|s| Snow {
                three_hour: s.try_into().ok(),
            }),
            wind: Wind {
                speed: value.wind_speed.try_into().unwrap(),
                deg: value.wind_direction.map(Direction::from_deg),
            },
            dt: value.created_at,
            sys: Sys {
                country: Some(value.country.into()),
                sunrise: value.sunrise,
                sunset: value.sunset,
            },
            timezone: value.timezone.try_into().unwrap(),
            name: value.location_name.into(),
        }
    }
}

impl WeatherDataDB {
    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        Self::get_by_id_conn(&conn, id).await
    }

    async fn get_by_id_conn<C>(conn: &C, id: Uuid) -> Result<Option<Self>, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!("SELECT * FROM weather_data WHERE id=$id", id = id,);
        query.fetch_opt(conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_dt_name(pool: &PgPool, dt: i32, name: &str) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        Self::get_by_dt_name_conn(&conn, dt, name).await
    }

    async fn get_by_dt_name_conn<C>(conn: &C, dt: i32, name: &str) -> Result<Option<Self>, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            "SELECT * FROM weather_data WHERE dt=$dt AND location_name = $name",
            dt = dt,
            name = name,
        );
        query.fetch_opt(conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get(
        pool: &PgPool,
        offset: Option<usize>,
        limit: Option<usize>,
        order: bool,
    ) -> Result<impl Stream<Item = Result<Self, PqError>>, Error> {
        let conn = pool.get().await?;
        let mut query = format_sstr!("SELECT * FROM weather_data");
        if order {
            query.push_str(" ORDER BY created_at DESC");
        } else {
            query.push_str(" ORDER BY created_at");
        };
        if let Some(offset) = offset {
            query.push_str(&format_sstr!(" OFFSET {offset}"));
        }
        if let Some(limit) = limit {
            query.push_str(&format_sstr!(" LIMIT {limit}"));
        }
        let query = query_dyn!(&query)?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn delete(&self, pool: &PgPool) -> Result<u64, Error> {
        let conn = pool.get().await?;
        let query = query!("DELETE FROM weather_data WHERE id = $id", id = self.id);
        query.execute(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn insert(&self, pool: &PgPool) -> Result<u64, Error> {
        let conn = pool.get().await?;
        self.insert_conn(&conn).await
    }

    async fn insert_conn<C>(&self, conn: &C) -> Result<u64, Error>
    where
        C: GenericClient + Sync,
    {
        let query = query!(
            r#"
                INSERT INTO weather_data (
                    dt,
                    created_at,
                    location_name,
                    latitude,
                    longitude,
                    condition,
                    temperature,
                    temperature_minimum,
                    temperature_maximum,
                    pressure,
                    humidity,
                    visibility,
                    rain,
                    snow,
                    wind_speed,
                    wind_direction,
                    country,
                    sunrise,
                    sunset,
                    timezone
                ) VALUES (
                    $dt,
                    $created_at,
                    $location_name,
                    $latitude,
                    $longitude,
                    $condition,
                    $temperature,
                    $temperature_minimum,
                    $temperature_maximum,
                    $pressure,
                    $humidity,
                    $visibility,
                    $rain,
                    $snow,
                    $wind_speed,
                    $wind_direction,
                    $country,
                    $sunrise,
                    $sunset,
                    $timezone
                ) ON CONFLICT DO NOTHING
            "#,
            dt = self.dt,
            created_at = self.created_at,
            location_name = self.location_name,
            latitude = self.latitude,
            longitude = self.longitude,
            condition = self.condition,
            temperature = self.temperature,
            temperature_minimum = self.temperature_minimum,
            temperature_maximum = self.temperature_maximum,
            pressure = self.pressure,
            humidity = self.humidity,
            visibility = self.visibility,
            rain = self.rain,
            snow = self.snow,
            wind_speed = self.wind_speed,
            wind_direction = self.wind_direction,
            country = self.country,
            sunrise = self.sunrise,
            sunset = self.sunset,
            timezone = self.timezone,
        );
        query.execute(conn).await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use weather_util_rust::weather_api::{WeatherApi, WeatherLocation};

    use crate::{config::Config, model::WeatherDataDB, pgpool::PgPool};

    #[tokio::test]
    #[ignore]
    async fn test_weather_data_db() -> Result<(), Error> {
        let config = Config::init_config(None)?;
        let api = WeatherApi::new(&config.api_key, &config.api_endpoint, &config.api_path);
        let loc = WeatherLocation::ZipCode {
            zipcode: 99782,
            country_code: None,
        };
        let weather = api.get_weather_data(&loc).await?;
        let weather_db: WeatherDataDB = weather.into();
        if let Some(db_url) = config.database_url.as_ref() {
            let pool = PgPool::new(db_url);
            let written = weather_db.insert(&pool).await?;
            println!("written {written}");

            let weather_fromcache =
                WeatherDataDB::get_by_dt_name(&pool, weather_db.dt, &weather_db.location_name)
                    .await?;
            assert!(weather_fromcache.is_some());
            weather_fromcache.unwrap().delete(&pool).await?;
        }
        Ok(())
    }
}