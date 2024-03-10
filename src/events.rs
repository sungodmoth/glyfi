use poise::serenity_prelude::*;
use crate::{err, info, info_sync, Res, sql};
use crate::core::{file_mtime, InteractionID, report_user_error};
use crate::server_data::{AMBIGRAM_SUBMISSION_CHANNEL_ID, GLYPH_SUBMISSION_CHANNEL_ID, SUBMIT_EMOJI_ID};
use crate::sql::Challenge;

pub struct GlyfiEvents;

/// Reply to an interaction.
macro_rules! reply_ephemeral {
    ($ctx:expr, $i:expr, $fmt:literal $(,$arg:expr)*) => {
        $i.create_response(&$ctx, CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(format!($fmt $(,$arg)*))
                .ephemeral(true)
        )).await
    };
}

/// Execute code and notify the user if execution fails.
macro_rules! run {
    ($ctx:expr, $user:expr, $code:expr, $msg:expr) => {
        if let Err(e) = $code {
            err!("{}: {}", $msg, e);
            report_user_error(
                $ctx,
                $user,
                &format!("Sorry, an internal error occurred: {}: {}", $msg, e)
            ).await;
            return;
        }
    }
}

/// Mark that the announcement image for a challenge has been acknowledged.
async fn act_on_confirm_announcement(ctx: &Context, i: &mut ComponentInteraction) -> Res {
    let mut it = i.data.custom_id.split(':').skip(1);
    let challenge = it.next().ok_or("Invalid interaction ID")?.parse::<Challenge>()?;
    let time = it.next().ok_or("Invalid interaction ID")?.parse::<u64>()?;
    let id = it.next().ok_or("Invalid interaction ID")?.parse::<i64>()?;

    // Check that the file is not out of date.
    let path = challenge.announcement_image_path();
    let mtime = file_mtime(&path)?;
    if time != mtime {
        info!("Refusing to accept outdated announcement image for {:?}. Please regenerate it.", challenge);
        return Ok(());
    }

    // Save prompt.
    reply_ephemeral!(ctx, i, "Confirmed.")?;
    Ok(())
}

async fn act_on_cancel_prompt(ctx: &Context, i: &mut ComponentInteraction) -> Res {
    let mut it = i.data.custom_id.split(':').skip(1);
    let id = it.next().ok_or("Invalid interaction ID")?.parse::<i64>()?;

    let changed = sql::delete_prompt(id).await?;
    reply_ephemeral!(ctx, i, "{}", if changed { "Cancelled." } else { "Entry has already been cancelled." })?;
    Ok(())
}

/// Get the confirm emoji.
fn confirm_reaction() -> ReactionType { return ReactionType::Unicode("✅".into()); }

/// Check if we care about a reaction event.
async fn match_relevant_reaction_event(ctx: &Context, r: &Reaction) -> Option<(
    UserId,
    Message,
    Challenge,
)> {
    // Ignore anything that isn’t the emoji we care about.
    if !matches!(r.emoji, ReactionType::Custom {id: SUBMIT_EMOJI_ID, .. }) { return None; };

    // Make sure we have all the information we need.
    let Some(user) = r.user_id else { return None; };
    let Ok(message) = r.message(&ctx).await else { return None; };

    // Ignore this outside of the submission channels.
    let challenge = match message.channel_id {
        GLYPH_SUBMISSION_CHANNEL_ID => Challenge::Glyph,
        AMBIGRAM_SUBMISSION_CHANNEL_ID => Challenge::Ambigram,
        _ => return None
    };

    return Some((user, message, challenge));
}

#[async_trait]
impl EventHandler for GlyfiEvents {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            // Ignore commands here.
            Interaction::Command(_) => {}

            // Buttons and other components.
            Interaction::Component(mut i) => {
                info!("Processing interaction: {}", i.data.custom_id);
                let id: InteractionID = match i.data.custom_id.parse() {
                    Ok(id) => id,
                    Err(e) => {
                        err!("{}", e);
                        let _ = i.create_response(ctx, CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(format!("Error: Unknown ID '{}'", i.data.custom_id))
                                .ephemeral(true)
                        )).await;
                        return;
                    }
                };

                let res = match id {
                    InteractionID::ConfirmAnnouncement => act_on_confirm_announcement(&ctx, &mut i).await,
                    InteractionID::CancelPrompt => act_on_cancel_prompt(&ctx, &mut i).await,
                };

                if let Err(e) = res {
                    err!("Error processing interaction: {}", e);
                    let _ = i.create_response(ctx, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(format!("Error processing interaction: {}", e))
                            .ephemeral(true)
                    )).await;
                }
            }

            _ => {
                info!("Unsupported interaction {:?}", interaction);
            }
        }
    }

    /// Check whether a user added the submit emoji.
    async fn reaction_add(&self, ctx: Context, r: Reaction) {
        let Some((user, message, challenge)) =
            match_relevant_reaction_event(&ctx, &r).await else { return; };

        // Helper to remove the reaction on error and return.
        macro_rules! remove_reaction {
            ($ctx:expr, $r:expr) => {
                if let Err(e) = $r.delete(&$ctx).await { err!("Error removing reaction: {}", e); }
                return;
            };
        }

        // If someone reacted w/ this emoji to someone else’s message, remove it.
        if user != message.author.id { remove_reaction!(ctx, r); }

        // Check the message for attachments.
        if message.attachments.len() != 1 {
            report_user_error(&ctx, user, "Submissions must contain exactly one image").await;
            remove_reaction!(ctx, r);
        }

        // Safe because we just checked that that is an attachment.
        let att = message.attachments.first().unwrap();

        // Error if the attachment is not an image.
        //
        // There doesn’t really seem to be a way of checking what an attachment
        // actually is (excepting checking the mime type, which I’m not willing
        // to do), so checking whether the height exists, which it only should
        // for images, will have to do.
        if att.height.is_none() {
            report_user_error(&ctx, user, "Submissions must contain only images").await;
            remove_reaction!(ctx, r);
        }

        // Add the submission.
        run!(
            ctx, user,
            sql::add_submission(message.id, challenge, user, &att.url).await,
            "Error adding submission"
        );

        // Done.
        info!("Added submission {} from {} for challenge {:?}", message.id, user, challenge);
        if let Err(e) = message.react(ctx, confirm_reaction()).await {
            err!("Error reacting to submission: {}", e);
        }
    }

    async fn reaction_remove(&self, ctx: Context, r: Reaction) {
        // Check if we care about this.
        let Some((user, message, challenge)) =
            match_relevant_reaction_event(&ctx, &r).await else { return; };

        // If the reaction that was removed is not the reaction of the
        // user that sent the message (which I guess can happen if there
        // is ever some amount of downtime on our part?) then ignore it.
        if user != message.author.id { return; };

        // Remove the submission.
        run!(
            ctx, user,
            sql::remove_submission(message.id, challenge).await,
            "Error removing submission"
        );

        // Done.
        info!("Removed submission {} from {} for challenge {:?}", message.id, user, challenge);

        // Remove our confirmation reaction. This is allowed to fail in case
        // it was already removed somehow.
        let me = ctx.cache.current_user().id;
        let _ = message.delete_reaction(ctx, Some(me), confirm_reaction()).await;
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        info_sync!("Glyfi running with id {}", ready.user.id);
    }
}
