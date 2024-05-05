#![warn(missing_docs)]

//! Cloudflare worker.

use futures::future::join_all;
use riven::consts::{Champion, PlatformRoute, RegionalRoute};
use serde_with::de::DeserializeAsWrap;
use serde_with::json::JsonString;
use serde_with::{BoolFromInt, DisplayFromStr, Same, TimestampMilliSeconds};
use util::get_rgapi;
use web_time::SystemTime;
use with::IgnoreKeys;
use worker::{
    event, query, Context, Env, Error, MessageBatch, MessageExt, Request, Response, Result,
};

use crate::with::WebSystemTime;

pub mod util;
pub mod webjob;
pub mod with;

/// Local region.
pub const ROUTE: RegionalRoute = RegionalRoute::AMERICAS;

/// Cloudflare queue handler.
#[event(queue)]
pub async fn queue(
    message_batch: MessageBatch<webjob::Task>,
    env: Env,
    _ctx: Context,
) -> Result<()> {
    util::init_logging();

    let rgapi = get_rgapi(&env);

    let futures = message_batch.messages()?.into_iter().map(|msg| {
        log::info!("Handling webjob task: {:?}", msg.body());
        let db = env.d1("BINDING_D1_DB").unwrap();
        webjob::handle(db, rgapi, msg)
    });
    let results = join_all(futures).await;
    let errors = results
        .into_iter()
        .filter_map(|result| result.map(|msg| msg.ack()).err())
        .collect::<Vec<_>>();

    errors
        .is_empty()
        .then_some(())
        .ok_or(Error::RustError(format!("{:?}", errors)))
}

/// Cloudflare fetch request handler.
#[event(fetch, respond_with_errors)]
pub async fn fetch(_req: Request, env: Env, _ctx: Context) -> Result<Response> {
    util::init_logging();

    let queue = env.queue("BINDING_QUEUE_WEBJOB").unwrap();
    queue.send(webjob::Task::UpdateSummoner(1)).await?;

    let db = env.d1("BINDING_D1_DB").unwrap();

    type UserWrap = DeserializeAsWrap<
        (u64, String, bool, Option<u64>),
        IgnoreKeys<(Same, Same, BoolFromInt, Same)>,
    >;
    let query = query!(
        &db,
        "SELECT id, reddit_user_name, profile_is_public, profile_bgskinid FROM user"
    );
    let response1 = query
        .all()
        .await?
        .results()?
        .into_iter()
        .map(UserWrap::into_inner)
        .collect::<Vec<_>>();

    let mut summoners = Vec::new();
    for &(id, ..) in response1.iter() {
        type SummonerWrap = DeserializeAsWrap<
            (
                u64,
                u64,
                String,
                PlatformRoute,
                String,
                String,
                Option<SystemTime>,
                Option<Vec<ChampScore>>,
            ),
            IgnoreKeys<(
                Same,
                Same,
                Same,
                DisplayFromStr,
                Same,
                Same,
                Option<WebSystemTime<TimestampMilliSeconds<i64>>>,
                Option<JsonString>,
            )>,
        >;
        let query = query!(&db, "SELECT id, user_id, puuid, platform, game_name, tag_line, last_update, champ_scores FROM summoner WHERE user_id = ?1", id)?;
        summoners.push(
            query
                .all()
                .await?
                .results()?
                .into_iter()
                .map(SummonerWrap::into_inner)
                .collect::<Vec<_>>(),
        );
    }

    Response::ok(format!("{:#?}\n\n{:#?}", response1, summoners))
}

/// Per-champion mastery info.
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChampScore {
    /// Which champion.
    pub champion: Champion,
    /// How many mastery points earned.
    pub points: i32,
    /// What level (up to 7).
    pub level: i32,
}
