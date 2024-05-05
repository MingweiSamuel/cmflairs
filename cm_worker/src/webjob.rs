//! Background "webjob" task handling.

use riven::models::champion_mastery_v4::ChampionMastery;
use riven::RiotApi;
use web_time::SystemTime;
use worker::{query, D1Database, Error, Message, Result};

use crate::db;

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

/// Handle [`Task::UpdateSummoner`].
pub async fn update_summoner(db: D1Database, rgapi: &RiotApi, summoner_id: u64) -> Result<()> {
    let query = query!(&db, "SELECT * FROM summoner WHERE id = ?1", summoner_id)?;
    let db_summoner: db::Summoner = query.first(None).await?.ok_or_else(|| {
        Error::RustError(format!(
            "Failed to find summoner with PK ID: {}",
            summoner_id
        ))
    })?;

    // TODO(mingwei): handle chaning riot IDs.
    let champion_masteries = rgapi
        .champion_mastery_v4()
        .get_all_champion_masteries_by_puuid(db_summoner.platform, &db_summoner.puuid)
        .await
        .map_err(|e| {
            Error::RustError(format!(
                "Failed to get summoner with PUUID {}: {}",
                db_summoner.puuid, e
            ))
        })?;
    let champion_masteries = champion_masteries
        .into_iter()
        .map(
            |ChampionMastery {
                 champion_id,
                 champion_points,
                 champion_level,
                 ..
             }| db::ChampionMastery {
                champion: champion_id,
                points: champion_points,
                level: champion_level,
            },
        )
        .collect::<Vec<_>>();
    if db_summoner.champion_masteries.as_ref() != Some(&champion_masteries) {
        let query = query!(
            &db,
            "UPDATE ?4 SET
                champion_masteries = ?2,
                last_update = ?3
            WHERE id = ?1",
            db_summoner.id,
            serde_json::to_string(&champion_masteries).unwrap(),
            SystemTime::UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
            "summoner",
        )?;
        query.run().await?;
    }

    Ok(())
}
