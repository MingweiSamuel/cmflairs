//! Model structs corresponding to database tables. Must be kept in sync with migrations.

use serde_with::serde_as;

/// A cmflairs user, associated with a specific Reddit account.
#[serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    id: u64,
    reddit_user_name: String,
    #[serde_as(as = "serde_with::BoolFromInt")]
    profile_is_public: bool,
    profile_bgskinid: u64,
}
