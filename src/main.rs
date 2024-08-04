use std::{fmt, sync::Arc};

use axum::Router;
use db::LunchPollRepository;
use models::{LunchPoll, ToVoter};
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use shuttle_persist::PersistInstance;
use shuttle_runtime::SecretStore;
use sqlx::ConnectOptions;
use teloxide::{
    dispatching::{DefaultKey, UpdateHandler},
    payloads::SendPoll,
    prelude::*,
    requests::JsonRequest,
    types::MessageId,
    utils::command::BotCommands,
    RequestError,
};

mod db;
mod models;

const YES_ANSWER_ID: i32 = 0;

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

#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "Підтримуються наступні команди:")]
enum Command {
    #[command(description = "Показати цей хелп")]
    Help,
    #[command(description = "Проголосувати за обід")]
    Lunch,
    #[command(description = "Завершити голосування і вибрати переможців :)")]
    Go,
    #[command(description = "Скасувати поточне голосування")]
    Cancel,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
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

async fn command_handler(bot_service: BotService, bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    use Command::*;

    let cmd_result = match cmd {
        Help => help_cmd(&bot, &msg).await,
        Lunch => lunch_cmd(&bot, &msg, &bot_service).await,
        Go => go_cmd(&bot, &msg, &bot_service).await,
        Cancel => cancel_cmd(&bot, &msg, &bot_service).await,
    };

    if let Err(err) = cmd_result {
        handle_endpoint_err(&bot, msg.chat.id, &err).await;
    }

    Ok(())
}

async fn help_cmd(bot: &Bot, msg: &Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;

    Ok(())
}

async fn lunch_cmd(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
    // TODO: new behavior idea: auto stop existing poll with notice message to the chat.
    // maybe even delete the previous incomplete poll message from chat?
    if bot_service.incomplete_poll_exists(msg.chat.id).await? {
        bot.send_message(msg.chat.id, "Будь ласка, завершіть поточне голосування.")
            .await?;
        return Ok(());
    }

    let send_poll_payload = SendPoll::new(msg.chat.id, "Обід?", ["Так".into(), "Ні".into()]).is_anonymous(false);
    let request = JsonRequest::new(bot.clone(), send_poll_payload);
    let poll_msg = request.await?;
    let poll_id = poll_msg
        .poll()
        .expect("Unable to get Poll from the poll Message")
        .id
        .as_str();
    bot_service.create_poll(poll_id, poll_msg.id, msg.chat.id).await?;

    Ok(())
}

async fn go_cmd(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
    let Some(mut poll) = bot_service.get_poll_by_chat_id(msg.chat.id).await? else {
        bot.send_message(
            msg.chat.id,
            format!("Створіть нове опитування, використовуючи команду /{}.", Command::Lunch),
        )
        .await?;

        return Ok(());
    };

    // TODO: extract this into a function (see the `BotExt` trait)
    if let Err(error) = bot.stop_poll(msg.chat.id, MessageId(poll.poll_msg_id)).await {
        match error {
            // we swallow the Telegram API error for a case when there is a stored poll but no Telegram poll
            RequestError::Api(_) => {}
            _ => return Err(anyhow::Error::new(error)),
        }
    }

    let voters = &mut *poll.yes_voters;
    if voters.is_empty() {
        bot.send_message(msg.chat.id, "Ніхто не хоче обідати.").await?;
        bot_service.delete_poll(poll.id).await?;

        return Ok(());
    }

    // scope to drop the ThreadRng before it crosses the `await` boundary
    {
        let mut rng = thread_rng();
        voters.shuffle(&mut rng);
    }

    let voters_str = voters
        .iter()
        .enumerate()
        .map(|(i, voter)| format!("{}.\t{}", i + 1, voter.display_name))
        .collect::<Vec<_>>()
        .join("\n");
    bot.send_message(msg.chat.id, format!("Щасливці у порядку пріоритету:\n{voters_str}"))
        .await?;

    bot_service.delete_poll(poll.id).await?; // remove the poll from the storage only after all work is finished

    Ok(())
}

async fn cancel_cmd(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
    if let Some(poll) = bot_service.get_poll_by_chat_id(msg.chat.id).await? {
        let _ = bot.stop_poll(msg.chat.id, MessageId(poll.poll_msg_id)).await; // ignore stop poll error
        bot_service.delete_poll(poll.id).await?;
        bot.send_message(msg.chat.id, "Охрана, отмєна.").await?;
    } else {
        bot.send_message(
            msg.chat.id,
            format!("Створіть нове опитування, використовуючи команду /{}.", Command::Lunch),
        )
        .await?;
    }

    Ok(())
}

async fn poll_answer_handler(bot_service: BotService, _bot: Bot, answer: PollAnswer) -> ResponseResult<()> {
    if let Err(err) = process_poll_answer(bot_service, answer).await {
        log_endpoint_err(&err);
    }

    Ok(())
}

async fn process_poll_answer(bot_service: BotService, answer: PollAnswer) -> anyhow::Result<()> {
    let Some(mut poll) = bot_service.get_poll_by_poll_id(&answer.poll_id).await? else {
        log::warn!("answer for unknown poll ID: {:?}", answer);
        return Ok(());
    };

    if answer.poll_id == poll.poll_id && answer.option_ids.as_slice() == [YES_ANSWER_ID] {
        log::info!("matching poll answer received: {:?}", answer);

        // it's simpler (from the DB interaction perspective) to store voters as a Vec than as a Set
        let new_voter = answer.user.to_voter();
        if !poll.yes_voters.as_ref().contains(&new_voter) {
            poll.yes_voters.push(new_voter)
        }
        bot_service.save_poll(&poll).await?;
    }

    Ok(())
}

async fn handle_endpoint_err(bot: &Bot, chat_id: ChatId, err: &anyhow::Error) {
    let _ = bot.send_message(chat_id, "Помилка обробки запиту.").await;
    log_endpoint_err(err);
}

fn log_endpoint_err(err: &anyhow::Error) {
    log::error!("{err}, {}", err.backtrace())
}
