#![warn(missing_docs)]

//! Cloudflare worker.

use riven::consts::RegionalRoute;
use worker::{event, Context, Date, Env, Request, Response, Result};

pub mod util;

/// Cloudflare fetch request handler.
#[event(fetch)]
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
