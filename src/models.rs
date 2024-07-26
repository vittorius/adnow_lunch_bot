use serde::Deserialize;

use sqlx::{types::Json, Encode, FromRow, Type};

use teloxide::types::{User};

#[derive(Type, Deserialize)]
pub struct Voter {
    user_id: i64,
    pub display_name: String,
}

impl PartialEq for Voter {
    fn eq(&self, other: &Self) -> bool {
        self.user_id == other.user_id
    }
}

impl Eq for Voter {}

pub trait ToVoter {
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
pub struct LunchPoll {
    pub id: i64, // TODO: primary key
    #[sqlx(rename = "tg_poll_id")]
    pub poll_id: String,
    #[sqlx(rename = "tg_poll_msg_id")]
    pub poll_msg_id: i32,
    // chat_id: ChatId,
    pub yes_voters: Json<Vec<Voter>>, // TODO: #[sqlx(json)]
}
