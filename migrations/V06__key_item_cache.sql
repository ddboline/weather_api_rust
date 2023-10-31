CREATE TABLE key_item_cache (
    s3_key TEXT NOT NULL UNIQUE PRIMARY KEY,
    etag TEXT NOT NULL,
    s3_timestamp BIGINT NOT NULL,
    s3_size BIGINT NOT NULL,
    has_local BOOLEAN NOT NULL DEFAULT false,
    has_remote BOOLEAN NOT NULL DEFAULT false
)