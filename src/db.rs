use std::time::SystemTime;

use anyhow::Ok;
use sqlx::{prelude::Type, query, query_as, types::chrono::Utc, Database, Encode, PgPool, Pool};
// use sqlx_postgres::Postgres;
use teloxide::types::{ChatId, MessageId};

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
            query_as::<_, LunchPoll>("SELECT * FROM lunch_polls WHERE tg_chat_id = $1 LIMIT 1")
                .bind(chat_id.0)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn create_poll(&self, poll_id: &str, poll_msg_id: MessageId, chat_id: ChatId) -> anyhow::Result<()> {
        let now = Utc::now();
        query("INSERT INTO lunch_polls (tg_chat_id, tg_poll_id, tg_poll_msg_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(chat_id.0)
            .bind(poll_id)
            .bind(poll_msg_id.0)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_poll(&self, id: i64) -> anyhow::Result<()> {
        query("DELETE FROM lunch_polls WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
