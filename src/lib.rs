#![warn(missing_docs)]

//! Cloudflare worker.

use riven::consts::RegionalRoute;
use worker::{event, Context, Date, Env, MessageBatch, Request, Response, Result};

pub mod util;

// TODO(mingwei)
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Job {
    UpdateSummoner(usize),
    UpdateSubreddit(usize),
}

/// Cloudflare queue handler.
#[event(queue)]
pub async fn queue(message_batch: MessageBatch<String>, env: Env, _ctx: Context) -> Result<()> {
    util::init_logging();

    for message in message_batch.messages()? {
        let message = message.into_body();
        log::info!("Received webjob message: {}", message,);
    }

    Ok(())
}

/// Cloudflare fetch request handler.
#[event(fetch, respond_with_errors)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    util::init_logging();

    let cf = req.cf().unwrap();
    log::info!(
        "{} - [{}], located at: {:?}, within: {}",
        Date::now().to_string(),
        req.path(),
        cf.coordinates().unwrap_or_default(),
        cf.region().unwrap_or("unknown region".into())
    );

    env.queue("BINDING_QUEUE_WEBJOB")
        .expect("Failed to get `BINDING_QUEUE_WEBJOB` for queue producer.")
        .send("hello world")
        .await?;

    let riot_api = util::get_rgapi(&env);

    let summoner = riot_api
        .account_v1()
        .get_by_riot_id(RegionalRoute::AMERICAS, "LugnutsK", "000")
        .await
        .unwrap()
        .unwrap();
    Response::ok(format!(
        "Hello {}#{}!",
        &summoner.game_name.as_deref().unwrap_or("???"),
        &summoner.tag_line.as_deref().unwrap_or("???"),
    ))
}
