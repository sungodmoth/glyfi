use crate::server_data::{AMBI_INTERVAL, GLYPH_INTERVAL};
use crate::types::{Challenge, PromptData, Timestamp, UserProfileData, WeekInfo};
use crate::{info, info_sync, Error, Res, ResT};
use chrono::{DateTime, Duration, Utc};
use const_format::formatcp;
use poise::serenity_prelude::{Member, MessageId, UserId};
use poise::ChoiceParameter;
use sqlx::migrate::MigrateDatabase;
use sqlx::{FromRow, Sqlite, SqlitePool};
use std::str::FromStr;
use std::thread::current;

pub const DB_PATH: &str = "glyfi.db";

static mut __GLYFI_DB_POOL: Option<SqlitePool> = None;

/// Get the global sqlite connexion pool.
fn pool() -> &'static SqlitePool {
    unsafe { __GLYFI_DB_POOL.as_ref().unwrap() }
}

/*/// Merge the DB into one file.
pub async fn truncate_wal() {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(pool()).await.unwrap();
}
*/

/// Only intended to be called by [`terminate()`].
pub async unsafe fn __glyfi_fini_db() {
    if let Some(pool) = __GLYFI_DB_POOL.as_ref() {
        pool.close().await;
    }
}

/// Only intended to be called by main().
pub async unsafe fn __glyfi_init_db() {
    // Create the database if it doesn’t exist yet.
    info_sync!("Initialising sqlite db...");
    if let Err(e) = Sqlite::create_database(DB_PATH).await {
        panic!("Failed to create sqlite db: {}", e);
    }

    // Create DB connexion.
    __GLYFI_DB_POOL = Some(SqlitePool::connect(DB_PATH).await.unwrap());

    // Create submissions table.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS submissions (
            message INTEGER, -- Message ID of the submission.
            week INTEGER NOT NULL, -- This is just an integer.
            challenge INTEGER NOT NULL, -- See Challenge enum.
            author INTEGER NOT NULL, -- Discord user ID of the author.
            link TEXT NOT NULL, -- Link to the submission.
            time INTEGER NOT NULL DEFAULT (unixepoch()), -- Time of submission.
            votes INTEGER NOT NULL DEFAULT 0, -- Number of votes.
            PRIMARY KEY (message, week, challenge)
        ) STRICT;
    "#,
    )
    .execute(pool())
    .await
    .unwrap();

    // Cached user profile data (excludes current week, obviously).
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY, -- Discord user ID.
            nickname TEXT, -- Nickname.

            -- Number of 1st, 2nd, 3rd place finishes in the Glyphs Challenge.
            glyphs_first INTEGER NOT NULL DEFAULT 0,
            glyphs_second INTEGER NOT NULL DEFAULT 0,
            glyphs_third INTEGER NOT NULL DEFAULT 0,

            -- Number of 1st, 2nd, 3rd place finishes in the Ambigram Challenge.
            ambigrams_first INTEGER NOT NULL DEFAULT 0,
            ambigrams_second INTEGER NOT NULL DEFAULT 0,
            ambigrams_third INTEGER NOT NULL DEFAULT 0,

            -- Highest ranking in either challenge.
            highest_ranking_glyphs INTEGER NOT NULL DEFAULT 0,
            highest_ranking_ambigrams INTEGER NOT NULL DEFAULT 0
        ) STRICT;
    "#,
    )
    .execute(pool())
    .await
    .unwrap();

    // The current week. This is a table with a single entry.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS current_week (
            challenge INTEGER NOT NULL PRIMARY KEY,
            week INTEGER NOT NULL
        ) STRICT;
    "#,
    )
    .execute(pool())
    .await
    .unwrap();

    let _ = sqlx::query("INSERT OR IGNORE INTO current_week (challenge, week) VALUES (0, 0)")
        .execute(pool())
        .await;
    let _ = sqlx::query("INSERT OR IGNORE INTO current_week (challenge, week) VALUES (1, 0)")
        .execute(pool())
        .await;

    // Table that stores what weeks are/were regular or special.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS weeks (
            week INTEGER,
            challenge INTEGER NOT NULL,
            prompt TEXT NOT NULL,
            size_percentage INTEGER NOT NULL,
            target_start_time INTEGER,
            target_end_time INTEGER,
            actual_start_time INTEGER,
            actual_end_time INTEGER,
            is_special INTEGER,
            num_subs INTEGER,
            poll_message_id INTEGER,
            second_poll_message_id INTEGER,
            PRIMARY KEY (week, challenge)
        ) STRICT;
    "#,
    )
    .execute(pool())
    .await
    .unwrap();

    // Table that stores future prompts.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS prompts (
            challenge INTEGER NOT NULL,
            prompt TEXT NOT NULL,
            size_percentage INTEGER,
            custom_duration INTEGER,
            is_special INTEGER,
            extra_announcement_text TEXT
        ) STRICT;
        "#,
    )
    .execute(pool())
    .await
    .unwrap();

    // Table that stores votes. `votes` is an i64 with bitfields for each submission.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS votes (
            challenge INTEGER NOT NULL,
            week INTEGER,
            user INTEGER,
            votes INTEGER,
            PRIMARY KEY(challenge, week, user)
        ) STRICT;
        "#,
    )
    .execute(pool())
    .await
    .unwrap();
}

