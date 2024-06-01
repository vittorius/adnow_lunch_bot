use std::backtrace::{self, Backtrace};
use std::collections::{BTreeSet, HashSet};

use anyhow::bail;
use rand::seq::IteratorRandom;
use rand::seq::SliceRandom;
use rand::thread_rng;
use shuttle_persist::PersistInstance;
use shuttle_runtime::SecretStore;
use teloxide::{
    dispatching::UpdateHandler,
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

#[shuttle_runtime::main]
async fn shuttle_main(
    #[shuttle_runtime::Secrets] secret_store: SecretStore,
    #[shuttle_persist::Persist] persist: PersistInstance,
) -> Result<BotService, shuttle_runtime::Error> {
    let token = secret_store.get("TELOXIDE_TOKEN").unwrap();

    Ok(BotService { token, persist })
}

type VoterSet = HashSet<User>;

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
                _ => panic!("error clearing previous poll data: {err}"),
            }
        }

        let voters = match self.persist.load::<VoterSet>(LUNCH_POLL_YES_VOTERS_KEY) {
            Ok(mut voters) => {
                voters.clear();
                voters
            }
            _ => HashSet::new(),
        };
        if let Err(err) = self.persist.save(LUNCH_POLL_YES_VOTERS_KEY, voters) {
            panic!("error initializing empty \"yes\" voters vec: {err}")
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
    dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(command_handler),
        )
        .branch(Update::filter_poll_answer().endpoint(poll_answer_handler))
}

async fn command_handler(bot_service: BotService, bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    let cmd_result = match cmd {
        Command::Help => help(&bot, &msg).await,
        Command::Vote => vote(&bot, &msg, &bot_service).await,
        Command::Random => random(&bot, &msg, &bot_service).await,
    };

    if let Err(err) = cmd_result {
        let _ = bot.send_message(msg.chat.id, "Помилка виконання.").await;
        log::error!("{err}, {}", err.backtrace())
    }

    Ok(())
}

async fn help(bot: &Bot, msg: &Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;

    Ok(())
}

async fn vote(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
    // TODO: add Ukrainian to cSpell in editor
    let send_poll_payload = SendPoll::new(msg.chat.id, "Обід?", ["Так".into(), "Ні".into()]).is_anonymous(false);
    let request = JsonRequest::new(bot.clone(), send_poll_payload);
    let msg = request.await?;
    // log::info!("Poll msg id: {}", msg.id);

    // TODO: when migrated to RDBMS, wrap these 2 operations in a transaction
    bot_service.persist.save(LUNCH_POLL_MSG_ID_KEY, msg.id)?;
    bot_service.persist.save(
        LUNCH_POLL_ID_KEY,
        msg.poll().expect("Unable to get Poll from poll Message").id.as_str(),
    )?;

    Ok(())
}

struct TestVoter {
    name: String,
}

impl TestVoter {
    fn mention(&self) -> Option<String> {
        None
    }

    fn full_name(&self) -> String {
        self.name.clone()
    }
}

async fn random(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
    // FIXME: print error message to the chat and exit if no "yes"-voted participants
    let request: JsonRequest<_>;

    if let Ok(poll_msg_id) = bot_service.persist.load::<MessageId>(LUNCH_POLL_MSG_ID_KEY) {
        bot_service.persist.remove("lunch_poll_msg_id")?; // comes first because it's more reliable than stop_poll
        bot.stop_poll(msg.chat.id, poll_msg_id).await?;

        // let Ok(voters) = bot_service.persist.load::<VoterSet>(LUNCH_POLL_YES_VOTERS_KEY) else {
        //     bot.send_message(msg.chat.id, "Помилка завантаження даних.").await?;
        //     bail!(r#"unable to load "yes" votes by {} key: {:?}"#, LUNCH_POLL_YES_VOTERS_KEY);
        // };

        let voters = bot_service.persist.load::<VoterSet>(LUNCH_POLL_YES_VOTERS_KEY)?;

        // TODO: remove after covered with unit tests
        // let mut voters = [
        //     TestVoter {
        //         name: "Victor Zagorodny".into(),
        //     },
        //     TestVoter {
        //         name: "Андрей Дмитриев".into(),
        //     },
        //     TestVoter {
        //         name: "Кирилл".into()
        //     },
        //     TestVoter {
        //         name: "adnow Office Lunch Bot (dev)".into(),
        //     },
        // ];

        let mut rng = thread_rng();
        // let voters_str = voters
        //     .iter()
        //     .choose_multiple(&mut rng, voters.iter().count())
        //     .iter()
        //     .map(|user| user.mention().unwrap_or(user.full_name()))
        //     .collect::<Vec<_>>()
        //     .join("\n");
        let mut voters = Vec::from_iter(voters);
        voters.shuffle(&mut rng);
        let voters_str = voters
            .iter()
            .map(|user| user.mention().unwrap_or(user.full_name()))
            .collect::<Vec<_>>()
            .join("\n");
        request = bot.send_message(msg.chat.id, format!("Щасливці у порядку приорітету:\n{voters_str}"));
    } else {
        request = bot.send_message(msg.chat.id, "Створіть нове опитування, використовуючи команду /vote.");
    }

    request.send().await?;
    Ok(())
}

async fn poll_answer_handler(bot_service: BotService, _bot: Bot, answer: PollAnswer) -> ResponseResult<()> {
    if let Ok(poll_id) = bot_service.persist.load::<String>(LUNCH_POLL_ID_KEY) {
        if answer.poll_id == poll_id && answer.option_ids.as_slice() == [YES_ANSWER_ID] {
            log::info!("Matching poll answer received: {:?}", answer);

            let mut voters = bot_service.persist.load::<VoterSet>(LUNCH_POLL_YES_VOTERS_KEY)?;
            log::info!("before insert: {voters:?}");
            voters.insert(answer.user);
            log::info!("after insert: {voters:?}");
            if let Err(err) = bot_service.persist.save(LUNCH_POLL_YES_VOTERS_KEY, voters) {
                log::error!("{err}, {}", Backtrace::force_capture());
            }
        }
    }
    Ok(())
}
