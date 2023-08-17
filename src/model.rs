use anyhow::{format_err, Error};
use futures::{Stream, StreamExt};
use isocountry::CountryCode;
use postgres_query::{
    client::GenericClient, query, query_dyn, Error as PgError, FromSqlRow, Parameter,
};
use serde::{Deserialize, Serialize};
use stack_string::{format_sstr, StackString};
use std::convert::TryInto;
use time::{macros::time, Date, OffsetDateTime, PrimitiveDateTime};
use uuid::Uuid;

use weather_util_rust::{
    direction::Direction,
    distance::Distance,
    precipitation::Precipitation,
    weather_api::{WeatherApi, WeatherLocation},
    weather_data::{Coord, Rain, Snow, Sys, WeatherCond, WeatherData, WeatherMain, Wind},
};

use crate::{date_time_wrapper::DateTimeWrapper, pgpool::PgPool};

#[derive(FromSqlRow, Clone, Debug)]
pub struct AuthorizedUsers {
    pub email: StackString,
}

impl AuthorizedUsers {
    /// # Errors
    /// Return error if db query fails
    pub async fn get_authorized_users(
        pool: &PgPool,
    ) -> Result<impl Stream<Item = Result<Self, PgError>>, Error> {
        let query = query!("SELECT * FROM authorized_users");
        let conn = pool.get().await?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
    }
}

#[derive(FromSqlRow, Serialize, Deserialize, Debug, Clone)]
pub struct WeatherDataDB {
    pub id: Uuid,
    pub dt: i32,
    pub created_at: DateTimeWrapper,
    pub location_name: StackString,
    pub latitude: f64,
    pub longitude: f64,
    pub condition: StackString,
    pub temperature: f64,
    pub temperature_minimum: f64,
    pub temperature_maximum: f64,
    pub pressure: f64,
    pub humidity: i32,
    pub visibility: Option<f64>,
    pub rain: Option<f64>,
    pub snow: Option<f64>,
    pub wind_speed: f64,
    pub wind_direction: Option<f64>,
    pub country: StackString,
    pub sunrise: DateTimeWrapper,
    pub sunset: DateTimeWrapper,
    pub timezone: i32,
    pub server: StackString,
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
            created_at: value.dt.into(),
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
                .and_then(|r| r.one_hour.map(Precipitation::millimeters)),
            snow: value
                .snow
                .and_then(|s| s.one_hour.map(Precipitation::millimeters)),
            wind_speed: value.wind.speed.mps(),
            wind_direction: value.wind.deg.map(|d| d.deg()),
            country: value.sys.country.map_or("".into(), Into::into),
            sunrise: value.sys.sunrise.into(),
            sunset: value.sys.sunset.into(),
            timezone: tz,
            server: "N/A".into(),
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
                three_hour: None,
                one_hour: r.try_into().ok(),
            }),
            snow: value.snow.map(|s| Snow {
                three_hour: None,
                one_hour: s.try_into().ok(),
            }),
            wind: Wind {
                speed: value.wind_speed.try_into().unwrap(),
                deg: value.wind_direction.map(Direction::from_deg),
            },
            dt: value.created_at.into(),
            sys: Sys {
                country: Some(value.country.into()),
                sunrise: value.sunrise.into(),
                sunset: value.sunset.into(),
            },
            timezone: value.timezone.try_into().unwrap(),
            name: value.location_name.into(),
        }
    }
}

impl WeatherDataDB {
    pub fn set_location_name(&mut self, name: &str) {
        self.location_name = name.into();
    }

