CREATE TABLE user_locations (
    domain       TEXT NOT NULL,
    user         TEXT NOT NULL,
    place_name   TEXT,
    country      TEXT,
    feature_code TEXT,
    longitude    REAL NOT NULL,
    latitude     REAL NOT NULL,
    PRIMARY KEY (
        domain COLLATE NOCASE,
        user COLLATE NOCASE
    )
    ON CONFLICT REPLACE,
    CHECK ( (place_name IS NULL) = (feature_code IS NULL) ) 
);
