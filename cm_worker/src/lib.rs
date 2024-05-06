#![warn(missing_docs)]

//! Cloudflare worker.

use auth::{OauthCallbackQueryResponse, OauthTokenRequest, OauthTokenResponse};
use futures::future::join_all;
use riven::consts::{Champion, PlatformRoute, RegionalRoute};
use secrecy::ExposeSecret;
use serde_with::de::DeserializeAsWrap;
use serde_with::json::JsonString;
use serde_with::{serde_as, BoolFromInt, DisplayFromStr, Same, TimestampMilliSeconds};
use url::Url;
use util::{envvar, get_reddit_oauth_client, get_rso_oauth_client, secret};
use web_time::{Duration, SystemTime};
use worker::{
    event, query, Context, Env, Error, MessageBatch, MessageExt, Request, Response, Result,
    RouteContext, Router,
};

use crate::util::get_reqwest_client;
use crate::with::{IgnoreKeys, WebSystemTime};

pub mod auth;
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
        .get_async("/", index_get)
        .get_async("/signin-reddit", signin_reddit_get)
        .get_async("/signin-rso", signin_rso_get)
        .get_async("/test", test_get)
        .run(req, env)
        .await
}

/// `GET /`
pub async fn index_get(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let rso_url = Url::parse_with_params(
        &envvar(&ctx.env, "RSO_PROVIDER_AUTHORIZE_URL")?,
        [
            ("response_type", "code"),
            ("scope", "openid cpid offline_access"),
            ("redirect_uri", &envvar(&ctx.env, "RSO_CALLBACK_URL")?),
            (
                "client_id",
                secret(&ctx.env, "RSO_CLIENT_ID")?.expose_secret(),
            ),
        ],
    )
    .unwrap();
    let reddit_url = Url::parse_with_params(
        &envvar(&ctx.env, "REDDIT_PROVIDER_AUTHORIZE_URL")?,
        [
            ("response_type", "code"),
            ("scope", "identity"),
            ("redirect_uri", &envvar(&ctx.env, "REDDIT_CALLBACK_URL")?),
            (
                "client_id",
                secret(&ctx.env, "REDDIT_CLIENT_ID")?.expose_secret(),
            ),
            ("duration", "temporary"),
            ("state", "asdf"),
        ],
    )
    .unwrap();

    Response::from_html(format!(
        "<a href=\"{}\">RSO Sign In</a><br><a href=\"{}\">Reddit Sign In</a>",
        rso_url, reddit_url
    ))
}

/// `GET /signin-reddit`
pub async fn signin_reddit_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tokens = get_reddit_oauth_client(&ctx.env)?
        .handle_callback(req, ctx)
        .await;
    Response::from_html(format!("<code>{:#?}</code>", tokens))
}

/// `GET /signin-rso`
pub async fn signin_rso_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tokens = get_rso_oauth_client(&ctx.env)?
        .handle_callback(req, ctx)
        .await;
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
