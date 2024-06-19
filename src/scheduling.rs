use poise::serenity_prelude::{ButtonStyle, Context, CreateAttachment, CreateButton, CreateEmbed, CreateMessage, GuildId, MessageId};
use tokio::time;
use chrono::Utc;

use crate::{err, file::{delete_submission, generate_challenge_image, initialise_submissions_directory}, info, server_data::{format_ambi_announcement_spiel, format_glyph_announcement_spiel, format_poll_spiel, EMPTY_MESSAGE, SERVER_ID, STATUS_UPDATE_CHANNEL_ID, VOTING_EMOJI_SEQUENCE}, sql::{delete_prompt, deregister_submission, get_current_week, get_prompt_data, get_submissions, get_week_info, rollover_week}, Res};
use crate::types::{Challenge, ChallengeImageOptions::*};

pub async fn schedule_loop(ctx: &Context) -> Res {
    for challenge in [Challenge::Glyph, Challenge::Ambigram].into_iter() {
        info!("Checking status of {} challenge...", challenge.short_name());
        let current_week = get_current_week(challenge).await?;
        let target_end_time = get_week_info(current_week, challenge).await?.target_end_time;
        let current_time = Utc::now();
        if current_time > target_end_time.0 {
            if let Ok(next_prompt) = get_prompt_data(challenge, 1).await {
                //we're good to change over
                let next_prompt = next_prompt;
                let current_week_info = get_week_info(current_week, challenge).await?;
                info!("Rolling over week. New prompt: {:?}", next_prompt);
                // details for the incoming week
                let target_start_time = target_end_time;
                let target_end_time = target_start_time + challenge.default_duration() 
                    * next_prompt.custom_duration.unwrap_or(1) as i32;
                let target_timestamp = target_end_time.0.timestamp();
                let full_discord_timestamp = format!("<t:{}:F>", target_timestamp);
                let relative_discord_timestamp = format!("<t:{}:R>", target_timestamp);

                // really, the time that we ought to do this is whenever we lock voting
                remove_absent_user_submissions(ctx, challenge, current_week, SERVER_ID).await?;

                // get all the files
                // it's pretty important that we do this before posting anything, since otherwise we could
                // fail halfway through and end up only posting one file, and then we would end up posting
                // that file over and over again as the database is never updated
                let announcement_attachment = CreateAttachment::path(
                    generate_challenge_image(challenge, current_week + 1, 
                        Announcement { prompt: next_prompt.prompt.clone(),
                        size_percentage: next_prompt.size_percentage.unwrap_or(100) }, 
                        target_start_time, target_end_time, false
                    ).await?
                ).await?;

                let poll_attachment = CreateAttachment::path(
                    generate_challenge_image(challenge, current_week, Poll { prompt: current_week_info.prompt, 
                        size_percentage: current_week_info.size_percentage },
                        current_week_info.target_start_time, current_week_info.target_end_time, false
                    ).await?
                ).await?;

                // post everything
                challenge.announcement_channel().send_message(&ctx, CreateMessage::new()
                    .content( match challenge {
                        Challenge::Glyph => format_glyph_announcement_spiel(current_week + 1, &next_prompt.prompt, 
                            &full_discord_timestamp, &relative_discord_timestamp),
                        Challenge::Ambigram => format_ambi_announcement_spiel(current_week + 1, &next_prompt.prompt, 
                            &full_discord_timestamp, &relative_discord_timestamp),
                    })
                    .add_file(announcement_attachment)
                ).await?;

                let mut poll_message_builder = CreateMessage::new()
                    .content(format_poll_spiel(&full_discord_timestamp, &relative_discord_timestamp))
                    .add_file(poll_attachment);

                let mut first_numsubs = get_submissions(challenge, current_week).await?.len();
                let mut second_numsubs = 0;
                let mut second_poll_message_id: Option<MessageId> = None;

                if first_numsubs > 25 {
                    // we are just going to assume there are not >50 subs so both of these are at most 25
                    second_numsubs = first_numsubs - 25;
                    first_numsubs = 25;
                }

                info!("There are {} + {} submissions for challenge {}.", first_numsubs, second_numsubs, challenge.short_name());

                let prefix = format!("{}{:04}", challenge.one_char_name(), current_week);
                for (idx, emoji) in VOTING_EMOJI_SEQUENCE.iter().enumerate().take(first_numsubs) {
                    poll_message_builder = poll_message_builder
                        .button(CreateButton::new(format!("{}-{:03}", prefix, idx))
                        .emoji(*emoji).style(ButtonStyle::Primary));
                }
                let poll_message = challenge.announcement_channel().send_message(&ctx, poll_message_builder).await?;

                if second_numsubs > 0 {
                    let mut second_poll_message_builder = CreateMessage::new().content(EMPTY_MESSAGE);
                    for (idx, emoji) in VOTING_EMOJI_SEQUENCE.iter().enumerate().skip(first_numsubs).take(second_numsubs) {
                        second_poll_message_builder = second_poll_message_builder
                            .button(CreateButton::new(format!("{}-{:03}", prefix, idx))
                            .emoji(*emoji).style(ButtonStyle::Primary));
                    }
                    let second_poll_message = challenge.announcement_channel()
                        .send_message(&ctx, second_poll_message_builder).await?;
                    second_poll_message_id = Some(second_poll_message.id);
                }

                info!("Rolling over database...");
                rollover_week(challenge, current_week, &next_prompt, Utc::now().into(), target_start_time, 
                    target_end_time, (first_numsubs + second_numsubs) as i64, poll_message.id, second_poll_message_id).await?;
                
                info!("Removing prompt from the database...");
                delete_prompt(challenge, 1).await?;

                info!("Initialising file system for upcoming week...");
                initialise_submissions_directory(challenge, current_week + 1).await?;
                
                info!("Done rolling over week!");
            }
            else {
                info!("It's time to rollover {} challenge, but there's no prompt to use.", challenge.short_name());
            }
        }
    }
    Ok(())
}

/// Remove all of the submissions from users who are not in the guild anymore (banned/left).
pub async fn remove_absent_user_submissions(ctx: &Context, challenge: Challenge, week_num: i64, guild_id: GuildId) -> Res {
    for (user_id, message) in get_submissions(challenge, week_num).await?.into_iter() {
        if let Err(_) = guild_id.member(&ctx, user_id).await {
            info!("Deregistering submission {} because user {} is no longer present.", message, user_id);
            deregister_submission(message, challenge, week_num).await?;
            delete_submission(message, challenge, week_num).await?;
        }
    }
    Ok(())
}