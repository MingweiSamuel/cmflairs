//! Helper utilities.

use std::sync::{Once, OnceLock};

use riven::reqwest::Client;
use riven::RiotApi;
use secrecy::{ExposeSecret, SecretString};
use web_sys::console;
use worker::{console_error, console_log, Env, Error, Result};

use crate::auth::OauthClient;

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

/// Initialize and return the [`RiotApi`] instance, if not already initialized.
pub fn get_rgapi(env: &Env) -> &'static RiotApi {
    static ONCE: OnceLock<RiotApi> = OnceLock::new();
    ONCE.get_or_init(|| {
        let rgapi = RiotApi::new(env.secret("RGAPI_KEY").unwrap().to_string());
        log::info!("rgapi initialized");
        rgapi
    })
}

/// Initialize and return the [`RiotApi`] instance, if not already initialized.
pub fn get_reqwest_client(env: &Env) -> Result<&'static Client> {
    static ONCE: OnceLock<Result<Client>> = OnceLock::new();
    ONCE.get_or_init(|| {
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
        let client = Client::builder()
            .user_agent(user_agent)
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {}", e))?;
        Ok(client)
    })
    .as_ref()
    .map_err(|s| Error::RustError(s.to_string()))
}

/// Gets the [`OauthClient`] for Riot Sign On (RSO).
pub fn get_rso_oauth_client(env: &Env) -> Result<&'static OauthClient> {
    static ONCE: OnceLock<Result<OauthClient>> = OnceLock::new();
    ONCE.get_or_init(|| {
        let client = OauthClient {
            client_id: envvar(env, "RSO_CLIENT_ID")?,
            client_secret: secret(env, "RSO_CLIENT_SECRET")?,
            provider_authorize_url: envvar(env, "RSO_PROVIDER_AUTHORIZE_URL")?,
            provider_token_url: envvar(env, "RSO_PROVIDER_TOKEN_URL")?,
            callback_url: envvar(env, "RSO_CALLBACK_URL")?,
        };
        log::info!("Initializing RSO oauth client: {:#?}", client);
        Ok(client)
    })
    .as_ref()
    .map_err(|s| Error::RustError(s.to_string()))
}

/// Gets the [`OauthClient`] for Reddit.
pub fn get_reddit_oauth_client(env: &Env) -> Result<&'static OauthClient> {
    static ONCE: OnceLock<Result<OauthClient>> = OnceLock::new();
    ONCE.get_or_init(|| {
        let client = OauthClient {
            client_id: envvar(env, "REDDIT_CLIENT_ID")?,
            client_secret: secret(env, "REDDIT_CLIENT_SECRET")?,
            provider_authorize_url: envvar(env, "REDDIT_PROVIDER_AUTHORIZE_URL")?,
            provider_token_url: envvar(env, "REDDIT_PROVIDER_TOKEN_URL")?,
            callback_url: envvar(env, "REDDIT_CALLBACK_URL")?,
        };
        log::info!("Initializing Reddit oauth client: {:#?}", client);
        Ok(client)
    })
    .as_ref()
    .map_err(|s| Error::RustError(s.to_string()))
}

/// Get an env var.
pub fn envvar(env: &Env, name: &str) -> Result<String> {
    env.var(name).map(|v| v.to_string())
}
/// Get an env secret.
pub fn secret(env: &Env, name: &str) -> Result<SecretString> {
    env.secret(name).map(|v| v.to_string().into())
}