/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

/// Add a user to the database.
pub async fn register_user(member: Member) -> Res {
    sqlx::query(
        r#"
    INSERT INTO users (id, nickname) VALUES (?, ?);
        "#,
    )
    .bind(member.user.id.get() as i64)
    .bind(member.nick.unwrap_or(member.user.name))
    .execute(pool())
    .await
    .map(|_| ())
    .map_err(|e| e.into())
}

/// Checks whether user is in the database.
pub async fn check_user(member: &Member) -> ResT<bool> {
    sqlx::query(r#"SELECT id, nickname FROM users WHERE id = ? LIMIT 1"#)
        .bind(member.user.id.get() as i64)
        .fetch_optional(pool())
        .await
        .map(|x| x.is_some())
        .map_err(|e| e.into())
}

/// Checks whether submission is in the database.
pub async fn check_submission(message_id: MessageId) -> ResT<bool> {
    sqlx::query(r#"SELECT message FROM submissions WHERE message = ? LIMIT 1"#)
        .bind(message_id.get() as i64)
        .fetch_optional(pool())
        .await
        .map(|x| x.is_some())
        .map_err(|e| e.into())
}

/// Add a submission to the database.
pub async fn register_submission(
    message: MessageId,
    challenge: Challenge,
    author: UserId,
    link: &str,
    week_num: i64,
) -> Res {
    sqlx::query(
        r#"
    INSERT INTO submissions (
        message,
        week,
        challenge,
            author,
            link
        ) VALUES (?, ?, ?, ?, ?);
        "#,
    )
    .bind(message.get() as i64)
    .bind(week_num)
    .bind(challenge as i64)
    .bind(author.get() as i64)
    .bind(link)
    .execute(pool())
    .await
    .map(|_| ())
    .map_err(|e| e.into())
}
/// Remove a submission from the database.
pub async fn deregister_submission(message: MessageId, challenge: Challenge, week_num: i64) -> Res {
    sqlx::query(
        r#"
            DELETE FROM submissions
            WHERE message = ?
            AND week = ?
            AND challenge = ?;
        "#,
    )
    .bind(message.get() as i64)
    .bind(week_num)
    .bind(challenge as i64)
    .execute(pool())
    .await
    .map(|_| ())
    .map_err(|e| e.into())
}

/// Get all the submissions from a particular week and challenge, along with the users who posted them.
pub async fn get_submissions(challenge: Challenge, week_num: i64) -> ResT<Vec<(UserId, MessageId)>> {
    sqlx::query_as("SELECT author, message FROM submissions WHERE challenge = ? AND week = ? ORDER BY message ASC")
        .bind(challenge.raw() as i16)
        .bind(week_num)
        .fetch_all(pool())
        .await
        .map_err(|e| e.into())
        .map(|x| x.into_iter().map(|(a,b): (i64, i64)| (UserId::new(a as u64), MessageId::new(b as u64))).collect())
}

/// Get the current week.
pub async fn get_current_week(challenge: Challenge) -> ResT<i64> {
    sqlx::query_scalar("SELECT week FROM current_week WHERE challenge = ? LIMIT 1;")
        .bind(challenge.raw() as i64)
        .fetch_one(pool())
        .await
        .map_err(|e| format!("Failed to get current week: {}", e).into())
}

/// Set the current week. Returns whether the operation was successful.
pub async fn set_current_week(challenge: Challenge, week_num: i64) -> ResT<bool> {
    sqlx::query("UPDATE current_week SET week = ? WHERE challenge = ?")
        .bind(week_num)
        .bind(challenge.raw() as i64)
        .execute(pool())
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.into())
}

/// Get profile data for a user.
pub async fn get_user_profile(user: UserId) -> ResT<UserProfileData> {
    #[derive(Default, FromRow)]
    pub struct UserProfileDataFirst {
        pub nickname: Option<String>,
        pub glyphs_first: i64,
        pub glyphs_second: i64,
        pub glyphs_third: i64,
        pub ambigrams_first: i64,
        pub ambigrams_second: i64,
        pub ambigrams_third: i64,
        pub highest_ranking_glyphs: i64,
        pub highest_ranking_ambigrams: i64,
    }

    #[derive(Default, FromRow)]
    pub struct UserProfileDataSecond {
        pub glyphs_submissions: i64,
        pub ambigrams_submissions: i64,
    }

    let first: UserProfileDataFirst = sqlx::query_as(
        r#"
        SELECT
            nickname,
            glyphs_first, glyphs_second, glyphs_third,
            ambigrams_first, ambigrams_second, ambigrams_third,
            highest_ranking_glyphs, highest_ranking_ambigrams
        FROM users
        WHERE id = ?;
    "#,
    )
    .bind(user.get() as i64)
    .fetch_optional(pool())
    .await
    .map_err(|e| format!("Failed to get user profile data: {}", e))?
    .unwrap_or_default();

    let second: UserProfileDataSecond = sqlx::query_as(formatcp!(
        r#"
        SELECT
            SUM(IIF(challenge = {}, 1, 0)) as glyphs_submissions,
            SUM(IIF(challenge = {}, 1, 0)) as ambigrams_submissions
        FROM submissions
        WHERE author = ?
        GROUP BY author;
    "#,
        Challenge::Glyph as i64,
        Challenge::Ambigram as i64
    ))
    .bind(user.get() as i64)
    .fetch_optional(pool())
    .await
    .map_err(|e| format!("Failed to get user profile data: {}", e))?
    .unwrap_or_default();

    Ok(UserProfileData {
        nickname: first.nickname,

        glyphs_first: first.glyphs_first,
        glyphs_second: first.glyphs_second,
        glyphs_third: first.glyphs_third,

        ambigrams_first: first.ambigrams_first,
        ambigrams_second: first.ambigrams_second,
        ambigrams_third: first.ambigrams_third,

        highest_ranking_glyphs: first.highest_ranking_glyphs,
        highest_ranking_ambigrams: first.highest_ranking_ambigrams,

        glyphs_submissions: second.glyphs_submissions,
        ambigrams_submissions: second.ambigrams_submissions,
    })
}

/// Set a user’s nickname.
pub async fn set_nickname(user: UserId, name: &str) -> Res {
    sqlx::query(
        r#"
        INSERT INTO users (id, nickname) VALUES (?1, ?2)
        ON CONFLICT (id) DO UPDATE SET nickname = ?2;
    "#,
    )
    .bind(user.get() as i64)
    .bind(name)
    .execute(pool())
    .await
    .map(|_| ())
    .map_err(|e| e.into())
}

/// Set the prompt for a challenge and week.
/// Returns the id of the prompt in the DB.
pub async fn add_prompt(prompt_data: &PromptData) -> ResT<i64> {
    sqlx::query_scalar("INSERT INTO prompts (challenge, prompt, size_percentage, custom_duration, is_special, extra_announcement_text) VALUES (?, ?, ?, ?, ?, ?) RETURNING rowid")
        .bind(prompt_data.challenge.raw())
        .bind(&prompt_data.prompt)
        .bind(prompt_data.size_percentage.map(|x| x as i32))
        .bind(prompt_data.custom_duration.map(|x| x as i32))
        .bind(prompt_data.is_special)
        .bind(&prompt_data.extra_announcement_text)
        .fetch_one(pool())
        .await
        .map_err(|e| e.into())
}

/// Swaps two prompts within a given queue. Returns whether the operation was successful
pub async fn swap_prompts(challenge: Challenge, pos1: usize, pos2: usize) -> ResT<bool> {
    let (id1, prompt_data1) = get_prompt_id_data(challenge, pos1).await?;
    let (id2, prompt_data2) = get_prompt_id_data(challenge, pos2).await?;
    Ok(edit_prompt(id1, &prompt_data2).await? & edit_prompt(id2, &prompt_data1).await?)
}

/// Delete the nth prompt in a given queue. Returns whether the operation was successful.
pub async fn delete_prompt(challenge: Challenge, position: usize) -> ResT<bool> {
    let id = get_prompt_id(challenge, position).await?;
    sqlx::query("DELETE FROM prompts WHERE rowid = ?")
        .bind(id)
        .execute(pool())
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.into())
}

