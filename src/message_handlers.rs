use std::fmt::{self, Display, Formatter};

use teloxide::{types::Message, Bot};

use teloxide::{
    prelude::*,
    utils::command::BotCommands,
};
use crate::command_handlers::{cancel_cmd, go_cmd, help_cmd, lunch_cmd};
use crate::error_handling::{handle_endpoint_err, log_endpoint_err};
use crate::poll_handlers::process_poll_answer;
use crate::{BotService};

#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "Підтримуються наступні команди:")]
pub(crate) enum Command {
    #[command(description = "Показати цей хелп")]
    Help,
    #[command(description = "Проголосувати за обід")]
    Lunch,
    #[command(description = "Завершити голосування і вибрати переможців :)")]
    Go,
    #[command(description = "Скасувати поточне голосування")]
    Cancel,
}

impl Display for Command {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
    }
}

pub(crate) async fn command_handler(
    bot_service: BotService,
    bot: Bot,
    msg: Message,
    cmd: Command,
) -> ResponseResult<()> {
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

pub(crate) async fn poll_answer_handler(bot_service: BotService, _bot: Bot, answer: PollAnswer) -> ResponseResult<()> {
    if let Err(err) = process_poll_answer(bot_service, answer).await {
        log_endpoint_err(&err);
    }

    Ok(())
}
