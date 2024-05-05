#![warn(missing_docs)]

//! Cloudflare worker.

use futures::future::join_all;
use riven::consts::RegionalRoute;
use util::get_rgapi;
use worker::{
    event, query, Context, Env, Error, MessageBatch, MessageExt, Request, Response, Result,
};

pub mod db;
pub mod util;
pub mod webjob;

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

    let d1db = env.d1("BINDING_D1_DB").unwrap();

    let query = query!(&d1db, "SELECT * FROM user");
    let mut response1: Vec<db::User> = query.all().await?.results()?;

    for user in response1.iter_mut() {
        let query = query!(&d1db, "SELECT * FROM summoner WHERE user_id = ?1", user.id)?;
        user.summoners = Some(query.all().await?.results()?);
    }

    Response::ok(format!("{:#?}", response1))
}
