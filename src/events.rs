use poise::serenity_prelude::*;
use crate::file::download_pfp;
use crate::{err, file, info, info_sync, sql, Res};
use crate::core::{file_mtime, report_user_error};
use crate::server_data::{AMBIGRAM_SUBMISSION_CHANNEL_ID, DISCORD_BOT_TOKEN, GLYPH_SUBMISSION_CHANNEL_ID, SUBMIT_EMOJI_ID};
use crate::sql::{check_submission, check_user, current_week, register_user, Challenge};

pub struct GlyfiEvents;

/// Execute code and notify the user if execution fails.
macro_rules! run {
    ($ctx:expr, $user:expr, $code:expr, $msg:expr) => {
        if let Err(e) = $code {
            err!("{}: {}", $msg, e);
            report_user_error(
                $ctx,
                $user,
                &format!("Sorry, an internal error occurred: {}: {}\nPlease contact @sungodmoth to file a bug report.", $msg, e)
            ).await;
            return;
        }
    }
}


/// Get the confirm emoji.
fn confirm_reaction() -> ReactionType { return ReactionType::Unicode("✅".into()); }

/// Check if we care about a reaction event.
async fn match_relevant_reaction_event(ctx: &Context, r: &Reaction) -> Option<(
    User,
    Message,
    Challenge,
)> {
    // Ignore anything that isn’t the emoji we care about.
    if !matches!(r.emoji, ReactionType::Custom {id: SUBMIT_EMOJI_ID, .. }) { return None; };
    // Make sure we have all the information we need.
    let Ok(user) = r.user(&ctx).await else { return None; };
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
    
    /// Check whether a user added the submit emoji.
    async fn reaction_add(&self, ctx: Context, r: Reaction) {
        let Some((user, message, challenge)) =
            match_relevant_reaction_event(&ctx, &r).await else { return; };

        let Ok(current_week) = current_week().await else { err!("Could not retrieve current week."); return };

        // TODO: check that the submission was actually posted within the current week.

        // Helper to remove the reaction on error and return.
        macro_rules! remove_reaction {
            ($ctx:expr, $r:expr) => {
                if let Err(e) = $r.delete(&$ctx).await { err!("Error removing reaction: {}", e); }
                return;
            };
        }
        let user_id = user.id;
        let Some(member) = r.member.clone() else { err!("Could not retrieve member for reaction event."); return };
        // If someone reacted w/ this emoji to someone else’s message, remove it.
        if user_id != message.author.id { remove_reaction!(ctx, r); }

        // Check the message for attachments.
        if message.attachments.len() != 1 {
            report_user_error(&ctx, user_id, "Submissions must contain exactly one image").await;
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
            report_user_error(&ctx, user_id, "Submissions must contain only images").await;
            remove_reaction!(ctx, r);
        }
        
        info!("Adding submission {} from {} for challenge {:?}", message.id, user_id, challenge);

        run!(
            ctx, user_id,
            async {sql::register_submission(message.id, challenge, user_id, &att.url, current_week).await?;
                file::download_submission(att, message.id, challenge, current_week).await }.await,
            "Error adding submission"
        );

        match check_user(&member).await {
            Ok(false) => {
                if let Err(e) = download_pfp(&member).await {
                    err!("Error downloading user pfp: {}", e);
                }
                //the user isn't in the database
                if let Err(e) = register_user(member).await {
                    err!("Error adding user to database: {}", e);
                }
            }
            Err(e) => { err!("Error checking whether user is in database: {}", e) }
            _ => {}
        }

        // Done.
        if let Err(e) = message.react(ctx, confirm_reaction()).await {
            err!("Error reacting to submission: {}", e);
        }
    }

    async fn reaction_remove(&self, ctx: Context, r: Reaction) {
        // Check if we care about this.
        let Some((user, message, challenge)) =
            match_relevant_reaction_event(&ctx, &r).await else { return; };
        
        let Ok(current_week) = current_week().await else { err!("Could not retrieve current week."); return };

        // TODO: check that the submission was actually posted within the current week.


        let user_id = user.id;
        
        // If the reaction that was removed is not the reaction of the
        // user that sent the message (which I guess can happen if there
        // is ever some amount of downtime on our part?) then ignore it.
        if user_id != message.author.id { return; };
        
        // check if we had ever registered the submission before trying to remove it
        // this will not be the case if, for instance, we just removed the user's
        // reaction for being an invalid attachment type or in the wrong channel
        match check_submission(message.id).await {
            Ok(true) => {
                info!("Removing submission {} from {} for challenge {:?}", message.id, user_id, challenge);
                // Remove the submission.
                run!(
                    ctx, user_id,
                    async {sql::deregister_submission(message.id, challenge, current_week).await?;
                        file::delete_submission(message.id, challenge, current_week).await }.await,
                        "Error removing submission"
                    );
                },
            Err(e) => {err!("Error checking whether submission exists: {}", e); },
            _ => {},
        }        

        // Remove our confirmation reaction. This is allowed to fail in case
        // it was already removed somehow.
        let me = ctx.cache.current_user().id;
        let _ = message.delete_reaction(ctx, Some(me), confirm_reaction()).await;
    }
    
    async fn guild_member_update(&self, _ctx: Context, old_if_available: Option<Member>, new: Option<Member>, _g: GuildMemberUpdateEvent) {
        if let (Some(old_member), Some(new_member)) = (old_if_available, new) {
            match check_user(&new_member).await {
                Ok(true) => { if old_member.face() != new_member.face() {
                    if let Err(e) = download_pfp(&new_member).await {
                        err!("Error downloading pfp: {}", e);
                    }
                }},
                Err(e) => { err!("Error checking whether user already exists: {}", e); },
                _ => {}
            }
 
        }
    }


    async fn ready(&self, _ctx: Context, ready: Ready) {
        info_sync!("Glyfi running with id {}", ready.user.id);
    }
}
