#![warn(missing_docs)]

//! Cloudflare worker.

use futures::future::join_all;
use riven::consts::{Champion, PlatformRoute, RegionalRoute};
use serde_with::de::DeserializeAsWrap;
use serde_with::json::JsonString;
use serde_with::{BoolFromInt, DisplayFromStr, Same, TimestampMilliSeconds};
use util::{get_reddit_oauth_client, get_rso_oauth_client};
use web_time::SystemTime;
use worker::{
    event, query, Context, Env, Error, MessageBatch, MessageExt, Request, Response, Result,
    RouteContext, Router,
};

use crate::auth::create_user_session_token;
use crate::reddit::get_me;
use crate::with::{IgnoreKeys, WebSystemTime};

pub mod auth;
pub mod base36;
pub mod reddit;
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

    let futures = message_batch.messages()?.into_iter().map(|msg| {
        log::info!("Handling webjob task: `{:?}`.", msg.body());
        webjob::handle(&env, msg)
    });
    let results = join_all(futures).await;
    let errors = results
        .into_iter()
        .filter_map(|result| result.map(|msg| msg.ack()).err())
        .collect::<Vec<_>>();

    log::info!("Handling webjob task complete. Errors: {:?}", errors);
    errors
        .is_empty()
        .then_some(())
        .ok_or(Error::RustError(format!("{:?}", errors)))
}

/// Cloudflare fetch request handler.
#[event(fetch, respond_with_errors)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    util::init_logging();

    let router = Router::new();
    router
        .get("/", index_get)
        .get("/login-reddit", |_, ctx| {
            Response::redirect(get_reddit_oauth_client(&ctx.env)?.make_login_link("foobar"))
        })
        .get("/login-rso", |_, ctx| {
            Response::redirect(get_rso_oauth_client(&ctx.env)?.make_login_link("foobar"))
        })
        .get_async("/signin-reddit", signin_reddit_get)
        .get_async("/signin-rso", signin_rso_get)
        .get_async("/test", test_get)
        .run(req, env)
        .await
}

/// `GET /`
pub fn index_get(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    Response::from_html(
        "<a href=\"/login-rso\">RSO Sign In</a><br><a href=\"/login-reddit\">Reddit Sign In</a>",
    )
}

/// `GET /signin-reddit`
pub async fn signin_reddit_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tokens = get_reddit_oauth_client(&ctx.env)?
        .handle_callback(req, &ctx)
        .await?;
    log::info!("Reddit tokens: {:#?}", tokens);
    let reddit_me = get_me(&ctx.env, &tokens.access_token).await?;
    log::info!("Reddit me: {:#?}", reddit_me);

    let user_id = create_or_get_db_user(&ctx.env, &reddit_me).await?;
    let user_session_token = create_user_session_token(&ctx.env, user_id).await?;

    Response::from_html(format!(
        "<code>{:#?}</code><br><code>{:#?}</code><br><code>{}</code>",
        tokens,
        reddit_me,
        user_session_token.as_str()
    ))
}

/// `GET /signin-rso`
pub async fn signin_rso_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tokens = get_rso_oauth_client(&ctx.env)?
        .handle_callback(req, &ctx)
        .await?;
    Response::from_html(format!("<code>{:#?}</code>", tokens))
}

/// `GET /test`
pub async fn test_get(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let queue = ctx.env.queue("BINDING_QUEUE_WEBJOB").unwrap();
    queue.send(webjob::Task::SummonerBulkUpdate).await?;

    let db = ctx.env.d1("BINDING_D1_DB").unwrap();

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

async fn create_or_get_db_user(env: &Env, reddit_me: &reddit::Me) -> Result<u64> {
    if reddit_me.can_edit_name {
        return Result::Err(Error::RustError(format!(
            "Cannot add new user with editable name: /u/{}.",
            reddit_me.name
        )));
    }

    let db = env.d1("BINDING_D1_DB").unwrap();
    let query = query!(
        &db,
        "INSERT INTO user(reddit_id, reddit_user_name, profile_is_public)
        VALUES (?, ?, 0)
        ON CONFLICT DO UPDATE SET id=id RETURNING id",
        reddit_me.id,
        reddit_me.name,
    )?;
    let id: DeserializeAsWrap<(u64,), IgnoreKeys<(Same,)>> = query
        .first(None)
        .await?
        .ok_or("Failed to get or insert user")?;
    Ok(id.into_inner().0)
}
