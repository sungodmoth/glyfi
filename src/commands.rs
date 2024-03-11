use chrono::naive;
use poise::builtins::register_application_commands;
use poise::{ChoiceParameter, CreateReply};
use poise::serenity_prelude::{ButtonStyle, CreateActionRow, CreateAttachment, CreateButton, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter};
use crate::{Context, Error, info, Res, sql};
use crate::core::{create_embed, DEFAULT_EMBED_COLOUR, file_mtime, handle_command_error};
use crate::sql::{edit_prompt, get_prompt, get_prompt_id, Challenge, PromptData, PromptOption};

async fn generate_challenge_image(prompt_data: &PromptData) -> Result<String, Error> {
    let name = match prompt_data.challenge {
        Challenge::Glyph => "glyph_announcement",
        Challenge::Ambigram => "ambigram_announcement",
    };

    // Command for generating the image.
    let mut command = tokio::process::Command::new("./generate.py");
    command.arg(name);
    command.arg(String::from(&prompt_data.prompt).replace("\\n", "\\\\"));
    if let Some(percentage) = prompt_data.size_percentage {
        command.arg("--size_percentage");
        command.arg(percentage.to_string());
    }
    command.kill_on_drop(true);
    command.current_dir("./generation");
    info!("Running shell command {:?}", command);

    // Run it.
    let res = command.spawn()?.wait().await?;
    if !res.success() { return Err("Failed to generate image".into()); }
    Ok(prompt_data.challenge.announcement_image_path())
}

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

#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", subcommands("queue_add", "queue_list", "queue_remove", "queue_preview", "queue_edit"), default_member_permissions = "ADMINISTRATOR")]
pub async fn queue(ctx: Context<'_>) -> Res { unreachable!(); }

/// Add a new prompt to the given queue.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "add", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_add(
    ctx: Context<'_>,
    #[description = "Which challenge to set the prompt for"] challenge: Challenge,
    #[description = "The prompt for the challenge"] prompt: String,
    #[description = "Percentage modifying the size of the prompt"] size_percentage: Option<i16>
) -> Res {
    let prompt_data = PromptData { challenge, prompt, size_percentage };

    // Save prompt.
    let id = sql::add_prompt(&prompt_data).await?;

    // Generate image based on new prompt.
    ctx.defer_ephemeral().await?;
    let path = generate_challenge_image(&prompt_data).await?;

    // Get mtime. This is just a little sanity check.
    let mtime = file_mtime(&path)?;

    // Reply with the image.
    ctx.send(CreateReply::default()
        .content("Successfully modified entry!")
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
    #[description = "Property to modify"] property: PromptOption,
    #[description = "Value to set property to"] value: String
) -> Res {
    let (id, mut prompt_data) = get_prompt(challenge, position).await?;
    match property {
        PromptOption::Prompt => {
            prompt_data.prompt = value;
        },
        PromptOption::SizePercentage => {
            let percentage = value.parse::<i16>().map_err(|_| "cannot set size_percentage to a non-integer value")?;
            prompt_data.size_percentage = Some(percentage);
        }
    }

    info!("Modifying prompt {}:{} to {:?} in db...", challenge.name(), position, prompt_data);
    edit_prompt(id, &prompt_data).await?;

    // Generate image based on modified prompt.
    ctx.defer_ephemeral().await?;
    let path = generate_challenge_image(&prompt_data).await?;

    // Get mtime. This is just a little sanity check.
    let mtime = file_mtime(&path)?;

    // Reply with the image.
    ctx.send(CreateReply::default()
        .content("Successfully added to queue!")
        .attachment(CreateAttachment::path(path).await?)
    ).await?;
    Ok(())
}

/// Show the current queue for a challenge.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error", rename = "list", default_member_permissions = "ADMINISTRATOR")]
pub async fn queue_list(
    ctx: Context<'_>,
    #[description = "Which challenge to show the queue for"] challenge: Challenge,
) -> Res {
    // Get the queue.
    let queue = sql::get_prompts(challenge)
        .await?
        .iter().enumerate()
        .map(|(i, t)| 
            if let Some(percentage) = t.1.size_percentage {
                format!("- **{}:** {}, {}%", i+1, t.1.prompt, percentage) }
            else {
                format!("- **{}:** {}", i+1, t.1.prompt) 
        } )
        .collect::<Vec<_>>()
        .join("\n");

    // Create embed.
    let embed = create_embed(&ctx)
        .author(CreateEmbedAuthor::new(format!("Queue for {} Challenge", challenge.name())))
        .description(format!("**n:** prompt, size_percentage\n{queue}"));

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
    ctx.defer_ephemeral().await?;
    let prompt_data = sql::get_prompt(challenge, position).await?;
    let path = generate_challenge_image(&prompt_data.1).await?;
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

/// Show stats for a week.
//
// Info shown are: That week’s glyph/ambigram, message link to
// that week’s announcement post, How many submissions there were
// in that week, how many people voted for that week’s submissions,
// message link to that week’s submissions post, top 3 winner names,
// message link to that week’s hall of fame, & the announcement image
// used for that week.
#[poise::command(slash_command, ephemeral, guild_only, on_error = "handle_command_error")]
pub async fn weekinfo(
    ctx: Context<'_>,
    #[description = "Which challenge to get stats for"] challenge: Challenge,
    #[description = "The week whose stats to retrieve"] week: Option<u64>,
) -> Res {
    /*let info = sql::weekinfo(week).await?;
    let mut embed = create_embed(&ctx);
    embed = embed.author(CreateEmbedAuthor::new(format!("Stats for Week {}", info.week)));
    embed = embed.field("Submissions", format!("{}", info.submissions), true);*/
    todo!();


    Ok(())
}