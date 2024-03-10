use std::str::FromStr;
use const_format::formatcp;
use poise::serenity_prelude::{MessageId, UserId};
use sqlx::migrate::MigrateDatabase;
use sqlx::{FromRow, Sqlite, SqlitePool};
use crate::{Error, info_sync, Res};

pub const DB_PATH: &str = "glyfi.db";

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

    pub fn announcement_image_path(self) -> String {
        let name = match self {
            Challenge::Glyph => "glyph_announcement",
            Challenge::Ambigram => "ambigram_announcement",
        };

        return format!("./weekly_challenges/{}.png", name);
    }
}

impl FromStr for Challenge {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(Challenge::Glyph),
            "1" => Ok(Challenge::Ambigram),
            id => Err(format!("Unknown challenge ID '{:?}'", id).into())
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
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Week {
    Regular = 0,
    Special = 1,
}

impl Week {
    pub fn raw(self) -> u8 {
        self as _
    }
}

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
    pub week: i64,
    pub glyph_challenge_kind: Option<i8>,
    pub ambigram_challenge_kind: Option<i8>,
    pub glyph_prompt: Option<String>,
    pub ambigram_prompt: Option<String>,
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
    if let Some(pool) = __GLYFI_DB_POOL.as_ref() { pool.close().await; }
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
    sqlx::query(r#"
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
    "#).execute(pool()).await.unwrap();

    // Cached user profile data (excludes current week, obviously).
    sqlx::query(r#"
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
    "#).execute(pool()).await.unwrap();

    // The current week. This is a table with a single entry.
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS current_week (
            week INTEGER NOT NULL
        ) STRICT;
    "#).execute(pool()).await.unwrap();

    // Prevent inserting additional weeks.
    sqlx::query(r#"
        CREATE TRIGGER IF NOT EXISTS current_week_insertion
        BEFORE INSERT ON current_week
        WHEN (SELECT COUNT(*) FROM current_week) > 0
        BEGIN
            SELECT RAISE(ABORT, "current_week table must not contain more than one entry!");
        END;
    "#).execute(pool()).await.unwrap();

    // The user is expected to set this manually, but ensure it exists. This
    // is allowed to fail due to the trigger above.
    let _ = sqlx::query("INSERT OR IGNORE INTO current_week (week) VALUES (0)").execute(pool()).await;

    // Table that stores what weeks are/were regular or special.
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS weeks (
            week INTEGER PRIMARY KEY, -- Week number.

            -- See Week enum.
            glyph_challenge_kind INTEGER,
            ambigram_challenge_kind INTEGER,

            -- Prompts.
            glyph_prompt TEXT,
            ambigram_prompt TEXT,

            -- Message ID of the announcement message.
            glyph_announcement_message INTEGER,
            ambigram_announcement_message INTEGER,

            -- Message ID of the submissions panel.
            glyph_panel_message INTEGER,
            ambigram_panel_message INTEGER,

            -- Message ID of the first hall of fame message.
            glyph_hof_message INTEGER,
            ambigram_hof_message INTEGER
        ) STRICT;
    "#).execute(pool()).await.unwrap();

    // Table that stores future prompts.
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS prompts (
            challenge INTEGER NOT NULL,
            prompt TEXT NOT NULL
        ) STRICT;
    "#).execute(pool()).await.unwrap();
}

/// Add a submission.
pub async fn add_submission(
    message: MessageId,
    challenge: Challenge,
    author: UserId,
    link: &str,
) -> Res {
    sqlx::query(r#"
        INSERT INTO submissions (
            message,
            week,
            challenge,
            author,
            link
        ) VALUES (?, ?, ?, ?, ?);
    "#)
        .bind(message.get() as i64)
        .bind(current_week().await?)
        .bind(challenge as i64)
        .bind(author.get() as i64)
        .bind(link)
        .execute(pool())
        .await
        .map(|_| ())
        .map_err(|e| e.into())
}

/// Get the current week.
pub async fn current_week() -> Result<i64, Error> {
    sqlx::query_scalar("SELECT week FROM current_week LIMIT 1;")
        .fetch_one(pool())
        .await
        .map_err(|e| format!("Failed to get current week: {}", e).into())
}

/// Get profile data for a user.
pub async fn get_user_profile(user: UserId) -> Result<UserProfileData, Error> {
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

    let first: UserProfileDataFirst = sqlx::query_as(r#"
        SELECT
            nickname,
            glyphs_first, glyphs_second, glyphs_third,
            ambigrams_first, ambigrams_second, ambigrams_third,
            highest_ranking_glyphs, highest_ranking_ambigrams
        FROM users
        WHERE id = ?;
    "#)
        .bind(user.get() as i64)
        .fetch_optional(pool())
        .await
        .map_err(|e| format!("Failed to get user profile data: {}", e))?
        .unwrap_or_default();

    let second: UserProfileDataSecond = sqlx::query_as(formatcp!(r#"
        SELECT
            SUM(IIF(challenge = {}, 1, 0)) as glyphs_submissions,
            SUM(IIF(challenge = {}, 1, 0)) as ambigrams_submissions
        FROM submissions
        WHERE author = ?
        GROUP BY author;
    "#, Challenge::Glyph as i64, Challenge::Ambigram as i64))
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

/// Remove a submission for the current week.
pub async fn remove_submission(message: MessageId, challenge: Challenge) -> Res {
    sqlx::query(r#"
        DELETE FROM submissions
        WHERE message = ?
        AND week = ?
        AND challenge = ?;
    "#)
        .bind(message.get() as i64)
        .bind(current_week().await?)
        .bind(challenge as i64)
        .execute(pool())
        .await
        .map(|_| ())
        .map_err(|e| e.into())
}

/// Set a user’s nickname.
pub async fn set_nickname(user: UserId, name: &str) -> Res {
    sqlx::query(r#"
        INSERT INTO users (id, nickname) VALUES (?1, ?2)
        ON CONFLICT (id) DO UPDATE SET nickname = ?2;
    "#)
        .bind(user.get() as i64)
        .bind(name)
        .execute(pool())
        .await
        .map(|_| ())
        .map_err(|e| e.into())
}

/// Set the prompt for a challenge and week.
/// Returns the id of the prompt in the DB.
pub async fn add_prompt(challenge: Challenge, prompt: &str) -> Result<i64, Error> {
    sqlx::query_scalar("INSERT INTO prompts (challenge, prompt) VALUES (?, ?) RETURNING rowid")
        .bind(challenge.raw())
        .bind(prompt)
        .fetch_one(pool())
        .await
        .map_err(|e| e.into())
}

/// Delete a prompt.
/// Returns whether a prompt was deleted.
pub async fn delete_prompt(id: i64) -> Result<bool, Error> {
    sqlx::query("DELETE FROM prompts WHERE rowid = ?")
        .bind(id)
        .execute(pool())
        .await
        .map(|r| r.rows_affected() > 0)
        .map_err(|e| e.into())
}


/// Get a prompt by id.
pub async fn get_prompt(id: i64) -> Result<(Challenge, String), Error> {
    let res: (i64, String) = sqlx::query_as("SELECT challenge, prompt FROM prompts WHERE rowid = ? LIMIT 1")
        .bind(id)
        .fetch_optional(pool())
        .await
        .map_err(Error::from)
        .and_then(|r| {
            r.ok_or_else(|| format!("No prompt with id {}", id).into())
        })?;

    Ok((Challenge::from(res.0), res.1))
}


/// Get all prompts for a challenge.
pub async fn get_prompts(challenge: Challenge) -> Result<Vec<(i64, String)>, Error> {
    sqlx::query_as("SELECT rowid, prompt FROM prompts WHERE challenge = ? ORDER BY rowid ASC")
        .bind(challenge.raw())
        .fetch_all(pool())
        .await
        .map_err(|e| e.into())
}

/// Get stats for a week.
pub async fn weekinfo(week: Option<u64>) -> Result<WeekInfo, Error> {
    let week = match week {
       Some(w) => w as i64,
       None => current_week().await?,
    };

    sqlx::query_as(r#"
        SELECT * FROM weeks WHERE week = ? LIMIT 1;
    "#)
        .bind(week)
        .fetch_optional(pool())
        .await
        .map_err(|e| format!("Failed to get week info: {}", e))?
        .ok_or_else(|| format!("No info for week {}", week).into())
}