/// Replaces the prompt with given id with the data specified. Returns whether the operation was successful.
pub async fn edit_prompt(id: i64, prompt_data: &PromptData) -> ResT<bool> {
    sqlx::query("UPDATE prompts SET challenge = ?, prompt = ?, size_percentage = ?, custom_duration = ?, is_special = ?, extra_announcement_text = ? WHERE rowid = ?")
        .bind(prompt_data.challenge.raw())
        .bind(&prompt_data.prompt)
        .bind(prompt_data.size_percentage.map(|x| x as i32))
        .bind(prompt_data.custom_duration.map(|x| x as i32))
        .bind(prompt_data.is_special)
        .bind(&prompt_data.extra_announcement_text)
        .bind(id)
        .execute(pool())
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.into())
}

/// Get the id in the db table of the nth prompt in a given queue.
pub async fn get_prompt_id(challenge: Challenge, position: usize) -> ResT<i64> {
    if position < 1 { return Err("Invalid position value.".into()); }
    sqlx::query_as("SELECT rowid FROM prompts WHERE challenge = ? ORDER BY rowid ASC LIMIT ?")
        .bind(challenge.raw())
        .bind(position as i64)
        .fetch_all(pool())
        .await
        .map(|x: Vec<(i64,)>| x.into_iter().skip(position - 1).
                last().ok_or("No prompt found at given position.".into()))?
        .map(|x| x.0)
}

