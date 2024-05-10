#![warn(missing_docs)]
#![feature(once_cell_try)]

//! Cloudflare worker.

use std::future::{ready, Ready};
use std::num::NonZeroU64;

use auth::{AuthError, OauthCallbackQueryResponse, SessionStateSignedIn, SessionStateTransition};
pub use axum;
use axum::extract::{Query, State};
use axum::response::Redirect;
use axum::{routing, Json};
use cm_macro::local_async;
use futures::future::join_all;
use hmac::Hmac;
use http::header::AUTHORIZATION;
use http::HeaderValue;
use init::{CmPagesOrigin, RedditOauthHelper, RsoOauthHelper};
use riven::consts::{Champion, PlatformRoute, RegionalRoute};
use riven::reqwest::Client;
use serde_with::de::DeserializeAsWrap;
use serde_with::{serde_as, Same};
use sha2::Sha512;
use tower::Service;
use tower_http::cors::CorsLayer;
use web_time::SystemTime;
use worker::{event, query, Context, D1Database, Env, Error, MessageBatch, MessageExt, Result};

use crate::auth::{create_session_state_token, SessionState};
use crate::error::CmError;
use crate::with::IgnoreKeys;

pub mod auth;
pub mod base36;
pub mod init;
pub mod reddit;
#[macro_use]
pub mod local_future;
pub mod error;
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
    init::init_logging();
    let app_state = init::get_appstate(&env)?;

    let futures = message_batch.messages()?.into_iter().map(|msg| {
        log::info!("Handling webjob task: `{:?}`.", msg.body());
        webjob::handle(
            &app_state.db,
            &app_state.riot_api,
            &app_state.webjob_config,
            msg,
        )
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
#[event(fetch)]
pub async fn fetch(
    req: http::Request<worker::Body>,
    env: Env,
    _ctx: Context,
) -> Result<http::Response<axum::body::Body>> {
    init::init_logging();
    let app_state = init::get_appstate(&env)?;

    let router = axum::Router::new();
    let mut app = router
        .route("/", routing::get(get_index))
        .route("/signin/anonymous", routing::get(get_signin_anonymous))
        .route("/signin/upgrade", routing::get(get_signin_upgrade))
        .route(
            "/signin/reddit",
            routing::get(
                |State(RedditOauthHelper(oauth)): State<&'static _>,
                 Query(query_state): Query<QueryState>| {
                    ready(Redirect::temporary(
                        oauth.make_signin_link(&query_state.state).as_str(),
                    ))
                },
            ),
        )
        .route(
            "/signin/rso",
            routing::get(
                |State(RsoOauthHelper(oauth)): State<&'static _>,
                 Query(query_state): Query<QueryState>| {
                    ready(Redirect::temporary(
                        oauth.make_signin_link(&query_state.state).as_str(),
                    ))
                },
            ),
        )
        .route("/signin-reddit", routing::get(get_signin_reddit))
        .route("/user/me", routing::get(get_user_me))
        .layer(
            CorsLayer::new()
                .allow_origin(
                    HeaderValue::from_str(
                        app_state.cm_pages_origin.0.as_str().trim_end_matches('/'),
                    )
                    .unwrap(),
                )
                .allow_headers([AUTHORIZATION]),
        )
        .with_state(app_state);

    Ok(app.call(req).await.unwrap())
}

#[axum::debug_handler(state = init::AppState)]
fn get_index(State(CmPagesOrigin(url)): State<&'static CmPagesOrigin>) -> Ready<Redirect> {
    ready(Redirect::temporary(url.as_str()))
}

#[axum::debug_handler(state = init::AppState)]
fn get_signin_anonymous(State(jwt_hmac): State<&'static Hmac<Sha512>>) -> Ready<Json<String>> {
    ready(Json(
        create_session_state_token(jwt_hmac, SessionState::Anonymous).unwrap(),
    ))
}

#[axum::debug_handler(state = init::AppState)]
async fn get_signin_upgrade(
    State(jwt_hmac): State<&'static Hmac<Sha512>>,
    SessionStateTransition { user_id }: SessionStateTransition,
) -> std::result::Result<Json<String>, AuthError> {
    let token = create_session_state_token(jwt_hmac, SessionState::SignedIn { user_id })?;
    Ok(Json(token))
}

/// Helper to parse `?state=...`.
#[derive(serde::Deserialize)]
pub struct QueryState {
    state: String,
}

/// `GET /signin-reddit`
#[axum::debug_handler(state = init::AppState)]
#[local_async]
pub async fn get_signin_reddit(
    State(RedditOauthHelper(oauth)): State<&'static RedditOauthHelper>,
    State(reqwest_client): State<&'static Client>,
    State(db): State<&'static D1Database>,
    State(jwt_hmac): State<&'static Hmac<Sha512>>,
    State(CmPagesOrigin(pages_origin)): State<&'static CmPagesOrigin>,
    Query(callback_data): Query<OauthCallbackQueryResponse>,
) -> std::result::Result<Redirect, AuthError> {
    let tokens = oauth
        .handle_callback(reqwest_client, jwt_hmac, &callback_data)
        .await?;
    log::info!("Reddit tokens: {:#?}", tokens);
    let reddit_me = reddit::get_me(reqwest_client, &tokens.access_token)
        .await
        .map_err(|_| AuthError::UpstreamError)?;
    log::info!("Reddit me: {:#?}", reddit_me);

    let user_id = create_or_get_db_user(db, &reddit_me)
        .await
        .map_err(|e| AuthError::TokenCreation(e.to_string()))?;
    let user_signin_token =
        create_session_state_token(jwt_hmac, SessionState::Transition { user_id })?;

    let mut url = pages_origin.clone();
    url.query_pairs_mut().extend_pairs([
        ("token", &user_signin_token),
        ("state", &callback_data.state),
    ]);
    Ok(Redirect::temporary(url.as_str()))
}

// /// `GET /signin-rso`
// pub async fn signin_rso_get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
//     let tokens = get_rso_oauth_client(&ctx.env)?
//         .handle_callback(req, &ctx)
//         .await?;
//     Response::from_html(format!("<code>{:#?}</code>", tokens))
// }

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
    #[serde(
        deserialize_with = "serde_with::As::<Option<serde_with::json::JsonString>>::deserialize"
    )]
    champ_scores: Option<Vec<ChampScore>>,
}

