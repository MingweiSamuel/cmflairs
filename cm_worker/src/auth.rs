//! Authentication-related stuff (oauth2 and utilities).

use std::num::NonZeroU64;

use axum::extract::{FromRef, FromRequestParts};
use axum::response::{IntoResponse, Response};
use axum::{async_trait, Json, RequestPartsExt};
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use hmac::Hmac;
use http::request::Parts;
use http::StatusCode;
use jwt::{SignWithKey, VerifyWithKey};
use rand::{thread_rng, RngCore};
use riven::reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde_with::serde_as;
use sha2::Sha512;
use url::Url;
use web_time::{Duration, SystemTime};

/// Query `?a=b` data returned to the callback url by the provider after the user authorizes login.
#[derive(Debug, serde::Deserialize)]
pub struct OauthCallbackQueryResponse {
    /// Code to post to the provider's token endpoint.
    pub code: String,
    /// Echoed state.
    pub state: String,
    /// Issuer.
    pub iss: Option<String>,
}

/// Form body data posted to the provider's token endpoint.
#[derive(Debug, serde::Serialize)]
pub struct OauthTokenRequest<'a> {
    /// `"authorization_code"`.
    pub grant_type: &'static str,
    /// Code from the callback.
    pub code: &'a str,
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
pub struct OauthHelper {
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
impl OauthHelper {
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
        reqwest_client: &Client,
        jwt_hmac: &Hmac<Sha512>,
        callback_data: &OauthCallbackQueryResponse,
    ) -> Result<OauthTokenResponse, AuthError> {
        let session_state = verify_session_state_token(jwt_hmac, &callback_data.state)?;
        let SessionState::Anonymous = session_state else {
            return Err(AuthError::MissingCredentials);
        };

        let request = reqwest_client
            .post(&self.provider_token_url)
            .basic_auth(&self.client_id, Some(self.client_secret.expose_secret()))
            .form(&OauthTokenRequest {
                grant_type: "authorization_code",
                code: &callback_data.code,
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
        let response = reqwest_client
            .execute(request)
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| AuthError::TokenCreation(e.to_string()))?; // Ensure non-2xx codes error.

        Ok(response
            .json()
            .await
            .map_err(|e| AuthError::TokenCreation(e.to_string()))?)
    }
}

/// Authorization error.
#[derive(Debug)]
pub enum AuthError {
    /// 401.
    Unauthorized(String),
    /// 400.
    MissingCredentials,
    /// 500.
    TokenCreation(String),
    /// 400.
    InvalidToken,
    /// 503.
    UpstreamError,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, &*format!("Unauthorized: {}", msg))
            }
            AuthError::MissingCredentials => (StatusCode::BAD_REQUEST, "Missing credentials"),
            AuthError::TokenCreation(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                &*format!("Token creation error: {}", msg),
            ),
            AuthError::InvalidToken => (StatusCode::BAD_REQUEST, "Invalid token"),
            AuthError::UpstreamError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to communicate with oauth provider",
            ),
        };
        let body = Json(serde_json::json!({
            "error": error_message,
        }));
        (status, body).into_response()
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
    #[serde(rename = "TRANSITION")]
    Transition {
        /// User ID to be signed-in.
        user_id: NonZeroU64,
    },

    /// User login session token.
    #[serde(rename = "SIGNEDIN")]
    SignedIn {
        /// User ID this is signed-in.
        user_id: NonZeroU64,
    },
}
impl SessionState {
    /// Time to live for each type of session.
    pub fn ttl(self) -> Duration {
        match self {
            SessionState::Anonymous { .. } => Duration::from_secs(24 * 60 * 60),
            SessionState::Transition { .. } => Duration::from_secs(60),
            SessionState::SignedIn { .. } => Duration::from_secs(3 * 60 * 60),
        }
    }
}
#[async_trait]
impl<S> FromRequestParts<S> for SessionState
where
    S: Send + Sync,
    &'static Hmac<Sha512>: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AuthError::InvalidToken)?;
        // Decode the user data
        verify_session_state_token(FromRef::from_ref(state), bearer.token())
    }
}

/// [`SessionState::Anonymous`]
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionStateAnonymous;
// TODO: cleanup boilerplate.
#[async_trait]
impl<S> FromRequestParts<S> for SessionStateAnonymous
where
    S: Send + Sync,
    &'static Hmac<Sha512>: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        if let SessionState::Anonymous = SessionState::from_request_parts(parts, state).await? {
            Ok(SessionStateAnonymous)
        } else {
            Err(AuthError::Unauthorized(
                "Session state must by anonymous.".to_owned(),
            ))
        }
    }
}

/// [`SessionState::Transition`]
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct SessionStateTransition {
    /// User ID to be signed-in.
    pub user_id: NonZeroU64,
}
// TODO: cleanup boilerplate.
#[async_trait]
impl<S> FromRequestParts<S> for SessionStateTransition
where
    S: Send + Sync,
    &'static Hmac<Sha512>: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        if let SessionState::Transition { user_id } =
            SessionState::from_request_parts(parts, state).await?
        {
            Ok(SessionStateTransition { user_id })
        } else {
            Err(AuthError::Unauthorized(
                "Session state must by transition.".to_owned(),
            ))
        }
    }
}

/// [`SessionState::SignedIn`]
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct SessionStateSignedIn {
    /// User ID that is signed-in.
    pub user_id: NonZeroU64,
}
// TODO: cleanup boilerplate.
#[async_trait]
impl<S> FromRequestParts<S> for SessionStateSignedIn
where
    S: Send + Sync,
    &'static Hmac<Sha512>: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        if let SessionState::SignedIn { user_id } =
            SessionState::from_request_parts(parts, state).await?
        {
            Ok(SessionStateSignedIn { user_id })
        } else {
            Err(AuthError::Unauthorized(
                "Session state must by signed in.".to_owned(),
            ))
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
    /// Not before time.
    #[serde_as(as = "crate::with::WebSystemTime<serde_with::TimestampSeconds<i64>>")]
    nbf: SystemTime,
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
        let nbf = iat - Duration::from_secs(10);
        let exp = iat + session_state.ttl();

        let mut nonce = [0; 16];
        thread_rng().fill_bytes(&mut nonce);

        Self {
            nonce,
            iat,
            nbf,
            exp,
            session_state,
        }
    }

    /// Checks that the token is valid right now.
    pub fn check_now(&self) -> Result<(), AuthError> {
        let now = SystemTime::now();
        if now < self.nbf || self.exp < now {
            return Err(AuthError::Unauthorized(
                "Token time is invalid (expired).".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Create a user session token for the given `user_id`, expiring in some amount of time.
pub fn create_session_state_token(
    jwt_hmac: &Hmac<Sha512>,
    session_state: SessionState,
) -> Result<String, AuthError> {
    let claims = JwtSessionState::create_now(session_state);
    let token = claims
        .sign_with_key(jwt_hmac)
        .map_err(|e| AuthError::TokenCreation(e.to_string()))?;
    Ok(token)
}

/// Verifies that the session token is valid. Returns the [`SessionState`] if valid, otherwise
/// returns an error.
pub fn verify_session_state_token(
    jwt_hmac: &Hmac<Sha512>,
    token: &str,
) -> Result<SessionState, AuthError> {
    let claims: JwtSessionState = token
        .verify_with_key(jwt_hmac)
        .map_err(|_| AuthError::InvalidToken)?;
    let () = claims.check_now()?;
    Ok(claims.session_state)
}
