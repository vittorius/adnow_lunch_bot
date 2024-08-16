use teloxide::{prelude::Requester, types::ChatId, Bot};

pub(crate) async fn handle_endpoint_err(bot: &Bot, chat_id: ChatId, err: &anyhow::Error) {
    let _ = bot.send_message(chat_id, "Помилка обробки запиту.").await;
    log_endpoint_err(err);
}

pub(crate) fn log_endpoint_err(err: &anyhow::Error) {
    log::error!("{err}, {}", err.backtrace())
}
