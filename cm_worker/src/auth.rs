//! Authentication-related stuff (oauth2 and utilities).
use secrecy::{ExposeSecret, SecretString};
use serde_with::serde_as;
use url::Url;
use web_time::Duration;
use worker::{Error, Request, Result, RouteContext};

use crate::util::get_reqwest_client;

/// Query `?a=b` data returned to the callback url by the provider after the user authorizes login.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct OauthCallbackQueryResponse {
    /// Code to post to the provider's token endpoint.
    pub code: String,
    /// Issuer.
    pub iss: Option<String>,
    /// Echo'd state.
    pub session_state: Option<String>,
}

/// Form body data posted to the provider's token endpoint.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, serde::Serialize, serde::Deserialize)]
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
    pub fn make_login_link(&self, state: &str) -> Url {
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
    ) -> Result<OauthTokenResponse> {
        let callback_data: OauthCallbackQueryResponse = req.query()?;

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
        Ok(tokens)
    }
}
