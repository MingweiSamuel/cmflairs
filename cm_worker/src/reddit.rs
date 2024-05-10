//! Reddit API access.
use serde_with::serde_as;
use worker::{Env, Result};

use crate::init::get_reqwest_client;

/// GET `/api/v1/me`
#[serde_as]
#[derive(Debug, serde::Deserialize)]
pub struct Me {
    /// base36 encoded numeric portion of the Reddit "fullname" ID.
    #[serde_as(as = "crate::with::Base36")]
    pub id: u64,
    /// Reddit username (no "/u/").
    pub name: String,
    /// If this is a new user that can edit their name.
    pub can_edit_name: bool,
    // Many other fields.
}

/// GET `/api/v1/me`.
pub async fn get_me(env: &Env, access_token: &str) -> Result<Me> {
    let reddit_me: Me = get_reqwest_client(env)?
        .get("https://oauth.reddit.com/api/v1/me")
        .bearer_auth(access_token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Failed to get Reddit identity info from API: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to get Reddit identity response body: {}", e))?;
    Ok(reddit_me)
}