    pub fn set_server(&mut self, server: &str) {
        self.server = server.into();
    }

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
    pub async fn get_by_name_dates(
        pool: &PgPool,
        name: Option<&str>,
        server: Option<&str>,
        start_date: Option<Date>,
        end_date: Option<Date>,
    ) -> Result<impl Stream<Item = Result<Self, PgError>>, Error> {
        let conn = pool.get().await?;
        let start_date = start_date.map(|d| PrimitiveDateTime::new(d, time!(00:00)).assume_utc());
        let end_date = end_date.map(|d| PrimitiveDateTime::new(d, time!(00:00)).assume_utc());
        let mut bindings = Vec::new();
        let mut constraints = Vec::new();
        if let Some(name) = &name {
            constraints.push(format_sstr!("location_name = $name"));
            bindings.push(("name", name as Parameter));
        }
        if let Some(server) = &server {
            constraints.push(format_sstr!("server = $server"));
            bindings.push(("server", server as Parameter));
        }
        if let Some(start_date) = &start_date {
            constraints.push(format_sstr!("created_at >= $start_date"));
            bindings.push(("start_date", start_date as Parameter));
        }
        if let Some(end_date) = &end_date {
            constraints.push(format_sstr!("created_at <= $end_date"));
            bindings.push(("end_date", end_date as Parameter));
        }
        let where_str = if constraints.is_empty() {
            "".into()
        } else {
            format_sstr!("WHERE {}", constraints.join(" AND "))
        };
        let query = format_sstr!(
            r#"
                SELECT * FROM weather_data
                {where_str}
                ORDER BY created_at
            "#
        );
        let query = query_dyn!(&query, ..bindings)?;
        query.fetch_streaming(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get(
        pool: &PgPool,
        offset: Option<usize>,
        limit: Option<usize>,
        order: bool,
    ) -> Result<impl Stream<Item = Result<Self, PgError>>, Error> {
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
    pub async fn get_locations(
        pool: &PgPool,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<impl Stream<Item = Result<(StackString, i64), Error>>, Error> {
        let conn = pool.get().await?;
        let mut query = format_sstr!(
            r#"
                SELECT location_name, count(*) as count
                FROM weather_data
                GROUP BY 1
                ORDER BY 2 DESC
            "#
        );
        if let Some(offset) = offset {
            query.push_str(&format_sstr!(" OFFSET {offset}"));
        }
        if let Some(limit) = limit {
            query.push_str(&format_sstr!(" LIMIT {limit}"));
        }
        let query = query_dyn!(&query)?;
        query
            .query_streaming(&conn)
            .await
            .map_err(Into::into)
            .map(|s| {
                s.map(|row| {
                    let row = row?;
                    let location: StackString = row.try_get("location_name")?;
                    let count: i64 = row.try_get("count")?;
                    Ok((location, count))
                })
            })
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
                    timezone,
                    server
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
                    $timezone,
                    $server
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
            server = self.server,
        );
        query.execute(conn).await.map_err(Into::into)
    }
}

#[derive(FromSqlRow, Serialize, Deserialize, Debug)]
pub struct WeatherLocationCache {
    pub id: Uuid,
    pub location_name: StackString,
    pub latitude: f64,
    pub longitude: f64,
    pub zipcode: Option<i32>,
    pub country_code: Option<StackString>,
    pub city_name: Option<StackString>,
    pub created_at: OffsetDateTime,
}

impl Default for WeatherLocationCache {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            location_name: StackString::new(),
            latitude: 0.0,
            longitude: 0.0,
            zipcode: None,
            country_code: None,
            city_name: None,
            created_at: OffsetDateTime::now_utc(),
        }
    }
}

impl WeatherLocationCache {
    /// # Errors
    /// Return error if db query fails
    pub fn get_lat_lon_location(&self) -> Result<WeatherLocation, Error> {
        Ok(WeatherLocation::LatLon {
            latitude: self.latitude.try_into()?,
            longitude: self.longitude.try_into()?,
        })
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        let query = query!("SELECT * FROM weather_location_cache WHERE id=$id", id = id,);
        query.fetch_opt(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_location_name(pool: &PgPool, name: &str) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        let query = query!(
            r#"
                SELECT * FROM weather_location_cache
                WHERE location_name=$name
                ORDER BY created_at DESC
                LIMIT 1
            "#,
            name = name,
        );
        query.fetch_opt(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_city_name(pool: &PgPool, name: &str) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        let query = query!(
            r#"
                SELECT * FROM weather_location_cache
                WHERE city_name=$name"
                ORDER BY created_at DESC
                LIMIT 1
            "#,
            name = name,
        );
        query.fetch_opt(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_zip(
        pool: &PgPool,
        zip: u64,
        country_code: Option<CountryCode>,
    ) -> Result<Option<Self>, Error> {
        let zip = zip as i32;
        let country_code = country_code.map(|c| format_sstr!("{c}"));
        let mut constraints = vec!["zipcode=$zip"];
        let mut bindings = vec![("zip", &zip as Parameter)];
        if let Some(country_code) = &country_code {
            constraints.push("country_code=$country_code");
            bindings.push(("country_code", country_code as Parameter));
        }
        let query = format_sstr!(
            r#"
                SELECT * FROM weather_location_cache 
                WHERE {}
                ORDER BY created_at DESC
                LIMIT 1
            "#,
            constraints.join(" AND "),
        );
        let query = query_dyn!(&query, ..bindings)?;
        let conn = pool.get().await?;
        query.fetch_opt(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn get_by_lat_lon(pool: &PgPool, lat: f64, lon: f64) -> Result<Option<Self>, Error> {
        let conn = pool.get().await?;
        let query = query!(
            r#"
                SELECT * FROM weather_location_cache
                WHERE abs(latitude - $lat) < 0.007
                  AND abs(longitude - $lon) < 0.008
                ORDER BY (latitude - $lat) * (latitude - $lat) + (longitude - $lon) * (longitude - $lon)
                LIMIT 1
            "#,
            lat = lat,
            lon = lon,
        );
        query.fetch_opt(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn insert(&self, pool: &PgPool) -> Result<u64, Error> {
        let query = query!(
            r#"
                INSERT INTO weather_location_cache (
                    location_name, latitude, longitude, zipcode, country_code, city_name, created_at
                ) VALUES (
                    $location_name, $latitude, $longitude, $zipcode, $country_code, $city_name, now()
                )
            "#,
            location_name = self.location_name,
            latitude = self.latitude,
            longitude = self.longitude,
            zipcode = self.zipcode,
            country_code = self.country_code,
            city_name = self.city_name,
        );
        let conn = pool.get().await?;
        query.execute(&conn).await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if api call fails
    pub async fn from_weather_location(
        api: &WeatherApi,
        location: &WeatherLocation,
    ) -> Result<Self, Error> {
        match location {
            WeatherLocation::LatLon {
                latitude,
                longitude,
            } => {
                let mut locations = api.get_geo_location(*latitude, *longitude).await?;
                if locations.is_empty() {
                    return Err(format_err!("no location"));
                }
                let loc = locations.swap_remove(0);
                Ok(Self {
                    id: Uuid::new_v4(),
                    location_name: loc.name.into(),
                    latitude: (*latitude).into(),
                    longitude: (*longitude).into(),
                    country_code: Some(loc.country.into()),
                    ..Self::default()
                })
            }
            WeatherLocation::ZipCode {
                zipcode,
                country_code,
            } => {
                let loc = api.get_zip_location(*zipcode, *country_code).await?;
                Ok(Self {
                    id: Uuid::new_v4(),
                    location_name: loc.name.into(),
                    latitude: loc.lat,
                    longitude: loc.lon,
                    zipcode: Some(*zipcode as i32),
                    country_code: Some(loc.country.into()),
                    ..Self::default()
                })
            }
            WeatherLocation::CityName(city_name) => {
                if let WeatherLocation::LatLon {
                    latitude,
                    longitude,
                } = location.to_lat_lon(api).await?
                {
                    let mut locations = api.get_geo_location(latitude, longitude).await?;
                    if locations.is_empty() {
                        return Err(format_err!("no location"));
                    }
                    let loc = locations.swap_remove(0);
                    Ok(Self {
                        id: Uuid::new_v4(),
                        location_name: loc.name.into(),
                        latitude: latitude.into(),
                        longitude: longitude.into(),
                        country_code: Some(loc.country.into()),
                        city_name: Some(city_name.into()),
                        ..Self::default()
                    })
                } else {
                    Err(format_err!("failed to get lat lon"))
                }
            }
        }
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn from_weather_location_cache(
        pool: &PgPool,
        location: &WeatherLocation,
    ) -> Result<Option<Self>, Error> {
        match location {
            WeatherLocation::LatLon {
                latitude,
                longitude,
            } => Self::get_by_lat_lon(pool, (*latitude).into(), (*longitude).into()).await,
            WeatherLocation::ZipCode {
                zipcode,
                country_code,
            } => Self::get_by_zip(pool, *zipcode, *country_code).await,
            WeatherLocation::CityName(city_name) => {
                if let Ok(Some(l)) = Self::get_by_city_name(pool, city_name).await {
                    Ok(Some(l))
                } else {
                    Self::get_by_location_name(pool, city_name).await
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use log::info;

    use weather_util_rust::weather_api::{WeatherApi, WeatherLocation};

    use crate::{config::Config, model::WeatherDataDB, pgpool::PgPool};

    #[tokio::test]
    #[ignore]
    async fn test_weather_data_db() -> Result<(), Error> {
        let config = Config::init_config(None)?;
        let api = WeatherApi::new(
            &config.api_key,
            &config.api_endpoint,
            &config.api_path,
            &config.geo_path,
        );
        let loc = WeatherLocation::ZipCode {
            zipcode: 99782,
            country_code: None,
        };
        let weather = api.get_weather_data(&loc).await?;
        let weather_db: WeatherDataDB = weather.into();
        if let Some(db_url) = config.database_url.as_ref() {
            let pool = PgPool::new(db_url);
            let written = weather_db.insert(&pool).await?;
            info!("written {written}");

            let weather_fromcache =
                WeatherDataDB::get_by_dt_name(&pool, weather_db.dt, &weather_db.location_name)
                    .await?;
            assert!(weather_fromcache.is_some());
            weather_fromcache.unwrap().delete(&pool).await?;
        }
        Ok(())
    }
}
