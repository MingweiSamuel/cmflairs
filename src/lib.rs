#![warn(missing_docs)]

//! Cloudflare worker.

use worker::{event, query, Context, Env, MessageBatch, Request, Response, Result};

pub mod db;
pub mod util;

/// Cloudflare queue handler.
#[event(queue)]
pub async fn queue(message_batch: MessageBatch<String>, _env: Env, _ctx: Context) -> Result<()> {
    util::init_logging();

    for message in message_batch.messages()? {
        let message = message.into_body();
        log::info!("Received webjob message: {}", message,);
    }

    Ok(())
}

/// Cloudflare fetch request handler.
#[event(fetch, respond_with_errors)]
pub async fn fetch(_req: Request, env: Env, _ctx: Context) -> Result<Response> {
    util::init_logging();

    let d1db = env.d1("BINDING_D1_DB").unwrap();
    let query = query!(&d1db, "SELECT * FROM user",)?;
    let response: Vec<db::User> = query.all().await?.results()?;

    Response::ok(format!("{:#?}", response))
}
