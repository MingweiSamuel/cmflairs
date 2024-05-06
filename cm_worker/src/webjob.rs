//! Background "webjob" task handling.

use futures::future::join_all;
use riven::consts::PlatformRoute;
use riven::models::champion_mastery_v4::ChampionMastery;
use riven::RiotApi;
use serde_with::de::DeserializeAsWrap;
use serde_with::json::JsonString;
use serde_with::ser::SerializeAsWrap;
use serde_with::{DisplayFromStr, Same, TimestampMilliSeconds};
use web_time::SystemTime;
use worker::{query, D1Database, Env, Error, Message, Result};

use crate::util::{get_bulk_update_batch_size, get_rgapi};
use crate::with::{IgnoreKeys, WebSystemTime};
use crate::ChampScore;

/// Enum of the possible tasks for the RiotApi web job.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Task {
    /// Update the summoner with the given PK ID.
    SummonerUpdate(u64),
    /// Update a batch of summoners. Amount determined by `WEBJOB_BULK_UPDATE_BATCH_SIZE`.
    SummonerBulkUpdate,
}

/// Handle a `Task`.
pub async fn handle(env: &Env, msg: Message<Task>) -> Result<Message<Task>> {
    let rgapi = get_rgapi(&env);
    let db = env.d1("BINDING_D1_DB").unwrap();

    match msg.body() {
        &Task::SummonerUpdate(summoner_id) => {
            summoner_update(db, rgapi, summoner_id).await?;
            Result::<Message<_>>::Ok(msg)
        }
        Task::SummonerBulkUpdate => {
            let batch_size = get_bulk_update_batch_size(env);
            summoner_bulk_update(db, rgapi, batch_size).await?;
            Result::<Message<_>>::Ok(msg)
        }
    }
}

type Wrap<T, U> = DeserializeAsWrap<T, IgnoreKeys<U>>;

/// Handle [`Task::SummonerBulkUpdate`].
pub async fn summoner_bulk_update(db: D1Database, rgapi: &RiotApi, batch_size: u32) -> Result<()> {
    type SummonerValus = (u64, String, PlatformRoute);
    type SummonerSerde = (Same, Same, DisplayFromStr);
    let query = query!(
        &db,
        "SELECT id, puuid, platform FROM summoner ORDER BY last_update ASC LIMIT ?",
        batch_size,
    )?;
    let summoner_to_update = query
        .all()
        .await?
        .results()?
        .into_iter()
        .map(<Wrap<SummonerValus, SummonerSerde>>::into_inner)
        .collect::<Vec<_>>();

    let champ_scores_list =
        summoner_to_update
            .into_iter()
            .map(|(id, puuid, platform)| async move {
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
                Result::Ok((id, champ_scores))
            });

    let champ_scores_list = join_all(champ_scores_list).await;

    let now = SystemTime::now();
    let now = <SerializeAsWrap<_, WebSystemTime<TimestampMilliSeconds<i64>>>>::new(&now);

    let mut errors = Vec::new();
    let updates = champ_scores_list
        .into_iter()
        .map(|result| {
            let (id, champ_scores) = result?;
            let update = query!(
                &db,
                "UPDATE summoner SET
                    champ_scores = ?,
                    last_update = ?
                WHERE id = ?",
                <SerializeAsWrap<_, JsonString>>::new(&champ_scores),
                now,
                id,
            )?;
            Ok(update)
        })
        .filter_map(|result| result.map_err(|err| errors.push(err)).ok())
        .collect();

    if let Err(err) = db.batch(updates).await {
        errors.push(err)
    }

    errors
        .is_empty()
        .then_some(())
        .ok_or(Error::RustError(format!("{:?}", errors)))
}

/// Handle [`Task::UpdateSummoner`].
pub async fn summoner_update(db: D1Database, rgapi: &RiotApi, summoner_id: u64) -> Result<()> {
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

    let update = query!(
        &db,
        "UPDATE summoner SET
            champ_scores = ?,
            last_update = ?
        WHERE id = ?",
        <SerializeAsWrap<_, JsonString>>::new(&champ_scores),
        <SerializeAsWrap<_, WebSystemTime<TimestampMilliSeconds<i64>>>>::new(&SystemTime::now()),
        summoner_id,
    )?;
    update.run().await?;

    Ok(())
}
