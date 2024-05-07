//! Authentication-related stuff (oauth2 and utilities).

use std::num::NonZeroU64;

use jwt::{SignWithKey, VerifyWithKey};
use rand::{thread_rng, RngCore};
use secrecy::{ExposeSecret, SecretString};
use serde_with::serde_as;
use url::Url;
use web_time::{Duration, SystemTime};
use worker::{Env, Error, Request, Result, RouteContext};

use crate::util::{get_jwt_hmac, get_reqwest_client};

/// Query `?a=b` data returned to the callback url by the provider after the user authorizes login.
#[derive(Debug, serde::Deserialize)]
pub struct OauthCallbackQueryResponse {
    /// Code to post to the provider's token endpoint.
    pub code: String,
    /// Echoed state.
    pub state: Option<String>,
    /// Issuer.
    pub iss: Option<String>,
}

/// Form body data posted to the provider's token endpoint.
#[derive(Debug, serde::Serialize)]
pub struct OauthTokenRequest<'a> {
    /// `"authorization_code"`.
    pub grant_type: &'static str,
    /// Code from the callback.
    pub code: String,
    /// Redirect for the token request (not useful?).
    pub redirect_uri: &'a str,
}

/// JSON body data returned by the provider's token endpoint.
#[serde_as]
#[derive(Debug, serde::Deserialize)]
pub struct OauthTokenResponse {
    /// The access token.
    pub access_token: String,
    /// Refresh token which may be used to create new access tokens.
    pub refresh_token: Option<String>,
    /// List of oauth scopes.
    #[serde_as(
        as = "serde_with::StringWithSeparator::<serde_with::formats::SpaceSeparator, String>"
    )]
    pub scope: Vec<String>,
    /// Identity token (RSO).
    pub id_token: Option<String>,
    /// `"bearer"`.
    pub token_type: String,
    /// How long until `access_token` expires.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    pub expires_in: Duration,
}

/// Helper for managing oauth authentication.
#[derive(Debug)]
pub struct OauthClient {
    /// Client app's ID.
    pub client_id: String,
    /// Client app's secret.
    pub client_secret: SecretString,
    /// Provider's authorization endpoint.
    pub provider_authorize_url: String,
    /// Provider's token endpoint.
    pub provider_token_url: String,
    /// Client's callback url.
    pub callback_url: String,
}
impl OauthClient {
    /// Creates the URL for the authorization endpoint.
    pub fn make_signin_link(&self, state: &str) -> Url {
        Url::parse_with_params(
            &self.provider_authorize_url,
            [
                ("response_type", "code"),
                ("scope", "identity"),
                ("redirect_uri", &self.callback_url),
                ("client_id", &self.client_id),
                ("duration", "temporary"),
                ("state", state),
            ],
        )
        .unwrap()
    }

    /// Handler for the callback at [`Self::callback_url`].
    pub async fn handle_callback(
        &self,
        req: Request,
        ctx: &RouteContext<()>,
    ) -> Result<(OauthTokenResponse, String)> {
        let callback_data: OauthCallbackQueryResponse = req.query()?;

        let state = callback_data.state.ok_or("Received no echoed `state`.")?;
        let session_state = verify_session_state_token(&ctx.env, &state)?;
        let () = matches!(session_state, SessionState::Anonymous)
            .then_some(())
            .ok_or("Session state must be ANONYMOUS.")?;

        log::info!(
            "{:#?}",
            OauthTokenRequest {
                grant_type: "authorization_code",
                code: callback_data.code.clone(),
                redirect_uri: &self.callback_url,
            }
        );

        let request = get_reqwest_client(&ctx.env)?
            .post(&self.provider_token_url)
            .basic_auth(&self.client_id, Some(self.client_secret.expose_secret()))
            .form(&OauthTokenRequest {
                grant_type: "authorization_code",
                code: callback_data.code,
                redirect_uri: &self.callback_url,
            })
            .build()
            .unwrap();
        log::info!(
            "REQ: {:#?}\n{:#?}",
            request,
            request
                .body()
                .and_then(|b| b.as_bytes())
                .map(|b| std::str::from_utf8(b))
        );
        let response = get_reqwest_client(&ctx.env)?.execute(request)
            .await
            .and_then(|r| r.error_for_status()) // Ensure non-2xx codes error.
            .map_err(|e| {
                Error::RustError(format!(
                    "Request to `{}` failed: {}",
                    self.provider_token_url, e,
                ))
            })?;

        let tokens: OauthTokenResponse = response.json().await.map_err(|e| {
            Error::RustError(format!(
                "Failed to parse `{}` response: {}",
                self.provider_token_url, e,
            ))
        })?;
        Ok((tokens, state))
    }
}

