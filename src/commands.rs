use chrono::{DateTime, Duration, Utc};
use poise::builtins::register_application_commands;
use poise::{ChoiceParameter, CreateReply};
use poise::serenity_prelude::{CreateAttachment, CreateEmbed, CreateEmbedAuthor};
use tokio::time;
use crate::{info, sql, Context, Res, ResT};
use crate::core::{create_embed, file_mtime, handle_command_error};
use crate::sql::{add_prompt, edit_prompt, forecast_prompt_details, get_current_week_num, get_prompt_data, get_prompt_id, get_prompt_id_data, get_week_info, swap_prompts};
use crate::types::{Challenge, ChallengeImageOptions::*, PreviewableImages, PromptData, UploadableImages};
use crate::file::generate_challenge_image;

/// Edit your nickname.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error")]
pub async fn nickname(
    ctx: Context<'_>,
    name: String,
) -> Res {
    // Name must not be empty, must not include only whitespace
    // and must not be longer than 200 characters.
    let name = name.trim();
    if name.is_empty() || name.len() > 200 {
        return Err("Name must not be empty and contain at most 200 characters".into());
    }

    // Set nickname.
    sql::set_nickname(ctx.author().id, name).await?;
    ctx.say(format!("Set your nickname to ‘{}’", name)).await?;
    Ok(())
}

/// Display your user profile.
//
// Shows the specified user profile or the user that executes it. Shows
// the user’s UserID, nickname, amount of glyphs submitted, amount of
// ambigrams submitted, the highest ranking in Glyph Challenge, the
// highest ranking in ambigram challenge, & amount of 1st, 2nd, and
// 3rd place placements.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error")]
pub async fn profile(ctx: Context<'_>) -> Res {
    const ZWSP: &str = "\u{200B}";

    let data = sql::get_user_profile(ctx.author().id).await?;
    let name: &str = data.nickname.as_ref()
        .or(ctx.author().global_name.as_ref())
        .unwrap_or(&ctx.author().name)
        .as_str();

    let mut embed = create_embed(&ctx);
    embed = embed.author(CreateEmbedAuthor::new(format!("{}’s Profile", name))
        .icon_url(ctx.author().face())
    );

    // Helper to add a field.
    fn add(embed: CreateEmbed, name: &'static str, value: i64) -> CreateEmbed {
        embed.field(
            name,
            format!(
                "{} time{}",
                value,
                if value == 1 { "" } else { "s" }
            ),
            true,
        )
    }

    let have_glyphs_rating = data.glyphs_first != 0 ||
        data.glyphs_second != 0 ||
        data.glyphs_third != 0;

    let have_ambigrams_rating = data.ambigrams_first != 0 ||
        data.ambigrams_second != 0 ||
        data.ambigrams_third != 0;

    // Add submissions.
    if data.glyphs_submissions != 0 || data.ambigrams_submissions != 0 {
        embed = embed.field("Submitted Glyphs", format!("{}", data.glyphs_submissions), true);
        embed = embed.field("Submitted Ambigrams", format!("{}", data.ambigrams_submissions), true);
        embed = embed.field(ZWSP, ZWSP, true); // Empty field.
    }

    // Add first/second/third place ratings for glyphs challenge.
    if have_glyphs_rating {
        embed = add(embed, "1st Place – G", data.glyphs_first);
        embed = add(embed, "2nd Place – G", data.glyphs_second);
        embed = add(embed, "3nd Place – G", data.glyphs_third);
    } else {
        embed = embed.field(
            "Highest ranking in Glyphs Challenge",
            format!("{}", data.highest_ranking_glyphs),
            false,
        );
    }

    // Add first/second/third place for ambigrams challenge.
    if have_ambigrams_rating {
        embed = add(embed, "1st Place – A", data.ambigrams_first);
        embed = add(embed, "2nd Place – A", data.ambigrams_second);
        embed = add(embed, "3nd Place – A", data.ambigrams_third);
    } else {
        embed = embed.field(
            "Highest ranking in Ambigrams Challenge",
            format!("{}", data.highest_ranking_ambigrams),
            false,
        );
    }

    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error",
 subcommands("queue_add", "queue_list", "queue_remove", "queue_preview", "queue_edit", "queue_swap", "queue_move"), 
 default_member_permissions = "ADMINISTRATOR")]
