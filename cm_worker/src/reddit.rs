//! Reddit API access.
use riven::reqwest::Client;
use serde_with::serde_as;

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
pub async fn get_me(client: &Client, access_token: &str) -> riven::reqwest::Result<Me> {
    let reddit_me: Me = client
        .get("https://oauth.reddit.com/api/v1/me")
        .bearer_auth(access_token)
        .send()
        .await
        .and_then(|r| r.error_for_status())?
        .json()
        .await?;
    Ok(reddit_me)
}
