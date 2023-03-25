CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE weather_data (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    dt INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    location_name TEXT NOT NULL,
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    condition TEXT NOT NULL,
    temperature DOUBLE PRECISION NOT NULL,
    temperature_minimum DOUBLE PRECISION NOT NULL,
    temperature_maximum DOUBLE PRECISION NOT NULL,
    pressure DOUBLE PRECISION NOT NULL,
    humidity INTEGER NOT NULL,
    visibility DOUBLE PRECISION,
    rain DOUBLE PRECISION,
    snow DOUBLE PRECISION,
    wind_speed DOUBLE PRECISION NOT NULL,
    wind_direction DOUBLE PRECISION,
    country TEXT NOT NULL,
    sunrise TIMESTAMP WITH TIME ZONE NOT NULL,
    sunset TIMESTAMP WITH TIME ZONE NOT NULL,
    timezone INTEGER NOT NULL,

    UNIQUE (dt, location_name)
);
