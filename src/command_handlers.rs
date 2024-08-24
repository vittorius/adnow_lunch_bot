use rand::{seq::SliceRandom, thread_rng};
use teloxide::{
    payloads::{SendPoll, SendPollSetters},
    prelude::Requester,
    requests::JsonRequest,
    types::{Message, MessageId},
    utils::command::BotCommands,
    Bot, RequestError,
};

use crate::{message_handlers::Command, BotService};

pub(crate) async fn help_cmd(bot: &Bot, msg: &Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;

    Ok(())
}

pub(crate) async fn lunch_cmd(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
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

pub(crate) async fn go_cmd(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
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

pub(crate) async fn cancel_cmd(bot: &Bot, msg: &Message, bot_service: &BotService) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use crate::build_update_handler;

    #[tokio::test]
    async fn test_help_sends_expected_message() {
        let message = MockMessageText::new().text("/help");
        let bot = MockBot::new(message, build_update_handler());
        // Sends the message as if it was from a user
        bot.dispatch().await;

        let responses = bot.get_responses();
        let message = responses.sent_messages.last().expect("No sent messages were detected!");
        assert_eq!(message.text(), Some(Command::descriptions().to_string()));
    }
}
