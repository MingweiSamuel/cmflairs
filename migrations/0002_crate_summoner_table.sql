-- Migration number: 0002 	 2024-05-04T20:30:45.479Z
DROP TABLE IF EXISTS summoner;

CREATE TABLE IF NOT EXISTS summoner (
    id INTEGER PRIMARY KEY,
    puuid TEXT NOT NULL UNIQUE,
    game_name TEXT NOT NULL,
    tag_line TEXT NOT NULL,
    platform TEXT NOT NULL
);

INSERT INTO
    summoner (
        id,
        puuid,
        game_name,
        tag_line,
        platform
    )
VALUES
    (
        1,
        'rw6rya0JBisqklX3No-CcVYRKEJfSPUXWzOBgLih_4aAUdhF5sgqzf8Czg-8HROdP6Kg-OrzgUNMgg',
        'LugnutsK',
        '000',
        'NA1'
    );