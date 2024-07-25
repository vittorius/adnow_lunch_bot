use sqlx::{prelude::Type, query, query_as, Database, Encode, PgPool, Pool};
// use sqlx_postgres::Postgres;
use teloxide::types::ChatId;

use crate::models::LunchPoll;

// TODO: re-implement using <'a> lifetime and a reference to PgPool
#[derive(Clone)]
pub struct LunchPollRepository {
    pool: PgPool,
}

impl LunchPollRepository {
    pub fn new(pool: PgPool) -> LunchPollRepository {
        LunchPollRepository { pool }
    }

    pub async fn get_poll_by_chat_id(&self, chat_id: ChatId) -> anyhow::Result<Option<LunchPoll>> {
        Ok(
            query_as::<_, LunchPoll>("SELECT * FROM lunch_polls WHERE chat_id = $1 LIMIT 1")
                .bind(chat_id.0)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
}
