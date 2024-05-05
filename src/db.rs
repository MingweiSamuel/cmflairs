//! Model structs corresponding to database tables. Must be kept in sync with migrations.

use std::time::SystemTime;

use riven::consts::{Champion, PlatformRoute};
use serde_with::serde_as;

/// A cmflairs user, associated with a specific Reddit account.
#[serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    /// PK ID.
    pub id: u64,
    /// Reddit user name (max length 21).
    pub reddit_user_name: String,
    /// If the profile should be publicly searchable.
    #[serde_as(as = "serde_with::BoolFromInt")]
    pub profile_is_public: bool,
    /// Profile background image skin ID. (`champID * 1000 + skinIdx`).
    pub profile_bgskinid: Option<u64>,

    /// FK [`Summoner::user_id`].
    #[serde(skip)]
    pub summoners: Option<Vec<Summoner>>,
}

/// A Riot Games account, i.e. a LoL summoner.
#[serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Summoner {
    /// PK ID.
    pub id: u64,
    /// FK [`User::id`].
    pub user_id: u64,

    /// Riot PUUID (player universally unique ID).
    pub puuid: String,
    /// Platform this summoner is located in.
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub platform: PlatformRoute,

    /// Riot ID game username (`game_name#tag_line`).
    pub game_name: String,
    /// Riot ID tag line (`game_name#tag_line`).
    pub tag_line: String,

    /// Last time update (unix epoch milliseconds).
    #[serde_as(as = "serde_with::TimestampMilliSeconds<i64>")]
    pub last_update: SystemTime,
    /// TEXT field containing a JSON dictionary.
    #[serde_as(as = "serde_with::DefaultOnNull<serde_with::json::JsonString>")]
    pub champion_masteries: Option<Vec<ChampionMastery>>,
}

/// Per-champion mastery info.
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChampionMastery {
    /// Which champion.
    pub champion: Champion,
    /// How many mastery points earned.
    pub points: i32,
    /// What level (up to 7).
    pub level: i32,
}
