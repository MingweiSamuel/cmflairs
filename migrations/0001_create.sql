-- Migration number: 0001 	 2024-05-04T19:17:45.048Z
DROP TABLE IF EXISTS user;

CREATE TABLE IF NOT EXISTS user (
    id INTEGER PRIMARY KEY,
    reddit_id INTEGER NOT NULL UNIQUE,
    reddit_user_name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    profile_is_public INTEGER NOT NULL,
    profile_bgskinid INTEGER
);

CREATE INDEX IF NOT EXISTS idx_user__reddit_user_name ON user(reddit_user_name COLLATE NOCASE);

INSERT INTO
    user (
        reddit_id,
        reddit_user_name,
        profile_is_public,
        profile_bgskinid
    )
VALUES
    (23806698, 'LugnutsK', 1, 99008);

DROP TABLE IF EXISTS summoner;

CREATE TABLE IF NOT EXISTS summoner (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    puuid TEXT NOT NULL UNIQUE,
    game_name TEXT NOT NULL,
    tag_line TEXT NOT NULL,
    platform TEXT NOT NULL,
    last_update INTEGER,
    champ_scores TEXT,
    FOREIGN KEY(user_id) REFERENCES user(id)
);

INSERT INTO
    summoner (
        user_id,
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