/// `GET /user/me`
#[axum::debug_handler(state = init::AppState)]
#[local_async]
pub async fn get_user_me(
    State(db): State<&'static D1Database>,
    SessionStateSignedIn { user_id }: SessionStateSignedIn,
) -> std::result::Result<Json<User>, CmError> {
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

    let mut user: User = user_result.results()?.into_iter().next().ok_or_else(|| {
        CmError::InternalServerError(format!(
            "User with ID {} does not exist. This should not happen - invalid session.",
            user_id
        ))
    })?;
    user.summoners = summoners_result.results()?.into_iter().collect::<Vec<_>>();
    Ok(Json(user))
}

// /// `GET /test`
// pub async fn test_get(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
//     let queue = ctx.env.queue("BINDING_QUEUE_WEBJOB").unwrap();
//     queue.send(webjob::Task::SummonerBulkUpdate).await?;

//     let db = ctx.env.d1("BINDING_D1_DB").unwrap();

//     type UserWrap = DeserializeAsWrap<
//         (u64, String, bool, Option<u64>),
//         IgnoreKeys<(Same, Same, BoolFromInt, Same)>,
//     >;
//     let query = query!(
//         &db,
//         "SELECT id, reddit_user_name, profile_is_public, profile_bgskinid FROM user"
//     );
//     let response1 = query
//         .all()
//         .await?
//         .results()?
//         .into_iter()
//         .map(UserWrap::into_inner)
//         .collect::<Vec<_>>();

//     let mut summoners = Vec::new();
//     for &(id, ..) in response1.iter() {
//         type SummonerWrap = DeserializeAsWrap<
//             (
//                 u64,
//                 u64,
//                 String,
//                 PlatformRoute,
//                 String,
//                 String,
//                 Option<SystemTime>,
//                 Option<Vec<ChampScore>>,
//             ),
//             IgnoreKeys<(
//                 Same,
//                 Same,
//                 Same,
//                 DisplayFromStr,
//                 Same,
//                 Same,
//                 Option<WebSystemTime<TimestampMilliSeconds<i64>>>,
//                 Option<JsonString>,
//             )>,
//         >;
//         let query = query!(&db, "SELECT id, user_id, puuid, platform, game_name, tag_line, last_update, champ_scores FROM summoner WHERE user_id = ?1", id)?;
//         summoners.push(
//             query
//                 .all()
//                 .await?
//                 .results()?
//                 .into_iter()
//                 .map(SummonerWrap::into_inner)
//                 .collect::<Vec<_>>(),
//         );
//     }

//     Response::ok(format!("{:#?}\n\n{:#?}", response1, summoners))
// }

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

// TODO: update return Result type.
/// Create or gets a DB user from the Reddit user.
pub async fn create_or_get_db_user(db: &D1Database, reddit_me: &reddit::Me) -> Result<NonZeroU64> {
    if reddit_me.can_edit_name {
        return Result::Err(Error::RustError(format!(
            "Cannot add new user with editable name: /u/{}.",
            reddit_me.name
        )));
    }

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
