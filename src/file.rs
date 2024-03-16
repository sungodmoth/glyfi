use poise::serenity_prelude::{Attachment, Member, MessageId};
use tokio::{
    fs::{self, remove_file, File},
    io::AsyncWriteExt,
};

use crate::{info, sql::Challenge, Res};

/// Download a submission's image file to the file system
pub async fn download_submission(
    attachment: &Attachment,
    message_id: MessageId,
    challenge: Challenge,
    week_num: i64,
) -> Res {
    let content = attachment.download().await?;
    let short_name = challenge.short_name();
    //we don't actually have to care about the file extension in the name since we're converting anyway
    // let extension = attachment.filename.split('.').last().ok_or("File doesn't have an extension.")?;
    let extension = "png";
    let dir = format!("generation/images/{short_name}/{week_num}");
    fs::create_dir(&dir).await.or_else(|err| {
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(err)
        }
    })?;
    let prefix = format!("{dir}/{message_id}");
    let location = format!("{}.{}", prefix, extension);
    info!("Saving submission file to {}", location);
    let mut file = File::create(&location).await?;
    file.write_all(&content).await?;
    info!("Converting {} to png...", location);
    convert_image_type(&prefix, extension, "png").await?;
    Ok(())
}

/// Remove a submission's image file from the file system
pub async fn delete_submission(message_id: MessageId, challenge: Challenge, week_num: i64) -> Res {
    let short_name = challenge.short_name();
    info!(
        "Removing file generation/images/{}/{}/{}.png",
        short_name, week_num, message_id
    );
    remove_file(format!(
        "generation/images/{short_name}/{week_num}/{message_id}.png"
    ))
    .await?;
    Ok(())
}

/// Download a user's profile picture and save it to the right location.
pub async fn download_pfp(member: &Member) -> Res {
    let response = reqwest::get(member.face()).await?;
    let content = response.bytes().await?;
    let extension = "png";
    let user_id = member.user.id;
    let prefix = format!("generation/images/pfp/{user_id}");
    let location = format!("{}.{}", prefix, extension);
    info!("Saving pfp file to {}", location);
    let mut file = File::create(&location).await?;
    file.write_all(&content).await?;
    info!("Converting {} to png...", location);
    convert_image_type(&prefix, extension, "png").await?;
    Ok(())
}

/// Use `imagemagick` to convert an image to a different filetype
pub async fn convert_image_type(prefix: &str, current_ext: &str, desired_ext: &str) -> Res {
    let mut command = tokio::process::Command::new("convert");
    // with the [0] in the first argument we ensure that a gif will have only the
    // first frame taken.
    command.arg(format!("{prefix}.{current_ext}[0]"));
    command.arg(format!("{prefix}.{desired_ext}"));
    command.kill_on_drop(true);
    info!("Running shell command {:?}", command);
    let res = command.spawn()?.wait().await?;
    if !res.success() {
        return Err("Failed to convert with imagemagick.".into());
    }
    // A natural question would be why we would even bother running a conversion if
    // the original and desired file extensions match. The answer is that the file
    // extension may not always match the actual underlying file type, but in this
    // case `imagemagick` will still detect the correct file type and perform the
    // conversion correctly. In this case the converted file will of course have the
    // same file name as the original, overwriting it, so we needn't remove it.
    // We exploit this in download_submission, naming a file with ".png" regardless
    // of what it actually is, then converting it to a real png.
    if current_ext != desired_ext {
        info!("Removing original file {}.{}", prefix, current_ext);
        remove_file(format!("{prefix}.{current_ext}")).await?;
    }
    Ok(())
}