/// Get the data of the nth prompt in a given queue
pub async fn get_prompt_data(challenge: Challenge, position: usize) -> ResT<PromptData> {
    get_prompts(challenge).await?.get(position.checked_sub(1).ok_or::<Error>("0 is not a valid prompt position.".into())?)
    .cloned().ok_or(format!("There is no prompt at position {position} in challenge {}.", challenge.name()).into())
}

/// Get the id and data of the nth prompt in a given queue
pub async fn get_prompt_id_data(challenge: Challenge, position: usize) -> ResT<(i64,PromptData)> {
    Ok((get_prompt_id(challenge, position).await?, get_prompt_data(challenge, position).await?))
}

/// Get all prompts for a challenge, together with their ids in the db table.
pub async fn get_prompts(challenge: Challenge) -> ResT<Vec<PromptData>> {
    sqlx::query_as("SELECT * FROM prompts WHERE challenge = ? ORDER BY rowid ASC")
        .bind(challenge.raw())
        .fetch_all(pool())
        .await
        .map_err(|e| e.into())
}

/// Get stats for a week.
pub async fn get_week_info(week_num: i64, challenge: Challenge) -> ResT<WeekInfo> {
    sqlx::query_as(
        r#"SELECT * FROM weeks WHERE week = ? AND challenge = ? LIMIT 1; "#)
        .bind(week_num)
        .bind(challenge.raw() as i64)
        .fetch_optional(pool())
        .await
        .map_err(|e| e.to_string())
        .map(|x| x.ok_or(format!("There is no week {week_num} for challenge {challenge:?} in the database.").into()))?
}

/// Inserts a week into the db or modifies it if it's already there.
pub async fn insert_or_modify_week(week_info: WeekInfo) -> Res {
    // there must be a better way to do this
    // like surely
    sqlx::query(r#"
    INSERT INTO weeks (week, challenge, prompt, size_percentage, target_start_time, target_end_time, actual_start_time, actual_end_time, is_special, num_subs, poll_message_id, second_poll_message_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
    ON CONFLICT (week, challenge) DO UPDATE SET (prompt, size_percentage, target_start_time, target_end_time, actual_start_time, actual_end_time, is_special, num_subs, poll_message_id, second_poll_message_id) = (?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);
"#)
        .bind(week_info.week)
        .bind(week_info.challenge.raw() as i64)
        .bind(week_info.prompt)
        .bind(week_info.size_percentage)
        .bind(week_info.target_start_time.0.timestamp())
        .bind(week_info.target_end_time.0.timestamp())
        .bind(week_info.actual_start_time.0.timestamp())
        .bind(week_info.actual_end_time.0.timestamp())
        .bind(week_info.is_special)
        .bind(week_info.num_subs)
        .bind(week_info.poll_message_id.0.map(|x| x.get() as i64))
        .bind(week_info.second_poll_message_id.0.map(|x| x.get() as i64))
        .execute(pool())
        .await
        .map(|_| ())
        .map_err(|e| e.into())
}

/// Updates the `votes` table with one user's vote. Returns whether the operation was successful.
pub async fn register_vote(challenge: Challenge, week_num: i64, user_id: UserId, sub_num: i64) -> ResT<bool> {
    let mut votes: i64 = sqlx::query_scalar("SELECT votes FROM votes WHERE challenge = ? AND week = ? AND user = ? LIMIT 1")
        .bind(challenge.raw() as i16)
        .bind(week_num)
        .bind(user_id.get() as i64)
        .fetch_optional(pool())
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or(0);
    votes ^= (1 << sub_num);
    sqlx::query(r#"INSERT INTO votes (challenge, week, user, votes) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT (challenge, week, user) DO UPDATE SET votes = ?4;"#,)
        .bind(challenge.raw() as i16)
        .bind(week_num)
        .bind(user_id.get() as i64)
        .bind(votes)
        .execute(pool())
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.into())
}

/// Reads all the votes from a user for a particular challenge and week. Processes the bitstring into an actual list.
pub async fn get_votes(challenge: Challenge, week_num: i64, user_id: UserId, num_subs: i64) -> ResT<Vec<i64>> {
    info!("{}, {}, {}, {}", challenge.short_name(), week_num, user_id, num_subs);
    let votes: i64 = sqlx::query_scalar("SELECT votes FROM votes WHERE challenge = ? AND week = ? AND user = ? LIMIT 1")
        .bind(challenge.raw() as i16)
        .bind(week_num)
        .bind(user_id.get() as i64)
        .fetch_optional(pool())
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or(0);
    Ok((0..num_subs).filter(|x| (1 << x) & votes != 0).collect())
}

/// Do the necessary database operations to roll over to next week.
pub async fn rollover_week(challenge: Challenge, current_week: i64, next_prompt: &PromptData, current_time: Timestamp, target_start_time: Timestamp, target_end_time: Timestamp, num_subs: i64, poll_message_id: MessageId, second_poll_message_id: Option<MessageId>) -> Res {
    let mut current_week_info = get_week_info(current_week, challenge).await?;
    current_week_info.actual_end_time = current_time;
    current_week_info.num_subs = num_subs;
    current_week_info.poll_message_id = Some(poll_message_id).into();
    current_week_info.second_poll_message_id = second_poll_message_id.into();
    let next_week_info = WeekInfo { challenge, week: current_week + 1, prompt: next_prompt.prompt.clone(), size_percentage: next_prompt.size_percentage.unwrap_or(100),
        target_start_time,  target_end_time, actual_start_time: current_time, actual_end_time: DateTime::<Utc>::UNIX_EPOCH.into(), is_special: next_prompt.is_special.unwrap_or(false), num_subs: 0, poll_message_id: None.into(), second_poll_message_id: None.into()};
    insert_or_modify_week(current_week_info).await?;
    insert_or_modify_week(next_week_info).await?;
    set_current_week(challenge, current_week + 1).await?;
    Ok(())
}

/// For a prompt in any queue, forecast based on current parameters when that prompt will be used and
/// what the week number will be. Allows for accurate image preview. Takes negative index.
pub async fn forecast_prompt_details(challenge: Challenge, mut position: i64) -> ResT<(i64, Timestamp, Timestamp)> {
    let queue = get_prompts(challenge).await?;
    info!("{:?}", queue);
    if position < 0 {
        position += queue.len() as i64 + 1;
    }
    let prompt = queue.get((position as usize).checked_sub(1).ok_or::<Error>("0 is not a valid prompt position.".into())?)
    .ok_or::<Error>(format!("There is no prompt at position {position} in challenge {}.", challenge.name()).into())?;
    let mut week = get_current_week(challenge).await?;
    let current_week_info = get_week_info(week, challenge).await?;
    let mut start_time = current_week_info.target_end_time;
    for pos in 1..position {
        start_time += challenge.default_duration() * queue[(pos as usize) - 1].custom_duration.unwrap_or(1) as i32;
    }
    let end_time = start_time + challenge.default_duration() * (prompt.custom_duration.unwrap_or(1) as i32);
    Ok((week + position, start_time, end_time))
}