pub async fn queue(_ctx: Context<'_>) -> Res { unreachable!(); }

/// Add a new prompt to the given queue.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "add", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_add(
    ctx: Context<'_>,
    #[description = "Which challenge to set the prompt for"] challenge: Challenge,
    #[description = "The prompt for the challenge"] prompt_string: String,
    #[description = "Percentage modifying the size of the prompt - defaults to 100 (normal size)"] size_percentage: Option<u16>,
    #[description = "Duration of the challenge measured in weeks - defaults to 1"] custom_duration: Option<u16>,
    #[description = "Whether the week is special - defaults to false"] is_special: Option<bool>,
    #[description = "Any extra text to accompany the announcement of this glyph"] extra_announcement_text: Option<String>
) -> Res {
    if let Some(0) = size_percentage { return Err("Cannot set size_percentage to 0.".into()); }
    if let Some(0) = custom_duration { return Err("Cannot set custom_duration to 0.".into()); }
    let prompt_data = PromptData { challenge, prompt_string, size_percentage: size_percentage.filter(|x| x != &100), 
        custom_duration, is_special: is_special.filter(|x| x == &true), extra_announcement_text };

    // Save prompt.
    add_prompt(&prompt_data).await?;

    // The next operation reads the same database we just updated. Unfortunately it seems like we have to wait 
    // just a bit in order to make sure that we get the correctly updated data.
    let mut timer = tokio::time::interval(time::Duration::from_secs(1));
    timer.tick().await;
    timer.tick().await;

    let (week_num, start_time, end_time) = forecast_prompt_details(challenge, -1).await?;
    
    // Generate image based on new prompt.
    ctx.defer_ephemeral().await?;
    let path = generate_challenge_image(challenge, week_num, Announcement { prompt_string: prompt_data.prompt_string, size_percentage: prompt_data.size_percentage.unwrap_or(100) },
        start_time, end_time, false).await?;

    // Get mtime. This is just a little sanity check.
    file_mtime(&path)?;

    // Reply with the image.
    ctx.send(CreateReply::default()
        .content("Successfully added entry!")
        .attachment(CreateAttachment::path(path).await?)
    ).await?;
    Ok(())
}

/// Edit an existing entry of a given queue.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "edit", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_edit(
    ctx: Context<'_>,
    #[description = "Which challenge to edit a prompt for"] challenge: Challenge,
    #[description = "Position in the queue of the prompt to edit"] position: usize,
    #[description = "New size modifier of the prompt"] size_percentage: Option<u16>,
    #[description = "New duration of the challenge in weeks"] custom_duration: Option<u16>,
    #[description = "Whether or not the week should be special"] is_special: Option<bool>,
    #[description = "Any extra text to accompany the announcement of this glyph"] extra_announcement_text: Option<String>
) -> Res {
    let (id, mut prompt_data) = get_prompt_id_data(challenge, position).await?;
    // whether or not this operation necessitates showing the user the new image because it has changed
    let mut changed = false;
    if let Some(v) = size_percentage { if v == 0 { return Err("Cannot set size_percentage to 0.".into()) } else {
        prompt_data.size_percentage = size_percentage.filter(|x| x != &100); changed = true; } }
    if let Some(v) = custom_duration { if v == 0 { return Err("Cannot set custom_duration to 0.".into()) } else {
        prompt_data.custom_duration = custom_duration; changed = true; } }
    if let Some(_) = is_special { prompt_data.is_special = is_special.filter(|x| x == &true); }
    if let Some(_) = &extra_announcement_text { prompt_data.extra_announcement_text = extra_announcement_text; }

    info!("Modifying prompt {}:{} to {:?} in db...", challenge.name(), position, prompt_data);
    let successful = edit_prompt(id, &prompt_data).await?;

    if !successful {
        ctx.say("Database operation failed while modifying prompt.").await?;
        return Ok(())
    }

    if changed {

        // The next operation reads the same database we just updated. Unfortunately it seems like we have to wait 
        // just a bit in order to make sure that we get the correctly updated data.
        let mut timer = tokio::time::interval(time::Duration::from_secs(1));
        timer.tick().await;
        timer.tick().await;
        let (week_num, start_time, end_time) = forecast_prompt_details(challenge, position as i64).await?;
        
        // Generate image based on modified prompt.
        ctx.defer_ephemeral().await?;
        let path = generate_challenge_image(challenge, week_num, Announcement { prompt_string: prompt_data.prompt_string, 
            size_percentage: prompt_data.size_percentage.unwrap_or(100) },
        start_time, end_time, false).await?;

        // Get mtime. This is just a little sanity check.
        file_mtime(&path)?;

        // Reply with the image.
        ctx.send(CreateReply::default()
            .content("Successfully modified entry!")
            .attachment(CreateAttachment::path(path).await?)
        ).await?;
    }
    else {
        ctx.send(CreateReply::default()
            .content("Successfully modified entry!")).await?;
    }
    Ok(())
}

