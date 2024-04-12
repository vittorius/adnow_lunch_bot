use shuttle_persist::PersistInstance;
use shuttle_runtime::SecretStore;
use teloxide::{
    dispatching::UpdateHandler,
    prelude::*,
    types::MessageId,
    utils::command::BotCommands,
    RequestError,
};

const LUNCH_POLL_MSG_ID_KEY: &str = "lunch_poll_msg_id";

#[shuttle_runtime::main]
async fn shuttle_main(
    #[shuttle_runtime::Secrets] secret_store: SecretStore,
    #[shuttle_persist::Persist] persist: PersistInstance,
) -> Result<BotService, shuttle_runtime::Error> {
    let token = secret_store.get("TELOXIDE_TOKEN").unwrap();

    Ok(BotService { token, persist })
}

// Customize this struct with things from `shuttle_main` needed in `bind`,
// such as secrets or database connections
#[derive(Clone)]
struct BotService {
    token: String,
    persist: PersistInstance,
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Підтримуються наступні команди:")]
enum Command {
    #[command(description = "Показати цей хелп")]
    Help,
    #[command(description = "Проголосувати за обід")]
    Vote,
    #[command(description = "Кинути кубік :)")]
    Random,
}

#[shuttle_runtime::async_trait]
impl shuttle_runtime::Service for BotService {
    async fn bind(self, _addr: std::net::SocketAddr) -> Result<(), shuttle_runtime::Error> {
        // Start your service and bind to the socket address
        log::info!("Starting bot...");

        if let Err(err) = self.persist.remove(LUNCH_POLL_MSG_ID_KEY) {
            match err {
                shuttle_persist::PersistError::RemoveFile(_) => {
                    log::info!("No previous persisted poll was found on bot start, nothing to clear")
                }
                _ => panic!("{err}"),
            }
        }

        let bot = Bot::new(&self.token);

        Dispatcher::builder(bot, build_update_handler())
            .dependencies(dptree::deps![self])
            // If no handler succeeded to handle an update, this closure will be called.
            .default_handler(|upd| async move {
                log::warn!("Unhandled update: {:?}", upd);
                // TODO: move `bot` here using Rc
                // if let Some(chat) = upd.chat() {
                //     bot.send_message(chat.id, "Невідома команда .");
                // }
            })
            // If the dispatcher fails for some reason, execute this handler.
            .error_handler(LoggingErrorHandler::with_custom_text(
                "An error has occurred in the dispatcher",
            ))
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        Ok(())
    }
}

fn build_update_handler() -> UpdateHandler<RequestError> {
    // TODO: add initial filter if chat is a group
    Update::filter_message()
        .branch(dptree::entry().filter_command::<Command>().endpoint(command_handler))
        .branch(Update::filter_poll_answer().endpoint(poll_answer_handler))
}

async fn command_handler(mut bot_service: BotService, bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    let cmd_result = match cmd {
        Command::Help => help(&bot, &msg).await,
        Command::Vote => vote(&bot, &msg, &mut bot_service).await,
        Command::Random => random(&bot, &msg, &mut bot_service).await,
    };

    if let Err(err) = cmd_result {
        log::error!("{}", err)
    }

    Ok(())
}

async fn help(bot: &Bot, msg: &Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;

    Ok(())
}

async fn vote(bot: &Bot, msg: &Message, bot_service: &mut BotService) -> anyhow::Result<()> {
    // TODO: add Ukrainian to cSpell in editor
    let msg = bot.send_poll(msg.chat.id, "Обід?", ["Так".into(), "Ні".into()]).await?;

    bot_service.persist.save::<MessageId>(LUNCH_POLL_MSG_ID_KEY, msg.id)?;

    Ok(())
}

async fn random(bot: &Bot, msg: &Message, bot_service: &mut BotService) -> anyhow::Result<()> {
    if let Ok(poll_msg_id) = bot_service.persist.load::<MessageId>("lunch_poll_msg_id") {
        bot_service.persist.remove("lunch_poll_msg_id")?; // comes first because it's more reliable than stop_poll
        bot.stop_poll(msg.chat.id, poll_msg_id).await?;
    } else {
        bot.send_message(msg.chat.id, "Створіть нове опитування, використовуючи команду /vote.")
            .await?;
    }

    Ok(())
}

async fn poll_answer_handler(bot_service: BotService, bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    Ok(())
}
