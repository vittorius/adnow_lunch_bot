use std::{collections::HashSet, fmt};

use axum::Router;
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use shuttle_persist::PersistInstance;
use shuttle_runtime::SecretStore;
use teloxide::{
    dispatching::{DefaultKey, UpdateHandler},
    payloads::SendPoll,
    prelude::*,
    requests::JsonRequest,
    types::{MessageId, User},
    utils::command::BotCommands,
    RequestError,
};

const LUNCH_POLL_MSG_ID_KEY: &str = "lunch_poll_msg_id";
const LUNCH_POLL_ID_KEY: &str = "lunch_poll_id";
const LUNCH_POLL_YES_VOTERS_KEY: &str = "lunch_poll_yes_voters";
const YES_ANSWER_ID: i32 = 0;

type VoterSet = HashSet<User>;

// Customize this struct with things from `shuttle_main` needed in `bind`,
// such as secrets or database connections
#[derive(Clone)]
struct BotService {
    token: String,
    persist: PersistInstance,
}

impl BotService {
    // HACK: bincode won't serialize teloxide::types::User because of its macro annotations;
    // we're using a hack here: first, serialize into a JSON String, then persist it into Shuttle (which uses bincode)
    fn persist_save<T: Serialize>(&self, key: &str, value: T) -> anyhow::Result<()> {
        Ok(self.persist.save(key, serde_json::to_string(&value)?)?)
    }

    // HACK: bincode won't serialize teloxide::types::User because of its macro annotations;
    // we're using a hack here: first, serialize into a JSON String, then persist it into Shuttle (which uses bincode)
    fn persist_load<T>(&self, key: &str) -> anyhow::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        Ok(serde_json::from_str::<T>(self.persist.load::<String>(key)?.as_str())?)
    }

    fn persist_remove(&self, key: &str) -> anyhow::Result<(), shuttle_persist::PersistError> {
        self.persist.remove(key)
    }

    fn init_data_on_startup(&self) {
        if let Err(err) = self.persist_remove(LUNCH_POLL_MSG_ID_KEY) {
            match err {
                shuttle_persist::PersistError::RemoveFile(_) => {
                    log::info!("no previous persisted poll was found on bot start, nothing to clear")
                }
                _ => panic!("error clearing previous poll data: {err}"),
            }
        }

        if let Err(err) = self.persist_save(LUNCH_POLL_YES_VOTERS_KEY, VoterSet::new()) {
            panic!("error initializing empty \"yes\" voters set: {err}")
        }
    }
}
#[shuttle_runtime::main]
/// Using dummy Axum web app to make the bot run continuously. This web app doesn't handle any requests.
async fn axum(
    #[shuttle_runtime::Secrets] secret_store: SecretStore,
    #[shuttle_persist::Persist] persist: PersistInstance,
) -> shuttle_axum::ShuttleAxum {
    let token = secret_store.get("TELOXIDE_TOKEN").unwrap();

    let router = build_router(BotService { token, persist });

    Ok(router.into())
}

fn build_router(bot_service: BotService) -> Router {
    log::info!("starting bot");

    bot_service.init_data_on_startup();

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
        // If the dispatcher fails for some reason, execute this handler.
        .error_handler(LoggingErrorHandler::with_custom_text(
            "error in the teloxide dispatcher",
        ))
        .enable_ctrlc_handler()
        .build()
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
        Help => help(&bot, &msg).await,
        Lunch => lunch(&bot, &msg, &bot_service).await,
        Go => go(&bot, &msg, &bot_service).await,
        Cancel => cancel(&bot, &msg, &bot_service).await,
    };

    if let Err(err) = cmd_result {
        handle_endpoint_err(&bot, msg.chat.id, &err).await;
    }

    Ok(())
}

async fn help(bot: &Bot, msg: &Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;

    Ok(())
}

async fn lunch(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
    if bot_service.persist_load::<MessageId>(LUNCH_POLL_MSG_ID_KEY).is_ok() {
        bot.send_message(msg.chat.id, "Будь ласка, завершіть поточне голосування.").await?;
        return Ok(())
    }

    let send_poll_payload = SendPoll::new(msg.chat.id, "Обід?", ["Так".into(), "Ні".into()]).is_anonymous(false);
    let request = JsonRequest::new(bot.clone(), send_poll_payload);
    let msg = request.await?;

    // TODO: when migrated to Postgres, wrap these 2 operations in a transaction
    bot_service.persist_save(LUNCH_POLL_MSG_ID_KEY, msg.id)?;
    bot_service.persist_save(
        LUNCH_POLL_ID_KEY,
        msg.poll().expect("Unable to get Poll from poll Message").id.as_str(),
    )?;

    Ok(())
}

async fn go(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
    let request: JsonRequest<_>;

    if let Ok(poll_msg_id) = bot_service.persist_load::<MessageId>(LUNCH_POLL_MSG_ID_KEY) {
        bot_service.persist_remove(LUNCH_POLL_MSG_ID_KEY)?; // comes first because it's more reliable than stop_poll
        bot.stop_poll(msg.chat.id, poll_msg_id).await?;

        let voters = bot_service.persist_load::<VoterSet>(LUNCH_POLL_YES_VOTERS_KEY)?;
        if voters.is_empty() {
            bot.send_message(msg.chat.id, "Ніхто не хоче обідати.").await?;
            return Ok(());
        }

        bot_service.persist_save(LUNCH_POLL_YES_VOTERS_KEY, VoterSet::new())?; // cleanup

        let mut voters = Vec::from_iter(voters);

        let mut rng = thread_rng();
        voters.shuffle(&mut rng);

        let voters_str = voters
            .iter()
            .enumerate()
            .map(|(i, user)| format!("{}.\t{}", i + 1, user.mention().unwrap_or(user.full_name())))
            .collect::<Vec<_>>()
            .join("\n");
        request = bot.send_message(msg.chat.id, format!("Щасливці у порядку пріоритету:\n{voters_str}"));
    } else {
        request = bot.send_message(
            msg.chat.id,
            format!("Створіть нове опитування, використовуючи команду /{}.", Command::Lunch),
        );
    }

    request.send().await?;
    Ok(())
}

async fn cancel(bot: &Bot, msg: &Message, bot_service: &BotService) -> Result<(), anyhow::Error> {
    if let Ok(poll_msg_id) = bot_service.persist_load::<MessageId>(LUNCH_POLL_MSG_ID_KEY) {
        bot_service.persist_remove(LUNCH_POLL_MSG_ID_KEY)?; // comes first because it's more reliable than stop_poll
        bot.stop_poll(msg.chat.id, poll_msg_id).await?;
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
    if let Err(err) = poll_answer_handler_impl(bot_service, answer) {
        log_endpoint_err(&err);
    }

    Ok(())
}

fn poll_answer_handler_impl(bot_service: BotService, answer: PollAnswer) -> anyhow::Result<()> {
    let poll_id = bot_service.persist_load::<String>(LUNCH_POLL_ID_KEY)?;
    if answer.poll_id == poll_id && answer.option_ids.as_slice() == [YES_ANSWER_ID] {
        log::info!("matching poll answer received: {:?}", answer);

        let mut voters = bot_service.persist_load::<VoterSet>(LUNCH_POLL_YES_VOTERS_KEY)?;
        voters.insert(answer.user);
        bot_service.persist_save(LUNCH_POLL_YES_VOTERS_KEY, voters)?;
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
