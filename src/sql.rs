use crate::server_data::{AMBI_INTERVAL, GLYPH_INTERVAL};
use crate::{info_sync, Error, Res, ResT};
use chrono::{Duration, NaiveDateTime};
use const_format::formatcp;
use poise::serenity_prelude::{Member, MessageId, UserId};
use sqlx::migrate::MigrateDatabase;
use sqlx::{FromRow, Sqlite, SqlitePool};
use std::str::FromStr;

pub const DB_PATH: &str = "glyfi.db";

/// Data associated with a given glyph/ambi prompt
#[derive(Clone, Debug, PartialEq)]
pub struct PromptData {
    pub challenge: Challenge,
    pub prompt: String,
    pub size_percentage: Option<u16>,
    pub custom_duration: Option<u16>,
    pub is_special: Option<bool>,
    pub extra_announcement_text: Option<String>,
}

/// What challenge a submission belongs to.
#[derive(Copy, Clone, Debug, PartialEq, poise::ChoiceParameter)]
#[repr(u8)]
pub enum Challenge {
    Glyph = 0,
    Ambigram = 1,
}

impl Challenge {
    pub fn raw(self) -> u8 {
        self as _
    }

    pub fn short_name(self) -> String {
        match self {
            Challenge::Glyph => "glyph".to_owned(),
            Challenge::Ambigram => "ambi".to_owned(),
        }
    }
    pub fn announcement_image_path(self) -> String {
        let name = match self {
            Challenge::Glyph => "glyph_announcement",
            Challenge::Ambigram => "ambigram_announcement",
        };

        return format!("./generation/{}.png", name);
    }
    pub fn default_duration(self) -> Duration {
        match self {
            Challenge::Glyph => GLYPH_INTERVAL,
            Challenge::Ambigram => AMBI_INTERVAL,
        }
    }
}

impl FromStr for Challenge {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(Challenge::Glyph),
            "1" => Ok(Challenge::Ambigram),
            id => Err(format!("Unknown challenge ID '{:?}'", id).into()),
        }
    }
}

impl From<i64> for Challenge {
    fn from(i: i64) -> Self {
        match i {
            0 => Challenge::Glyph,
            1 => Challenge::Ambigram,
            _ => panic!("Invalid challenge ID {}", i),
        }
    }
}

/// Determines what kind of actions should be taken in a week.
///
/// Every week, we need to perform the following actions for
/// each challenge:
///
/// - Make an announcement post that describes that week’s challenge.
/// - Post a panel containing all submissions from the previous week.
/// - Post the top 3 submissions from the week before that.
///
/// Some weeks, however, are special in that we don’t want to take
/// one or more of those actions. A week can either be ‘regular’ or
/// ‘special’.
///
/// At the ‘beginning’ of the week (that is, the day the announcement
/// is made) we need to:
///
/// - Make a new announcement post for the current week, unless this
///   week is special.
///
/// - Post a panel containing all submissions from the previous week,
///   unless that week was special.
///
/// - Post the top three from the week before the last.

/// Profile for a user.
#[derive(Clone, Debug)]
pub struct UserProfileData {
    pub nickname: Option<String>,

    /// Number of 1st, 2nd, 3rd place finishes in the Glyphs Challenge.
    pub glyphs_first: i64,
    pub glyphs_second: i64,
    pub glyphs_third: i64,

    /// Number of 1st, 2nd, 3rd place finishes in the Ambigram Challenge.
    pub ambigrams_first: i64,
    pub ambigrams_second: i64,
    pub ambigrams_third: i64,

    /// Highest ranking in either challenge.
    pub highest_ranking_glyphs: i64,
    pub highest_ranking_ambigrams: i64,

    /// Number of submissions.
    pub glyphs_submissions: i64,
    pub ambigrams_submissions: i64,
}

#[derive(Clone, Debug, FromRow)]
pub struct WeekInfo {
    pub challenge: Challenge,
    pub week: i64,
    pub prompt: String,
    pub target_start_time: NaiveDateTime,
    pub target_end_time: NaiveDateTime,
    pub actual_start_time: NaiveDateTime,
    pub actual_end_time: NaiveDateTime,
    pub is_special: bool,
}

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
            target_start_time INTEGER,
            target_end_time INTEGER,
            actual_start_time INTEGER,
            actual_end_time INTEGER,
            is_special INTEGER,
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