/// Session token types.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum SessionState {
    /// Pre-session token issued to prevent login CSRF.
    #[serde(rename = "ANONYMOUS")]
    Anonymous,

    /// Short-lived sign-in token, to be exchanged for a [`Self::Session`] token.
    #[serde(rename = "SIGNIN")]
    Signin {
        /// User ID to be signed-in.
        user_id: NonZeroU64,
    },

    /// User login session token.
    #[serde(rename = "SESSION")]
    Session {
        /// User ID of signed-in user.
        user_id: NonZeroU64,
    },
}
impl SessionState {
    /// Time to live for each type of session.
    pub fn ttl(self) -> Duration {
        match self {
            SessionState::Anonymous => Duration::from_secs(24 * 60 * 60),
            SessionState::Signin { .. } => Duration::from_secs(60),
            SessionState::Session { .. } => Duration::from_secs(3 * 60 * 60),
        }
    }
}

/// User session JWT, for login.
#[serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct JwtSessionState {
    /// Nonce UUID.
    #[serde_as(as = "serde_with::base64::Base64<serde_with::base64::UrlSafe>")]
    nonce: [u8; 16],
    /// Issued-at time.
    #[serde_as(as = "crate::with::WebSystemTime<serde_with::TimestampSeconds<i64>>")]
    iat: SystemTime,
    /// Expiration time.
    #[serde_as(as = "crate::with::WebSystemTime<serde_with::TimestampSeconds<i64>>")]
    exp: SystemTime,
    /// User session state.
    #[serde_as(as = "serde_with::json::JsonString")]
    session_state: SessionState,
}
impl JwtSessionState {
    /// Creates a new token expiring after [`SessionState::ttl`] from now.
    /// Sets a random [`Self::nonce`].
    pub fn create_now(session_state: SessionState) -> Self {
        let iat = SystemTime::now();
        let exp = iat + session_state.ttl();

        let mut nonce = [0; 16];
        thread_rng().fill_bytes(&mut nonce);

        Self {
            nonce,
            iat,
            exp,
            session_state,
        }
    }

    /// Checks that the token is valid right now.
    pub fn check_now(&self) -> Result<()> {
        let now = SystemTime::now();
        (self.iat < now && now < self.exp)
            .then_some(())
            .ok_or("Token is expired.")?;
        Ok(())
    }
}

/// Create a user session token for the given `user_id`, expiring in some amount of time.
pub fn create_session_state_token(env: &Env, session_state: SessionState) -> Result<String> {
    let claims = JwtSessionState::create_now(session_state);
    let token = claims
        .sign_with_key(get_jwt_hmac(env)?)
        .map_err(|e| format!("Failed to sign user session jwt: {}.", e))?;
    Ok(token)
}

/// Verifies that the session token is valid. Returns the [`SessionState`] if valid, otherwise
/// returns an error.
pub fn verify_session_state_token(env: &Env, token: &str) -> Result<SessionState> {
    let claims: JwtSessionState = token
        .verify_with_key(get_jwt_hmac(env)?)
        .map_err(|e| format!("Failed to read/verify user sesssion jwt: {}.", e))?;
    let () = claims.check_now()?;
    Ok(claims.session_state)
}

/// Verify the `Authorization: Bearer ...` token in the requet.
pub fn verify_authorization_bearer_token(env: &Env, req: Request) -> Result<SessionState> {
    let header = req.headers().get("Authorization")?;
    let token = header
        .as_deref()
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or("Expected `Authorization: Bearer ...` header.")?;
    verify_session_state_token(env, token)
}
