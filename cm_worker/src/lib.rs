#![warn(missing_docs)]

//! Cloudflare worker.

use std::num::NonZeroU64;

use futures::future::join_all;
use riven::consts::{Champion, PlatformRoute, RegionalRoute};
use serde_with::de::DeserializeAsWrap;
use serde_with::json::JsonString;
use serde_with::{serde_as, BoolFromInt, DisplayFromStr, Same, TimestampMilliSeconds};
use util::{get_reddit_oauth_client, get_rso_oauth_client};
use web_time::SystemTime;
use worker::{
    event, query, Context, Env, Error, MessageBatch, MessageExt, Request, Response, Result,
    RouteContext, Router,
};

use crate::auth::{
    create_session_state_token, verify_authorization_bearer_token, verify_session_state_token,
    SessionState,
};
use crate::reddit::get_me;
use crate::util::get_pages_origin;
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

    #[derive(serde::Serialize, serde::Deserialize)]
    pub struct State {
        state: String,
    }

    let pages_origin = get_pages_origin(&env)?;

    let router = Router::new();
    let response = router
        .get("/", |_req, ctx| {
            Response::redirect(get_pages_origin(&ctx.env)?)
        })
        .options("/signin/anonymous", |_, _| Response::empty())
        .get("/signin/anonymous", |_req, ctx| {
            Response::from_json(&create_session_state_token(
                &ctx.env,
                SessionState::Anonymous,
            )?)
        })
        .options("/signin/upgrade", |_, _| Response::empty())
        .get("/signin/upgrade", |req, ctx| {
            let session_state = verify_authorization_bearer_token(&ctx.env, &req)?;
            let SessionState::Signin { user_id } = session_state else {
                return Err(Error::RustError("Session state must be SIGNIN.".to_owned()));
            };
            let token = create_session_state_token(&ctx.env, SessionState::Session { user_id })?;
            Response::from_json(&token)
        })
        .get("/signin/reddit", |req, ctx| {
            let State { state } = req.query()?;
            let session_state = verify_session_state_token(&ctx.env, &state)?;
            let () = matches!(session_state, SessionState::Anonymous)
                .then_some(())
                .ok_or("Session state must be ANONYMOUS.")?;
            Response::redirect(get_reddit_oauth_client(&ctx.env)?.make_signin_link(&state))
        })
        .get_async("/signin-reddit", signin_reddit_get)
        .get("/signin/rso", |_req, ctx| {
            Response::redirect(get_rso_oauth_client(&ctx.env)?.make_signin_link("foobar"))
        })
        .get_async("/signin-rso", signin_rso_get)
        .options("/user/me", |_, _| Response::empty())
        .get_async("/user/me", user_me_get)
        .options("/summoner/update", |_, _| Response::empty())
        .post_async("/summoner/update", |mut req, ctx| async move {
            let id: u64 = req.json().await?;
            let queue = ctx.env.queue("BINDING_QUEUE_WEBJOB").unwrap();
            queue.send(webjob::Task::SummonerUpdate(id)).await?;
            Response::empty()
        })
        .run(req, env)
        .await;

    let mut response = response?;
    if 302 != response.status_code() {
        let headers = response.headers_mut();
        headers.append(
            "Access-Control-Allow-Origin",
            pages_origin.as_str().trim_end_matches('/'),
        )?;
        headers.append("Access-Control-Allow-Headers", "Authorization")?;
    }
    Ok(response)
}

/// `GET /`
pub fn index_get(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let reddit_signin_link = get_reddit_oauth_client(&ctx.env)?.make_signin_link("foobar");
    Response::from_html(format!(
        r##"
<a id="signin-reddit" href="#">Sign In With Reddit</a>
<script type="text/javascript">
    const loginNonce = "" + crypto.getRandomValues(new Uint32Array(1))[0];
    localStorage.setItem('login_nonce', loginNonce);
    const loginUrl = new URL('{}');
    loginUrl.searchParams.set('state', loginNonce);
    document.getElementById('signin-reddit').href = loginUrl.href;
</script>
"##,
        reddit_signin_link
    ))
}

