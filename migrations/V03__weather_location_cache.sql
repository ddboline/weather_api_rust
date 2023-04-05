CREATE TABLE weather_location_cache (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    location_name TEXT NOT NULL,
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    zipcode INTEGER,
    country_code TEXT,
    city_name TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,

    UNIQUE (location_name)
);
