-- Migration number: 0001 	 2024-05-04T19:17:45.048Z
DROP TABLE IF EXISTS user;

CREATE TABLE IF NOT EXISTS user (
    id INTEGER PRIMARY KEY,
    reddit_user_name TEXT NOT NULL,
    profile_is_public INTEGER NOT NULL,
    profile_bgskinid INTEGER
);

CREATE INDEX IF NOT EXISTS idx_user__reddit_user_name ON user(reddit_user_name);

INSERT INTO
    user (
        id,
        reddit_user_name,
        profile_is_public,
        profile_bgskinid
    )
VALUES
    (1, 'LugnutsK', 1, 99008);