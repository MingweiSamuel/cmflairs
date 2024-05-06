#![warn(missing_docs)]

//! Cloudflare worker.

use futures::future::join_all;
use riven::consts::{Champion, PlatformRoute, RegionalRoute};
use serde_with::de::DeserializeAsWrap;
use serde_with::json::JsonString;
use serde_with::{serde_as, BoolFromInt, DisplayFromStr, Same, TimestampMilliSeconds};
use url::Url;
use util::{get_rso_callback_url, get_rso_client_id};
use web_time::{Duration, SystemTime};
use with::IgnoreKeys;
use worker::{
    event, query, Context, Env, Error, MessageBatch, MessageExt, Request, Response, Result,
    RouteContext, Router,
};

use crate::util::{get_client, get_rso_client_secret};
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
        .get_async("/signin-rso", signinrso_get)
        .get_async("/test", test_get)
        .run(req, env)
        .await
}

/// `GET /`
pub async fn index_get(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = Url::parse_with_params(
        &ctx.env
            .var("RSO_PROVIDER_AUTHORIZE_URL")
            .unwrap()
            .to_string(),
        [
            ("response_type", "code"),
            ("scope", "openid cpid offline_access"),
            ("redirect_uri", &get_rso_callback_url(&ctx.env)),
            ("client_id", &get_rso_client_id(&ctx.env)),
        ],
    )
    .unwrap();

    Response::from_html(format!("<a href=\"{}\">Sign In</a>", url))
}

/// `GET /signin-rso`
pub async fn signinrso_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct RsoQuery {
        code: String,
        iss: String,
        session_state: String,
    }
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct RsoRequest {
        grant_type: &'static str,
        code: String,
        redirect_uri: String,
    }
    #[serde_as]
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct RsoTokens {
        access_token: String,
        refresh_token: String,
        #[serde_as(as = "serde_with::StringWithSeparator::<serde_with::formats::SpaceSeparator, String>")]
        scope: Vec<String>,
        id_token: Option<String>,
        token_type: String,
        #[serde_as(as = "serde_with::DurationSeconds<u64>")]
        expires_in: Duration,
    }

    let rso_data: RsoQuery = req.query()?;
    let response = get_client()
        .post(ctx.env.var("RSO_PROVIDER_TOKEN_URL").unwrap().to_string())
        .basic_auth(
            get_rso_client_id(&ctx.env),
            Some(get_rso_client_secret(&ctx.env)),
        )
        .form(&RsoRequest {
            grant_type: "authorization_code",
            code: rso_data.code,
            redirect_uri: get_rso_callback_url(&ctx.env),
        })
        .send()
        .await
        .map_err(|e| Error::RustError(format!("Request to RSO `/token` failed: {}", e)))?;

    // Response::from_html(format!("<code>{:#?}</code>", response.text().await))

    let tokens: RsoTokens = response
        .json()
        .await
        .map_err(|e| Error::RustError(format!("Failed to parse RSO `/token` response: {}", e)))?;

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
