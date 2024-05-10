//! Helper utilities.

use std::sync::{Once, OnceLock};

use cm_macro::FromRefStatic;
use hmac::Hmac;
use riven::reqwest::Client;
use riven::RiotApi;
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha512;
use url::Url;
use web_sys::console;
use worker::{console_error, console_log, D1Database, Env, Error, Result};

use crate::auth::OauthHelper;
use crate::webjob::WebjobConfig;

/// Initialize [`log`] logging into Cloudflare's [`console`] logging system, if not already
/// initialized.
pub fn init_logging() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        {
            fn hook(info: &std::panic::PanicInfo) {
                console_error!("{}", info);
            }
            std::panic::set_hook(Box::new(hook));
            console_log!("[panic hook set]");
        }
        {
            struct ConsoleLog;
            static LOG: ConsoleLog = ConsoleLog;
            impl log::Log for ConsoleLog {
                fn enabled(&self, _metadata: &log::Metadata) -> bool {
                    true // TODO
                }

                fn log(&self, record: &log::Record) {
                    let method = match record.level() {
                        log::Level::Error => console::error_1,
                        log::Level::Warn => console::warn_1,
                        log::Level::Info => console::info_1,
                        log::Level::Debug => console::debug_1,
                        log::Level::Trace => console::trace_1,
                    };
                    (method)(
                        &format!(
                            "[{} {}] {}",
                            record.level(),
                            record.module_path().unwrap_or("?"),
                            record.args()
                        )
                        .into(),
                    );
                }

                fn flush(&self) {}
            }
            log::set_logger(&LOG).unwrap();
            log::set_max_level(log::LevelFilter::Trace); // TODO

            log::info!("logger set");
        }
    });
}

/// `AppState`. Static reference to [`AppStateOwned`] to avoid cloning in Axum.
pub type AppState = &'static AppStateOwned;
/// State for the application, used as the Axum router state.
#[derive(FromRefStatic)]
pub struct AppStateOwned {
    /// Database.
    pub db: D1Database,
    /// Riot API client.
    pub riot_api: RiotApi,
    /// General/Reddit API client.
    pub reqwest_client: Client,
    /// Reddit Oauth helper.
    pub reddit_oauth: RedditOauthHelper,
    /// RSO Oauth helper.
    pub rso_oauth: RsoOauthHelper,
    /// HMAC for signing JWTs.
    pub jwt_hmac: Hmac<Sha512>,
    /// Origin (with trailing slash) for `cm_pages` static site.
    pub cm_pages_origin: CmPagesOrigin,
    /// See [`crate::webjob::Task::SummonerBulkUpdate`].
    pub webjob_config: WebjobConfig,
}

/// Get the AppState, initializing it if needed.
pub fn get_appstate(env: &Env) -> worker::Result<AppState> {
    static ONCE: OnceLock<AppStateOwned> = OnceLock::new();
    ONCE.get_or_try_init(|| {
        let db = env.d1("BINDING_D1_DB").unwrap();
        let riot_api = RiotApi::new(env.secret("RGAPI_KEY").unwrap().to_string());
        let reqwest_client = {
            let user_agent = format!(
                "cmflairs:{client_id}:{version} (by /u/{reddit_user})",
                client_id = secret(env, "REDDIT_CLIENT_ID")?.expose_secret(),
                version = option_env!("GIT_HASH").unwrap_or("localdev"),
                reddit_user = secret(env, "REDDIT_OWNER_USERNAME")?.expose_secret(),
            );
            log::info!(
                "Initializing reqwest client with user agent: {:?}",
                user_agent
            );
            Client::builder()
                .user_agent(user_agent)
                .build()
                .map_err(|e| format!("Failed to build reqwest client: {}", e))?
        };
        let reddit_oauth = RedditOauthHelper(OauthHelper {
            client_id: envvar(env, "REDDIT_CLIENT_ID")?,
            client_secret: secret(env, "REDDIT_CLIENT_SECRET")?,
            provider_authorize_url: envvar(env, "REDDIT_PROVIDER_AUTHORIZE_URL")?,
            provider_token_url: envvar(env, "REDDIT_PROVIDER_TOKEN_URL")?,
            callback_url: envvar(env, "REDDIT_CALLBACK_URL")?,
        });
        let rso_oauth = RsoOauthHelper(OauthHelper {
            client_id: envvar(env, "RSO_CLIENT_ID")?,
            client_secret: secret(env, "RSO_CLIENT_SECRET")?,
            provider_authorize_url: envvar(env, "RSO_PROVIDER_AUTHORIZE_URL")?,
            provider_token_url: envvar(env, "RSO_PROVIDER_TOKEN_URL")?,
            callback_url: envvar(env, "RSO_CALLBACK_URL")?,
        });
        let jwt_hmac = {
            let secret = secret(env, "HMAC_SECRET")?;
            let secret = base64::decode_config(secret.expose_secret(), base64::URL_SAFE_NO_PAD)
                .map_err(|e| format!("Failed to decode `HMAC_SECRET`: {}", e))?;
            if secret.len() < 32 {
                return Result::Err(Error::RustError(format!(
                    "`HMAC_SECRET` is too short, len: {}",
                    secret.len(),
                )));
            }
            hmac::Mac::new_from_slice(&secret)
                .map_err(|e| format!("Failed to create hmac: {}", e))?
        };
        let cm_pages_origin = CmPagesOrigin(
            Url::parse(&envvar(env, "PAGES_ORIGIN")?)
                .map_err(|e| format!("Invalid url in `PAGES_ORIGIN`: {}", e))?,
        );
        let webjob_config = WebjobConfig {
            bulk_update_batch_size: envvar(env, "WEBJOB_BULK_UPDATE_BATCH_SIZE")?
                .parse()
                .map_err(|e| Error::RustError(format!("Env var `WEBJOB_BULK_UPDATE_BATCH_SIZE` should be a positive integer string: {}", e)))?,
        };
        Ok(AppStateOwned {
            db,
            riot_api,
            reqwest_client,
            reddit_oauth,
            rso_oauth,
            jwt_hmac,
            cm_pages_origin,
            webjob_config,
        })
    })
}

/// Wraper to distinguish Axum states.
pub struct RedditOauthHelper(pub OauthHelper);
/// Wraper to distinguish Axum states.
pub struct RsoOauthHelper(pub OauthHelper);
/// Wraper to distinguish Axum states.
pub struct CmPagesOrigin(pub Url);

/// Get an env var.
pub fn envvar(env: &Env, name: &str) -> Result<String> {
    env.var(name).map(|v| v.to_string())
}
/// Get an env secret.
pub fn secret(env: &Env, name: &str) -> Result<SecretString> {
    env.secret(name).map(|v| v.to_string().into())
}