/// Swap two existing entries of a given queue.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "swap", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_swap(
    ctx: Context<'_>,
    #[description = "Which challenge to swap two prompts for"] challenge: Challenge,
    #[description = "First position in the queue to swap"] position1: usize,
    #[description = "Second position in the queue to swap"] position2: usize,
) -> Res {

    if position1 == position2 {
        ctx.say("Trying to swap an entry with itself.").await?;
        return Ok(());
    }

    info!("Swapping prompts {}:{} and {}:{} in db...", challenge.name(), position1, challenge.name(), position2);
    let successful = swap_prompts(challenge, position1, position2).await?;

    if !successful { ctx.say("Something went wrong in the database while swapping.").await?; }
    else { ctx.say("Successfully swapped prompts!").await?; }
    Ok(())
}

/// Move an entry of a queue into a specified position.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "move", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_move(
    ctx: Context<'_>,
    #[description = "Which challenge to move a prompt for"] challenge: Challenge,
    #[description = "Position of prompt to move"] from: usize,
    #[description = "Position to move into"] to: usize,
) -> Res {
    info!("Moving prompt {}:{} into {}:{} in db...", challenge.name(), from, challenge.name(), to);
    let mut successful = true; 

    match from.cmp(&to) {
    std::cmp::Ordering::Equal => { ctx.say("Trying to move prompt into the same position it's already in.").await?; return Ok(());},
        std::cmp::Ordering::Greater => { for n in (to+1)..=from {
            successful &= swap_prompts(challenge, to, n).await?;
        }},
        std::cmp::Ordering::Less => { for n in ((from+1)..=to).rev() {
            successful &= swap_prompts(challenge, from, n).await?;
        }}, 
    }
    
    if !successful { ctx.say("Database operation failed while moving prompt.").await?; }
    else { ctx.say("Successfully moved prompt!").await?; }
    Ok(())
}

/// Show the current queue for a challenge.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "list", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_list(
    ctx: Context<'_>,
    #[description = "Which challenge to show the queue for"] challenge: Challenge,
) -> Res {
    // Get the queue.
    let queue = sql::get_prompts(challenge).await?;

    // Create embed.
    let mut embed = create_embed(&ctx)
        .author(CreateEmbedAuthor::new(format!("Queue for {} Challenge", challenge.name())))
        .description("Listed properties: size_percentage, custom_duration, is_special, extra_announcement_text.\nIf a property has its default value, it is not listed.");
    for (idx, prompt) in queue.into_iter().enumerate() {
        embed = embed.field(format!("**{}**: {}", idx + 1, prompt.prompt_string),[
            prompt.size_percentage.map(|x| format!("> size_percentage: {x}%")),
            prompt.custom_duration.map(|x| format!("> custom_duration: {x} weeks")),
            prompt.is_special.map(|x| format!("> is_special: {x}")),
            prompt.extra_announcement_text.map(|x| format!("> extra_announcement_text: {x}"))
        ].into_iter().flatten().collect::<Vec<String>>().join("\n"), false);
    }

    // Send it.
    ctx.send(CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Remove an entry from a queue.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "remove", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_remove(
    ctx: Context<'_>,
    #[description = "The challenge to remove an entry from"] challenge: Challenge,
    #[description = "The entry number in the queue to remove"] position: usize,
) -> Res {
    // Remove it.
    let changed = sql::delete_prompt(challenge, position).await?;
    let name = challenge.name();
    // Send a reply.
    if changed { ctx.say(format!("Removed entry {position} from queue {name}.")).await?; } //
    else { ctx.say("No such entry").await?; }
    Ok(())
}

/// Preview an entry in the queue.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "preview", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_preview(
    ctx: Context<'_>,
    #[description = "The challenge to preview an entry from"] challenge: Challenge,
    #[description = "The entry number in the queue to preview"] position: usize,
) -> Res {
    let (week_num, start_time, end_time) = forecast_prompt_details(challenge, position as i64).await?;

    ctx.defer_ephemeral().await?;
    let prompt_data = sql::get_prompt_data(challenge, position).await?;
    let path = generate_challenge_image(challenge, week_num, Announcement { prompt_string: prompt_data.prompt_string, 
        size_percentage: prompt_data.size_percentage.unwrap_or(100) }, start_time, end_time, false).await?;

    ctx.send(CreateReply::default()
        .attachment(CreateAttachment::path(path).await?)
    ).await?;
    Ok(())
}

/// Update bot commands.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", default_member_permissions = "ADMINISTRATOR")]
pub async fn update(ctx: Context<'_>) -> Res {
    register_application_commands(ctx, false).await?;
    Ok(())
}

#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error",
 subcommands("image_preview", "image_upload"), 
 default_member_permissions = "ADMINISTRATOR")]
