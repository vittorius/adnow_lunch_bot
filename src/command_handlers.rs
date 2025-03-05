use crate::{message_handlers::Command, BotService};
use rand::{seq::SliceRandom, thread_rng};
use teloxide::{
    payloads::{SendPoll, SendPollSetters},
    prelude::Requester,
    requests::JsonRequest,
    types::{Message, MessageId},
    utils::command::BotCommands,
    Bot, RequestError,
};

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
    // if let Err(error) = bot.stop_poll(msg.chat.id, MessageId(poll.poll_msg_id)).await {
    //     match error {
    //         // we swallow the Telegram API error for a case when there is a stored poll but no Telegram poll
    //         RequestError::Api(_) => {}
    //         _ => return Err(anyhow::Error::new(error)), // TODO: such a wrapping is probably wrong, fix it
    //     }
    // }
    bot.stop_poll(msg.chat.id, MessageId(poll.poll_msg_id)).await?;

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
    use crate::message_handlers::Command;
    use crate::{build_update_handler, BotService};
    use delegate::delegate;
    use sqlx::PgPool;
    use teloxide::dispatching::{UpdateFilterExt, UpdateHandler};
    use teloxide::{dptree, Bot};
    use teloxide::prelude::Requester;
    use teloxide::types::{ChatId, MessageId, Poll, Update};
    use teloxide::utils::command::BotCommands;
    use teloxide_tests::{IntoUpdate, MockBot, MockMessagePoll, MockMessageText, Responses};

    struct Environment {
        bot: MockBot,
        bot_service: BotService,
        chat_id: Option<ChatId>,
    }

    impl Environment {
        fn new<TUpdate: IntoUpdate>(db_pool: PgPool, update: TUpdate) -> Self {
            let bot_service = BotService::new("1234567890".to_string(), db_pool);
            let bot = MockBot::new(update, Self::build_test_update_handler());
            bot.dependencies(dptree::deps![bot_service.clone()]);

            let chat_id = bot.updates.lock().unwrap().last().unwrap().chat().map(|chat| chat.id);
            Self {
                bot,
                bot_service,
                chat_id,
            }
        }

        delegate! {
            to self.bot {
                #[call(dispatch)]
                pub async fn bot_dispatch(&self);

                #[call(get_responses)]
                pub fn bot_responses(&self) -> Responses;

                #[call(update)]
                pub fn update<T: IntoUpdate>(&self, update: T);
            }
        }

        async fn create_poll_in_db(&self, poll_id: Option<&str>, poll_msg_id: Option<MessageId>) {
            self.bot_service
                .create_poll(
                    poll_id.unwrap_or(MockMessagePoll::POLL_ID),
                    poll_msg_id.unwrap_or(MessageId(MockMessageText::new().id.0)),
                    self.chat_id
                        .expect("Previous update did not have a chat associated with it"),
                )
                .await
                .expect("Failed to create a fixture poll");
        }

        // async fn create_and_start_poll(&self) {
        //     self.bot
        //         .bot
        //         .send_poll(
        //             self.chat_id
        //                 .expect("Previous update did not have a chat associated with it"),
        //             "Dummy poll",
        //             vec!["Yes".into(), "No".into()],
        //         )
        //         .await
        //         .expect("Failed to send a fixture poll");
        //     self.create_poll().await;
        // }
        fn build_test_update_handler() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
            build_update_handler().branch(
                Update::filter_poll().endpoint(|_bot_service: BotService, _bot: Bot, _poll: Poll| async { Ok(()) }),
            )
        }
    }

    #[sqlx::test]
    async fn help_cmd_sends_expected_message(db_pool: PgPool) {
        let message = MockMessageText::new().text("/help");
        let env = Environment::new(db_pool, message);
        env.bot_dispatch().await;

        let responses = env.bot_responses();
        assert_eq!(responses.sent_messages.len(), 1);
        let message = responses.sent_messages.last().expect("No sent messages were detected!");
        assert_eq!(message.text(), Some(Command::descriptions().to_string().as_str()));
    }

    #[sqlx::test]
    async fn on_incomplete_poll_lunch_cmd_sends_notice_and_exits(db_pool: PgPool) {
        let message = MockMessageText::new().text("/lunch");
        let env = Environment::new(db_pool, message);
        env.create_poll_in_db(None, None).await;
        env.bot_dispatch().await;

        let responses = env.bot_responses();
        assert_eq!(responses.sent_messages.len(), 1);
        let message = responses.sent_messages.last().expect("No sent messages were detected!");
        assert_eq!(message.text(), Some("Будь ласка, завершіть поточне голосування."));
    }

    #[sqlx::test]
    async fn on_successful_poll_send_lunch_cmd_stores_poll(db_pool: PgPool) {
        let message = MockMessageText::new().text("/lunch");
        let env = Environment::new(db_pool, message);
        env.bot_dispatch().await;

        let poll = env
            .bot_service
            .get_poll_by_chat_id(env.chat_id.unwrap())
            .await
            .expect("Failed to get poll");
        assert!(poll.is_some());
    }

    #[sqlx::test]
    async fn on_failed_poll_send_lunch_cmd_stores_poll(db_pool: PgPool) {
        let message = MockMessageText::new().text("/lunch");
        let env = Environment::new(db_pool, message);
        env.bot_dispatch().await;

        let poll = env
            .bot_service
            .get_poll_by_chat_id(env.chat_id.unwrap())
            .await
            .expect("Failed to get poll");
        assert!(poll.is_some());
    }

    #[sqlx::test]
    async fn on_non_existent_poll_go_cmd_sends_notice_and_exits(db_pool: PgPool) {
        let message = MockMessageText::new().text("/go");
        let env = Environment::new(db_pool, message);
        env.bot_dispatch().await;

        let responses = env.bot_responses();
        assert_eq!(responses.sent_messages.len(), 1);
        let message = responses.sent_messages.last().expect("No sent messages were detected!");
        assert_eq!(
            message.text(),
            Some("Створіть нове опитування, використовуючи команду /lunch.")
        );
    }

    #[sqlx::test]
    async fn on_existing_poll_go_cmd_stops_the_poll(db_pool: PgPool) {
        // FIXME: send dummy empty initial message, then send poll using the bot
        let poll = MockMessagePoll::new();
        let env = Environment::new(db_pool, poll);
        env.bot_dispatch().await;

        assert_eq!(env.bot_responses().sent_messages_poll.len(), 1);

        // let message = MockMessageText::new().text("/go");
        // env.update(message);
        // env.bot_dispatch().await;
    }

    async fn on_existing_poll_go_cmd_ignores_telegram_error(db_pool: PgPool) {
        // let message = MockMessageText::new().text("/go");
        // let env = Environment::new(db_pool, message);
        // env.create_poll().await;
        // env.bot_dispatch().await;

        // let responses = env.bot_responses();
        // assert_eq!(responses.sent_messages.len(), 1);
        // let message = responses.sent_messages.last().expect("No sent messages were detected!");
        // assert_eq!(message.text(), Some("Будь ласка, завершіть поточне голосування."));
    }

    async fn on_nobody_voted_go_cmd_sends_notice_and_deletes_poll(db_pool: PgPool) {}

    async fn on_voted_go_cmd_sends_the_list_of_voters_and_deletes_poll(db_pool: PgPool) {}
}