/// `GET /signin-reddit`
pub async fn signin_reddit_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let (tokens, state) = get_reddit_oauth_client(&ctx.env)?
        .handle_callback(req, &ctx)
        .await?;
    log::info!("Reddit tokens: {:#?}", tokens);
    let reddit_me = get_me(&ctx.env, &tokens.access_token).await?;
    log::info!("Reddit me: {:#?}", reddit_me);

    let user_id = create_or_get_db_user(&ctx.env, &reddit_me).await?;
    let user_signin_token = create_session_state_token(&ctx.env, SessionState::Signin { user_id })?;

    let mut url = get_pages_origin(&ctx.env)?;
    url.query_pairs_mut()
        .extend_pairs([("token", user_signin_token), ("state", state)]);
    Response::redirect(url)

    //     Response::from_html(format!(
    //         r##"
    // <script type="text/javascript">
    //     const loginNonce = localStorage.getItem('login_nonce');
    //     const state = new URL(document.location).searchParams.get('state');
    //     if (loginNonce === state) {{
    //         localStorage.setItem('login_token', '{}');
    //     }}
    // </script>
    // "##,
    //         user_signin_token
    //     ))
}

/// `GET /signin-rso`
pub async fn signin_rso_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let tokens = get_rso_oauth_client(&ctx.env)?
        .handle_callback(req, &ctx)
        .await?;
    Response::from_html(format!("<code>{:#?}</code>", tokens))
}

/// `GET /user/me`
pub async fn user_me_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session_state = verify_authorization_bearer_token(&ctx.env, &req)?;
    let SessionState::Session { user_id } = session_state else {
        return Response::from_json(&"Unauthorized").map(|r| r.with_status(401));
    };

    let db = ctx.env.d1("BINDING_D1_DB").unwrap();
    let user_query = query!(
        &db,
        "SELECT reddit_user_name, profile_is_public, profile_bgskinid
        FROM user
        WHERE id = ?",
        user_id,
    )?;
    let summoners_query = query!(
        &db,
        "SELECT id, puuid, platform, game_name, tag_line, last_update, champ_scores
        FROM summoner
        WHERE user_id = ?1",
        user_id,
    )?;

    let [user_result, summoners_result] = &db.batch(vec![user_query, summoners_query]).await?[..]
    else {
        unreachable!();
    };

    let mut user: User = user_result
        .results()?
        .into_iter()
        .next()
        .ok_or("User does not exist. This should not happen - invalid session.")?;
    user.summoners = summoners_result.results()?.into_iter().collect::<Vec<_>>();

    #[serde_as]
    #[derive(serde::Serialize, serde::Deserialize)]
    struct User {
        reddit_user_name: String,
        #[serde_as(as = "serde_with::BoolFromInt")]
        profile_is_public: bool,
        profile_bgskinid: Option<u64>,
        #[serde(skip_deserializing)]
        summoners: Vec<Summoner>,
    }
    #[serde_as]
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Summoner {
        id: u64,
        puuid: String,
        #[serde_as(as = "serde_with::DisplayFromStr")]
        platform: PlatformRoute,
        game_name: String,
        tag_line: String,
        #[serde_as(as = "Option<crate::with::WebSystemTime<serde_with::TimestampSeconds<i64>>>")]
        last_update: Option<SystemTime>,
        #[serde(deserialize_with = "serde_with::As::<Option<serde_with::json::JsonString>>::deserialize")]
        champ_scores: Option<Vec<ChampScore>>,
    }

    Response::from_json(&user)
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

async fn create_or_get_db_user(env: &Env, reddit_me: &reddit::Me) -> Result<NonZeroU64> {
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
    Ok(id.into_inner().0.try_into().unwrap())
}