pub async fn image(_ctx: Context<'_>) -> Res { unreachable!(); }

#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename="preview", default_member_permissions = "ADMINISTRATOR")]
pub async fn image_preview(ctx: Context<'_>, 
    #[description="The challenge to preview an image for"] challenge: Challenge,
    #[description="The image to preview"] image_type: PreviewableImages,
    #[description="Whether or not to return the raw pdf file instead of the rendered png. Defaults to false"] raw: Option<bool>) -> Res {
        
    ctx.defer_ephemeral().await?;
    let path = match image_type {
        PreviewableImages::Announcement => { 
            let next_prompt_data = get_prompt_data(challenge, 1).await?;
            let (week_num, start_time, end_time) = forecast_prompt_details(challenge, 1).await?;
            generate_challenge_image(challenge, week_num, 
                Announcement { prompt_string: next_prompt_data.prompt_string , size_percentage: next_prompt_data.size_percentage.unwrap_or(100) }, 
                start_time, end_time, raw.unwrap_or(false)).await? },
        PreviewableImages::Poll => {
            let week_num = get_current_week_num(challenge).await?;
            let week_info = get_week_info(week_num, challenge).await?;
            generate_challenge_image(challenge, week_num, Poll { prompt_string: week_info.prompt_string, 
                size_percentage: week_info.size_percentage }, week_info.target_start_time, week_info.target_end_time, 
                raw.unwrap_or(false)).await? },
        PreviewableImages::FirstPlace => { unimplemented!() },
        PreviewableImages::SecondPlace => { unimplemented!() },
        PreviewableImages::ThirdPlace => {unimplemented!() },
    };

    ctx.send(CreateReply::default()
        .attachment(CreateAttachment::path(path).await?)
    ).await?;

    Ok(())
}

#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "upload", default_member_permissions = "ADMINISTRATOR")]
pub async fn image_upload(ctx: Context<'_>, 
    #[description="The challenge to upload an image for"] challenge: Challenge,
    #[description="The image type to upload"] image_type: UploadableImages) -> Res {
    
    todo!()
}

///// Show stats for a week.
//
// Info shown are: That week’s glyph/ambigram, message link to
// that week’s announcement post, How many submissions there were
// in that week, how many people voted for that week’s submissions,
// message link to that week’s submissions post, top 3 winner names,
// message link to that week’s hall of fame, & the announcement image
// used for that week.
// #[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error")]
// pub async fn week_info(
//     ctx: Context<'_>,
//     #[description = "Which challenge to get stats for"] challenge: Challenge,
//     #[description = "The week whose stats to retrieve"] week: Option<u64>,
// ) -> Res {
//     let info = sql::weekinfo(week).await?;
//     let mut embed = create_embed(&ctx);
//     embed = embed.author(CreateEmbedAuthor::new(format!("Stats for Week {}", info.week)));
//     embed = embed.field("Submissions", format!("{}", info.submissions), true);
//     todo!();


//     Ok(())
// }