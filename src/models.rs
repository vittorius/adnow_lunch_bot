use serde::{Deserialize, Serialize};

use sqlx::{types::Json, FromRow, Type};

use teloxide::types::User;

#[derive(Type, Serialize, Deserialize)]
pub(crate) struct Voter {
    user_id: i64,
    pub display_name: String,
}

impl PartialEq for Voter {
    fn eq(&self, other: &Self) -> bool {
        self.user_id == other.user_id
    }
}

impl Eq for Voter {}

pub(crate) trait ToVoter {
    fn to_voter(&self) -> Voter;
}

impl ToVoter for User {
    fn to_voter(&self) -> Voter {
        Voter {
            user_id: i64::from_ne_bytes(self.id.0.to_ne_bytes()),
            display_name: self.mention().unwrap_or(self.full_name()),
        }
    }
}

#[derive(FromRow)]
pub(crate) struct LunchPoll {
    pub id: i64,
    #[sqlx(rename = "tg_poll_id")]
    pub poll_id: String,
    #[sqlx(rename = "tg_poll_msg_id")]
    pub poll_msg_id: i32,
    // TODO: #[sqlx(json)]
    pub yes_voters: Json<Vec<Voter>>,
}
