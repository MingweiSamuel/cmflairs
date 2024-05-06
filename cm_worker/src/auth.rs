//! Authentication-related stuff (oauth2 and utilities).
use secrecy::{ExposeSecret, SecretString};
use serde_with::serde_as;
use url::Url;
use web_time::Duration;
use worker::{Error, Request, Response, Result, RouteContext};

use crate::util::get_reqwest_client;

/// Query `?a=b` data returned to the callback url by the provider after the user authorizes login.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct OauthCallbackQueryResponse {
    pub code: String,
    pub iss: Option<String>,
    pub session_state: Option<String>,
}

/// Form body data posted to the provider's token endpoint.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct OauthTokenRequest<'a> {
    pub grant_type: &'static str,
    pub code: String,
    pub redirect_uri: &'a str,
}

/// JSON body data returned by the provider's token endpoint.
#[serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct OauthTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    #[serde_as(
        as = "serde_with::StringWithSeparator::<serde_with::formats::SpaceSeparator, String>"
    )]
    pub scope: Vec<String>,
    pub id_token: Option<String>,
    pub token_type: String,
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    pub expires_in: Duration,
}

#[derive(Debug)]
pub struct OauthClient {
    pub client_id: String,
    pub client_secret: SecretString,
    pub provider_authorize_url: String,
    pub provider_token_url: String,
    pub callback_url: String,
}
impl OauthClient {
    pub fn make_login_link(&self) -> Url {
        Url::parse_with_params(
            &self.provider_authorize_url,
            [
                ("response_type", "code"),
                ("scope", "identity"),
                ("redirect_uri", &self.callback_url),
                ("client_id", &self.client_id),
                ("duration", "temporary"),
                ("state", "asdf"),
            ],
        )
        .unwrap()
    }

    pub async fn handle_callback(
        &self,
        req: Request,
        ctx: RouteContext<()>,
    ) -> Result<OauthTokenResponse> {
        let callback_data: OauthCallbackQueryResponse = req.query()?;
        let response = get_reqwest_client(&ctx.env)?
            .post(&self.provider_token_url)
            .basic_auth(&self.client_id, Some(self.client_secret.expose_secret()))
            .form(&OauthTokenRequest {
                grant_type: "authorization_code",
                code: callback_data.code,
                redirect_uri: &self.callback_url,
            })
            .send()
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
