use teloxide::types::PollAnswer;

use crate::{models::ToVoter, BotService};

const YES_ANSWER_ID: i32 = 0;

pub(crate) async fn process_poll_answer(bot_service: BotService, answer: PollAnswer) -> anyhow::Result<()> {
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
