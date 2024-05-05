//! Background "webjob" task handling.

use riven::consts::PlatformRoute;
use riven::models::champion_mastery_v4::ChampionMastery;
use riven::RiotApi;
use serde_with::de::DeserializeAsWrap;
use serde_with::json::JsonString;
use serde_with::ser::SerializeAsWrap;
use serde_with::{DisplayFromStr, Same, TimestampMilliSeconds};
use web_time::SystemTime;
use worker::{query, D1Database, Error, Message, Result};

use crate::with::{IgnoreKeys, WebSystemTime};
use crate::ChampScore;

/// Enum of the possible tasks for the RiotApi web job.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Task {
    /// Update the summoner with the given PK ID.
    UpdateSummoner(u64),
}

/// Handle a `Task`.
pub async fn handle(db: D1Database, rgapi: &RiotApi, msg: Message<Task>) -> Result<Message<Task>> {
    match msg.body() {
        &Task::UpdateSummoner(summoner_id) => {
            update_summoner(db, rgapi, summoner_id).await?;
            Result::<Message<_>>::Ok(msg)
        }
    }
}

type Wrap<T, U> = DeserializeAsWrap<T, IgnoreKeys<U>>;

/// Handle [`Task::UpdateSummoner`].
pub async fn update_summoner(db: D1Database, rgapi: &RiotApi, summoner_id: u64) -> Result<()> {
    type SummonerValus = (String, PlatformRoute);
    type SummonerSerde = (Same, DisplayFromStr);
    let query = query!(
        &db,
        "SELECT puuid, platform FROM summoner WHERE id = ?",
        summoner_id,
    )?;
    let (puuid, platform) = query
        .first(None)
        .await?
        .map(<Wrap<SummonerValus, SummonerSerde>>::into_inner)
        .ok_or_else(|| {
            Error::RustError(format!(
                "Failed to find summoner with PK ID: {}",
                summoner_id
            ))
        })?;

    // TODO(mingwei): handle chaning riot IDs `username#tagline`.

    let champion_masteries = rgapi
        .champion_mastery_v4()
        .get_all_champion_masteries_by_puuid(platform, &puuid)
        .await
        .map_err(|e| {
            Error::RustError(format!(
                "Failed to get summoner with PUUID {}: {}",
                puuid, e
            ))
        })?;
    let champ_scores = champion_masteries
        .into_iter()
        .map(
            |ChampionMastery {
                 champion_id,
                 champion_points,
                 champion_level,
                 ..
             }| ChampScore {
                champion: champion_id,
                points: champion_points,
                level: champion_level,
            },
        )
        .collect::<Vec<_>>();

    let query = query!(
        &db,
        "UPDATE summoner SET
            champ_scores = ?,
            last_update = ?
        WHERE id = ?",
        <SerializeAsWrap<_, JsonString>>::new(&champ_scores),
        <SerializeAsWrap<_, WebSystemTime<TimestampMilliSeconds<i64>>>>::new(&SystemTime::now()),
        summoner_id,
    )?;
    query.run().await?;

    Ok(())
}
