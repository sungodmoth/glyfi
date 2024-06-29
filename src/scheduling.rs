use poise::serenity_prelude::{ButtonStyle, Context, CreateAttachment, CreateButton, CreateEmbed, CreateMessage, GuildId, MessageId};
use tokio::time;
use chrono::Utc;

use crate::{err, file::{delete_submission, generate_challenge_image, initialise_submissions_directory}, info, server_data::{format_ambi_announcement_spiel, format_glyph_announcement_spiel, format_poll_spiel, EMPTY_MESSAGE, SERVER_ID, STATUS_UPDATE_CHANNEL_ID, TIME_GAP, VOTING_EMOJI_SEQUENCE}, sql::{delete_prompt, deregister_submission, end_week, get_current_week_num, get_prompt_data, get_submissions, get_week_info, initialise_week, rollover_week}, types::{Timestamp, NULL_TIMESTAMP}, Res};
use crate::types::{Challenge, ChallengeImageOptions::*};

pub async fn schedule_loop(ctx: &Context) -> Res {
    for challenge in [Challenge::Glyph, Challenge::Ambigram].into_iter() {
        info!("Checking status of {} challenge...", challenge.short_name());
        let mut current_week_num = get_current_week_num(challenge).await?;
        let current_week_info = get_week_info(current_week_num, challenge).await?;
        let actual_end_time = current_week_info.actual_end_time;
        let current_time = Utc::now();
        if let Timestamp(Some(t)) = actual_end_time {
            Some(current_time > t).filter(|_| true).ok_or("Unexpected state: end time of current week set in the future")?;
            //we've already ended the challenge but haven't started the next one
            if let Ok(next_week_data) = get_week_info(current_week_num + 1, challenge).await {
                //next week has already been initialised; now we're just waiting for it to begin
                if current_time > next_week_data.target_start_time.0.unwrap() {
                    info!("Rolling over week for challenge {}. New prompt: {:?}", challenge.short_name(), next_week_data.prompt_string);

                    let next_prompt_string = next_week_data.prompt_string;
                    let target_start_time = next_week_data.target_start_time;
                    let target_end_time = next_week_data.target_end_time;
                    let target_timestamp = target_end_time.0.unwrap().timestamp();
                    let full_discord_timestamp = format!("<t:{}:F>", target_timestamp);
                    let relative_discord_timestamp = format!("<t:{}:R>", target_timestamp);
            
                    // get all the files
                    // it's pretty important that we do this before posting anything, since otherwise we could
                    // fail halfway through and end up only posting one file, and then we would end up posting
                    // that file over and over again as the database is never updated
                    let announcement_attachment = CreateAttachment::path(
                        generate_challenge_image(challenge, current_week_num + 1, 
                            Announcement { prompt_string: next_prompt_string.clone(),
                            size_percentage: next_week_data.size_percentage }, 
                            target_start_time, target_end_time, false
                        ).await?
                    ).await?;
        
                    let poll_attachment = CreateAttachment::path(
                        generate_challenge_image(challenge, current_week_num, Poll { prompt_string: current_week_info.prompt_string, 
                            size_percentage: current_week_info.size_percentage },
                            current_week_info.target_start_time, current_week_info.target_end_time, false
                        ).await?
                    ).await?;
        
                    // post everything
                    challenge.announcement_channel().send_message(&ctx, CreateMessage::new()
                        .content( match challenge {
                            Challenge::Glyph => format_glyph_announcement_spiel(current_week_num + 1, &next_prompt_string, 
                                &full_discord_timestamp, &relative_discord_timestamp),
                            Challenge::Ambigram => format_ambi_announcement_spiel(current_week_num + 1, &next_prompt_string, 
                                &full_discord_timestamp, &relative_discord_timestamp),
                        })
                        .add_file(announcement_attachment)
                    ).await?;
        
                    let mut poll_message_builder = CreateMessage::new()
                        .content(format_poll_spiel(&full_discord_timestamp, &relative_discord_timestamp))
                        .add_file(poll_attachment);
        
                    let mut first_numsubs = get_submissions(challenge, current_week_num).await?.len();
                    let mut second_numsubs = 0;
                    let mut second_poll_message_id: Option<MessageId> = None;
        
                    if first_numsubs > 25 {
                        // we are just going to assume there are not >50 subs so both of these are at most 25
                        second_numsubs = first_numsubs - 25;
                        first_numsubs = 25;
                    }
        
                    info!("There are {} + {} submissions for challenge {}.", first_numsubs, second_numsubs, challenge.short_name());
        
                    let prefix = format!("{}{:04}", challenge.one_char_name(), current_week_num);
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
                    rollover_week(challenge, current_week_num, Utc::now().into(), (first_numsubs + second_numsubs) as i64,
                     poll_message.id, second_poll_message_id).await?;
                    
                    info!("Removing prompt from the database...");
                    delete_prompt(challenge, 1).await?;
        
                    info!("Initialising file system for upcoming week...");
                    initialise_submissions_directory(challenge, current_week_num + 1).await?;
                    
                    info!("Done rolling over week!");
                }
            } else if let Ok(next_prompt) = get_prompt_data(challenge, 1).await {
                //we have a prompt to initialise next week
                let next_target_start_time = current_week_info.target_end_time + TIME_GAP;;
                let next_target_end_time = next_target_start_time + challenge.default_duration() 
                    * next_prompt.custom_duration.unwrap_or(1) as i32 - TIME_GAP;
                let week_num = current_week_num + 1;
                info!("Initialising next week for challenge {}");
                initialise_week(challenge, week_num, &next_prompt, next_target_start_time, next_target_end_time).await?;
            } else {
                //we need a prompt but don't have one
                info!("No prompt to initialise next {} challenge.", challenge.short_name());
            }
        } else if current_time > current_week_info.target_end_time.0.unwrap() {
            info!("Ending the current week for challenge {}", challenge.short_name());
            end_week(challenge, current_week_num, Utc::now().into()).await?;
            remove_absent_user_submissions(ctx, challenge, current_week_num, SERVER_ID).await?;
        } else {
            info!("No action needed for challenge {}", challenge.short_name());
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