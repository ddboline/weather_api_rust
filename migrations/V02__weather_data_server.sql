ALTER TABLE weather_data ADD COLUMN server TEXT;

UPDATE weather_data SET server = 'N/A';

ALTER TABLE weather_data ALTER COLUMN server SET NOT NULL;
