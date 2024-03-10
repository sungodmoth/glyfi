use poise::serenity_prelude::{Attachment, MessageId};
use tokio::{fs::{remove_file, File}, io::AsyncWriteExt};

use crate::{info, sql::Challenge, Res};

/// Download a submission's image file to the file system
pub async fn download_submission(attachment: &Attachment, message_id: MessageId, challenge: Challenge) -> Res {
    let content = attachment.download().await?;
    let short_name = challenge.short_name();
    let extension = attachment.filename.split('.').last().ok_or("File doesn't have an extension.")?;
    let prefix  = format!("generation/images/thisweek/{short_name}/{message_id}");
    let location = format!("{}.{}", prefix, extension);
    info!("Saving submission file to {}", location);
    let mut file = File::create(&location).await?;
    file.write_all(&content).await?;
    info!("Converting {} to png...", location);
    convert_image_type(&prefix, extension, "png").await?;
    Ok(())
}

/// Remove a submission's image file from the file system
pub async fn delete_submission(message_id: MessageId, challenge: Challenge) -> Res {
    let short_name = challenge.short_name();
    info!("Removing file generation/images/thisweek/{}/{}.png", short_name, message_id);
    remove_file(format!("generation/images/thisweek/{short_name}/{message_id}.png")).await?;
    Ok(())
}

/// Use `imagemagick` to convert an image to a different filetype
pub async fn convert_image_type(prefix: &str, current_ext: &str, desired_ext: &str) -> Res {
    let mut command = tokio::process::Command::new("convert");
    command.arg(format!("{prefix}.{current_ext}"));
    command.arg(format!("{prefix}.{desired_ext}"));
    command.kill_on_drop(true);
    info!("Running shell command {:?}", command);
    let res = command.spawn()?.wait().await?;
    if !res.success() { return Err("Failed to convert with imagemagick.".into()); }
    // A natural question would be why we would even bother running a conversion if
    // the original and desired file extensions match. The answer is that the file
    // extension may not always match the actual underlying file type, but in this
    // case `imagemagick` will still detect the correct file type and perform the
    // conversion correctly. In this case the converted file will of course have the
    // same file name as the original, overwriting it, so we needn't remove it.
    if current_ext != desired_ext {
        info!("Removing original file {}.{}", prefix, current_ext);
        remove_file(format!("{prefix}.{current_ext}")).await?;
    }
    Ok(())
}