/// Get the current week.
pub async fn get_current_week(challenge: Challenge) -> ResT<i64> {
    sqlx::query_scalar("SELECT week FROM current_week WHERE challenge = ? LIMIT 1;")
        .bind(challenge.raw() as i64)
        .fetch_one(pool())
        .await
        .map_err(|e| format!("Failed to get current week: {}", e).into())
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
    let (id1, prompt_data1) = get_prompt(challenge, pos1).await?;
    let (id2, prompt_data2) = get_prompt(challenge, pos2).await?;
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
    let prompts = get_prompts(challenge).await?;
    let name = poise::ChoiceParameter::name(&challenge);
    Ok(prompts
        .get(position.checked_sub(1).ok_or("There is no 0th prompt.")?)
        .ok_or(format!("There is no {position}th prompt in queue {name}."))?
        .0)
}

/// Get the id and data of the nth prompt in a given queue
pub async fn get_prompt(challenge: Challenge, position: usize) -> ResT<(i64, PromptData)> {
    let id = get_prompt_id(challenge, position).await?;
    let res: (i64, String, Option<u16>, Option<u16>, Option<bool>, Option<String>) = 
        sqlx::query_as("SELECT challenge, prompt, size_percentage, custom_duration, is_special, extra_announcement_text FROM prompts WHERE rowid = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool())
        .await
        .map_err(Error::from)
        .and_then(|r| {
            r.ok_or_else(|| format!("No prompt with id {}", id).into())
        })?;
    Ok((
        id,
        PromptData {
            challenge,
            prompt: res.1,
            size_percentage: res.2,
            custom_duration: res.3,
            is_special: res.4,
            extra_announcement_text: res.5,
        },
    ))
}

/// Get all prompts for a challenge, together with their ids in the db table.
pub async fn get_prompts(challenge: Challenge) -> ResT<Vec<(i64, PromptData)>> {
    sqlx::query_as("SELECT rowid, prompt, size_percentage, custom_duration, is_special, extra_announcement_text FROM prompts WHERE challenge = ? ORDER BY rowid ASC")
        .bind(challenge.raw())
        .fetch_all(pool())
        .await
        .map_err(|e| e.into())
        .map(|x| x.into_iter()
            .map(|(a, b, c, d, e, f): (i64, String, Option<u16>, Option<u16>, Option<bool>, Option<String>)| 
            (a, PromptData {challenge: challenge, prompt: b, size_percentage: c, custom_duration: d,
                is_special: e, extra_announcement_text: f }))
            .collect())
}

/// Get stats for a week.
pub async fn get_week_info(week: i64, challenge: Challenge) -> ResT<WeekInfo> {
    sqlx::query_as(
        r#"SELECT week, challenge, prompt, target_start_time, target_end_time, actual_start_time, actual_end_time, is_special FROM weeks WHERE week = ? AND challenge = ? LIMIT 1; "#)
        .bind(week)
        .bind(challenge.raw() as i64)
        .fetch_optional(pool())
        .await
        .map_err(|e| e.to_string())
        .map(|x| { 
            x.map(|y: (i64, i64, String, i64, i64, i64, i64, bool)|
             WeekInfo { week: y.0, challenge: y.1.into(), prompt: y.2, 
                target_start_time: NaiveDateTime::from_timestamp(y.4, 0), target_end_time: NaiveDateTime::from_timestamp(y.5, 0),
                actual_start_time: NaiveDateTime::from_timestamp(y.4, 0), actual_end_time: NaiveDateTime::from_timestamp(y.4, 0),
                 is_special: y.7}) 
        })
        .map(|x| x.ok_or("There is no such week in the database.".into()))?
}

/// Inserts a week into the db or modifies it if it's already there.
pub async fn insert_or_modify_week(week_info: WeekInfo) -> Res {
    sqlx::query(r#"
    INSERT INTO weeks (week, challenge, prompt, target_start_time, target_end_time, actual_start_time, actual_end_time, is_special) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
    ON CONFLICT (week, challenge) DO UPDATE SET (prompt, target_start_time, target_end_time, actual_start_time, actual_end_time, is_special) = (?3, ?4, ?5, ?6, ?7, ?8);
"#)
    .bind(week_info.week)
    .bind(week_info.challenge.raw() as i64)
    .bind(week_info.prompt)
    .bind(week_info.target_start_time.timestamp())
    .bind(week_info.target_end_time.timestamp())
    .bind(week_info.actual_start_time.timestamp())
    .bind(week_info.actual_end_time.timestamp())
    .bind(week_info.is_special)
    .execute(pool())
    .await
    .map(|_| ())
    .map_err(|e| e.into())
}
