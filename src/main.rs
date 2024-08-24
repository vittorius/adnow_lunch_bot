use axum::Router;
use db::LunchPollRepository;
use message_handlers::{command_handler, poll_answer_handler, Command};
use models::LunchPoll;

use shuttle_runtime::SecretStore;

use teloxide::{
    dispatching::{DefaultKey, UpdateHandler},
    prelude::*,
    types::MessageId,
    RequestError,
};

mod command_handlers;
mod db;
mod error_handling;
mod message_handlers;
mod models;
mod poll_handlers;
#[cfg(test)]
mod testing;

// TODO: finish this extension tract
trait BotExt {
    fn stop_poll_ignoring_api_error(&self);
}

// Customize this struct with things from `shuttle_main` needed in `bind`,
// such as secrets or database connections
#[derive(Clone)]
struct BotService {
    token: String,
    repo: LunchPollRepository,
}

impl BotService {
    async fn incomplete_poll_exists(&self, chat_id: ChatId) -> anyhow::Result<bool> {
        self.repo
            .get_poll_by_chat_id(chat_id)
            .await
            .map(|poll_opt| poll_opt.is_some())
    }

    async fn create_poll(&self, poll_id: &str, poll_msg_id: MessageId, chat_id: ChatId) -> anyhow::Result<()> {
        self.repo.create_poll(poll_id, poll_msg_id, chat_id).await
    }

    async fn get_poll_by_chat_id(&self, chat_id: ChatId) -> anyhow::Result<Option<LunchPoll>> {
        self.repo.get_poll_by_chat_id(chat_id).await
    }

    async fn get_poll_by_poll_id(&self, poll_id: &str) -> anyhow::Result<Option<LunchPoll>> {
        self.repo.get_poll_by_poll_id(poll_id).await
    }

    async fn save_poll(&self, poll: &LunchPoll) -> anyhow::Result<()> {
        self.repo.update_poll_voters(poll).await
    }

    async fn delete_poll(&self, id: i64) -> anyhow::Result<()> {
        self.repo.delete_poll(id).await
    }
}

#[shuttle_runtime::main]
/// Using dummy Axum web app to make the bot run continuously. This web app doesn't handle any requests.
async fn axum(
    #[shuttle_runtime::Secrets] secret_store: SecretStore,
    #[shuttle_shared_db::Postgres] db_pool: sqlx::PgPool,
) -> shuttle_axum::ShuttleAxum {
    let token = secret_store.get("TELOXIDE_TOKEN").unwrap();

    let router = build_router(BotService {
        token,
        repo: LunchPollRepository::new(db_pool),
    });

    log::info!("Starting bot...");

    Ok(router.into())
}

fn build_router(bot_service: BotService) -> Router {
    let bot = Bot::new(&bot_service.token);

    let mut dispatcher = build_bot_dispatcher(bot, bot_service);

    tokio::spawn(async move {
        dispatcher.dispatch().await;
    });

    Router::new()
}

fn build_bot_dispatcher(bot: Bot, bot_service: BotService) -> Dispatcher<Bot, RequestError, DefaultKey> {
    Dispatcher::builder(bot, build_update_handler())
        .dependencies(dptree::deps![bot_service])
        .default_handler(|upd| async move {
            log::warn!("unhandled update: {:?}", upd);
        })
        // if the dispatcher fails for some reason, execute this handler.
        .error_handler(LoggingErrorHandler::with_custom_text(
            "error in the teloxide dispatcher",
        ))
        .enable_ctrlc_handler()
        .build()
}

fn build_update_handler() -> UpdateHandler<RequestError> {
    // TODO: add initial filter if chat is a group
    dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(command_handler),
        )
        .branch(Update::filter_poll_answer().endpoint(poll_answer_handler))
}
